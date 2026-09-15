use std::path::Path;
use std::time::Duration;

use serde_json::json;

use crate::attachments::LoadedAttachment;
use crate::providers::{GenerationOptions, MediaProvider};
use crate::schema::AudioKind;
use crate::ui;

const DEFAULT_MODEL: &str = "V6";
const POLL_INTERVAL_SECS: u64 = 10;
const MAX_POLL_ATTEMPTS: u32 = 120; // 20 minutes at 10s intervals
const API_BASE: &str = "https://api.sunoapi.org";
const RATE_LIMIT_RETRY_SECS: u64 = 30;

/// Backoff decision for gateway rate limits (HTTP 429). Batch jobs over ~150
/// files hit these; a single retry after a fixed wait clears most of them.
/// Returns Some(delay_secs) only when one retry remains (retries_used == 0).
fn retry_after_rate_limit(status: u16, retries_used: u32) -> Option<u64> {
    if status == 429 && retries_used == 0 {
        Some(RATE_LIMIT_RETRY_SECS)
    } else {
        None
    }
}

/// Music duration must be 10–360 seconds (custom mode only).
fn clamp_music_duration(secs: u32) -> u32 {
    secs.clamp(10, 360)
}

/// Serialize weighting knobs as clean 2-decimal numbers.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Build the submit request (url + JSON body). Routing between the music and
/// sounds endpoints is structural (AudioKind), not name-based.
fn build_request(
    prompt_text: &str,
    model: &str,
    options: &GenerationOptions,
) -> (String, serde_json::Value) {
    let callback_url = options
        .provider_options
        .get("callBackUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("https://httpbin.org/post");

    if options.audio_kind == AudioKind::Sfx {
        // Sounds endpoint: prompt ≤500 chars (enforced upstream via the
        // suno-sfx constraint), no duration field, model passthrough.
        let sound_loop = options
            .provider_options
            .get("soundLoop")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let sound_tempo = options
            .provider_options
            .get("soundTempo")
            .and_then(|v| v.as_u64())
            .unwrap_or(120) as u32;
        let sound_key = options
            .provider_options
            .get("soundKey")
            .and_then(|v| v.as_str())
            .unwrap_or("Any");
        let grab_lyrics = options
            .provider_options
            .get("grabLyrics")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let b = json!({
            "prompt": prompt_text,
            "model": model,
            "soundLoop": sound_loop,
            "soundTempo": sound_tempo,
            "soundKey": sound_key,
            "grabLyrics": grab_lyrics,
            "callBackUrl": callback_url,
        });

        (format!("{}/api/v1/generate/sounds", API_BASE), b)
    } else {
        // Music generation
        let has_style = options
            .provider_options
            .get("style")
            .and_then(|v| v.as_str())
            .is_some();
        let custom_mode = options
            .provider_options
            .get("customMode")
            .and_then(|v| v.as_bool())
            .unwrap_or(has_style || prompt_text.len() > 200);

        let instrumental = options
            .provider_options
            .get("instrumental")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut b = json!({
            "prompt": prompt_text,
            "model": model,
            "customMode": custom_mode,
            "instrumental": instrumental,
            "callBackUrl": callback_url,
        });

        if custom_mode {
            if let Some(style) = options
                .provider_options
                .get("style")
                .and_then(|v| v.as_str())
            {
                b["style"] = json!(style);
            }
            if let Some(title) = options
                .provider_options
                .get("title")
                .and_then(|v| v.as_str())
            {
                b["title"] = json!(title);
            }
            if let Some(pid) = options
                .provider_options
                .get("personaId")
                .and_then(|v| v.as_str())
            {
                b["personaId"] = json!(pid);
            }
            if let Some(pm) = options
                .provider_options
                .get("personaModel")
                .and_then(|v| v.as_str())
            {
                b["personaModel"] = json!(pm);
            }
            // duration is a custom-mode-only knob, clamped to 10–360
            if let Some(dur) = options.duration_seconds {
                b["duration"] = json!(clamp_music_duration(dur.round() as u32));
            }
        }

        if let Some(neg) = options.negative_prompt.as_deref().or_else(|| {
            options
                .provider_options
                .get("negativeTags")
                .and_then(|v| v.as_str())
        }) {
            b["negativeTags"] = json!(neg);
        }

        if let Some(vg) = options
            .provider_options
            .get("vocalGender")
            .and_then(|v| v.as_str())
        {
            b["vocalGender"] = json!(vg);
        }

        for float_key in &["styleWeight", "weirdnessConstraint", "audioWeight"] {
            if let Some(val) = options
                .provider_options
                .get(*float_key)
                .and_then(|v| v.as_f64())
            {
                b[*float_key] = json!(round2(val));
            }
        }

        (format!("{}/api/v1/generate", API_BASE), b)
    }
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    body: &serde_json::Value,
) -> Result<reqwest::Response, reqwest::Error> {
    client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
}

pub struct SunoProvider;

#[async_trait::async_trait]
impl MediaProvider for SunoProvider {
    async fn generate(
        &self,
        prompt_text: &str,
        output_path: &Path,
        api_key: &str,
        options: &GenerationOptions,
        _attachments: &[LoadedAttachment],
    ) -> color_eyre::Result<bool> {
        let model = if options.model.is_empty() || options.model == "default" {
            DEFAULT_MODEL
        } else {
            &options.model
        };

        let (url, body) = build_request(prompt_text, model, options);
        let is_sfx = options.audio_kind == AudioKind::Sfx;

        if options.verbose {
            ui::verbose(&format!("POST {}", url));
            let preview: String = prompt_text.chars().take(120).collect();
            ui::verbose(&format!(
                "Prompt: {}{}",
                preview,
                if prompt_text.len() > 120 { "..." } else { "" }
            ));
            if is_sfx {
                ui::verbose(&format!("Model: {} (sound generation)", model));
            } else {
                ui::verbose(&format!("Model: {}", model));
            }
        }

        let client = reqwest::Client::new();

        // Submit generation request (single 429 retry for batch runs)
        let mut response = match post_json(&client, &url, api_key, &body).await {
            Ok(r) => r,
            Err(e) => {
                ui::fail_msg(&format!("Network error submitting to Suno: {}", e));
                return Ok(false);
            }
        };

        if let Some(wait) = retry_after_rate_limit(response.status().as_u16(), 0) {
            ui::info(&format!(
                "Suno rate limited (429) — waiting {}s before a single retry",
                wait
            ));
            tokio::time::sleep(Duration::from_secs(wait)).await;
            response = match post_json(&client, &url, api_key, &body).await {
                Ok(r) => r,
                Err(e) => {
                    ui::fail_msg(&format!("Network error resubmitting to Suno: {}", e));
                    return Ok(false);
                }
            };
        }

        let status = response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            let body_text = response.text().await.unwrap_or_default();
            color_eyre::eyre::bail!(
                "Suno authentication failed ({}): {}\n  Check your SUNO_API_KEY",
                status.as_u16(),
                &body_text[..body_text.len().min(200)]
            );
        }
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            ui::fail_msg(&format!(
                "Suno API error ({}): {}",
                status.as_u16(),
                &body_text[..body_text.len().min(300)]
            ));
            return Ok(false);
        }

        let result: serde_json::Value = response.json().await?;
        let task_id = result["data"]["taskId"]
            .as_str()
            .ok_or_else(|| color_eyre::eyre::eyre!("No taskId in Suno response: {}", result))?;

        if options.verbose {
            ui::verbose(&format!("Task submitted: {}", task_id));
        }
        ui::info(&format!("Suno task {} — polling for completion", task_id));

        // Poll for completion
        let poll_url = format!(
            "{}/api/v1/generate/record-info?taskId={}",
            API_BASE, task_id
        );

        let mut poll_429_retried = false;
        for attempt in 1..=MAX_POLL_ATTEMPTS {
            tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;

            let poll_get = || async {
                client
                    .get(&poll_url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await
            };

            let mut poll_response = match poll_get().await {
                Ok(r) => r,
                Err(e) => {
                    if options.verbose {
                        ui::verbose(&format!("Poll attempt {} failed: {}", attempt, e));
                    }
                    continue;
                }
            };

            if let Some(wait) =
                retry_after_rate_limit(poll_response.status().as_u16(), poll_429_retried as u32)
            {
                poll_429_retried = true;
                ui::info(&format!(
                    "Suno rate limited (429) on record-info — waiting {}s before a single retry",
                    wait
                ));
                tokio::time::sleep(Duration::from_secs(wait)).await;
                poll_response = match poll_get().await {
                    Ok(r) => r,
                    Err(e) => {
                        if options.verbose {
                            ui::verbose(&format!("Poll retry failed: {}", e));
                        }
                        continue;
                    }
                };
            }

            if !poll_response.status().is_success() {
                if options.verbose {
                    ui::verbose(&format!(
                        "Poll attempt {} returned {}",
                        attempt,
                        poll_response.status()
                    ));
                }
                continue;
            }

            let poll_result: serde_json::Value = poll_response.json().await?;
            let status_str = poll_result["data"]["status"].as_str().unwrap_or("UNKNOWN");

            match status_str {
                "SUCCESS" => {
                    if options.verbose {
                        let preview =
                            serde_json::to_string_pretty(&poll_result).unwrap_or_default();
                        let truncated: String = preview.chars().take(1000).collect();
                        ui::verbose(&format!("Suno SUCCESS response: {}", truncated));
                    }

                    // Try multiple known response shapes
                    let tracks = poll_result["data"]["response"]["sunoData"]
                        .as_array()
                        .or_else(|| poll_result["data"]["response"]["data"].as_array())
                        .or_else(|| poll_result["data"]["data"].as_array());

                    if let Some(tracks) = tracks {
                        if let Some(first) = tracks.first() {
                            let audio_url = first["audioUrl"]
                                .as_str()
                                .or_else(|| first["audio_url"].as_str())
                                .unwrap_or("");
                            if audio_url.is_empty() {
                                ui::fail_msg("Suno returned SUCCESS but no audio_url");
                                return Ok(false);
                            }

                            if options.verbose {
                                if let Some(title) = first["title"].as_str() {
                                    ui::verbose(&format!("Title: {}", title));
                                }
                                if let Some(dur) = first["duration"].as_f64() {
                                    ui::verbose(&format!("Duration: {:.1}s", dur));
                                }
                                if let Some(tags) = first["tags"].as_str() {
                                    ui::verbose(&format!("Tags: {}", tags));
                                }
                            }

                            return self
                                .download_audio(audio_url, output_path, &client, options.verbose)
                                .await;
                        }
                    }

                    ui::fail_msg("Suno returned SUCCESS but no tracks in response");
                    return Ok(false);
                }
                "FAILED" => {
                    ui::fail_msg(&format!("Suno generation failed for task {}", task_id));
                    if options.verbose {
                        let preview =
                            serde_json::to_string_pretty(&poll_result).unwrap_or_default();
                        ui::verbose(&preview[..preview.len().min(500)]);
                    }
                    return Ok(false);
                }
                "PENDING" | "GENERATING" | "TEXT_SUCCESS" | "FIRST_SUCCESS" => {
                    if options.verbose && attempt % 3 == 0 {
                        ui::verbose(&format!(
                            "Still {} (poll {}/{})",
                            status_str, attempt, MAX_POLL_ATTEMPTS
                        ));
                    }
                }
                _ => {
                    if options.verbose {
                        ui::verbose(&format!("Unknown status: {}", status_str));
                    }
                }
            }
        }

        ui::fail_msg(&format!(
            "Suno task {} timed out after {} polls",
            task_id, MAX_POLL_ATTEMPTS
        ));
        Ok(false)
    }

    fn name(&self) -> &str {
        "suno"
    }
}

impl SunoProvider {
    async fn download_audio(
        &self,
        url: &str,
        output_path: &Path,
        client: &reqwest::Client,
        verbose: bool,
    ) -> color_eyre::Result<bool> {
        if verbose {
            ui::verbose(&format!("Downloading: {}", url));
        }

        let resp = client
            .get(url)
            .timeout(Duration::from_secs(120))
            .send()
            .await;

        match resp {
            Ok(response) => {
                if !response.status().is_success() {
                    ui::fail_msg(&format!("Download failed: HTTP {}", response.status()));
                    return Ok(false);
                }

                let bytes = response.bytes().await?;
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(output_path, &bytes)?;
                Ok(true)
            }
            Err(e) => {
                ui::fail_msg(&format!("Download error: {}", e));
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn opts(kind: AudioKind, model: &str) -> GenerationOptions {
        GenerationOptions {
            model: model.to_string(),
            aspect_ratio: None,
            negative_prompt: None,
            provider_options: HashMap::new(),
            verbose: false,
            duration_seconds: None,
            audio_kind: kind,
        }
    }

    #[test]
    fn sfx_routes_by_explicit_kind_not_name() {
        // No "SOUND"/"sfx" in the model name — routing comes from AudioKind.
        let (url, body) = build_request("rain on a tin roof", "V6", &opts(AudioKind::Sfx, "V6"));
        assert!(url.ends_with("/api/v1/generate/sounds"));
        assert_eq!(body["model"], "V6"); // passthrough, not hardcoded
        assert!(body.get("duration").is_none()); // sounds endpoint has no duration field
        assert_eq!(body["soundTempo"], 120);
        assert_eq!(body["soundKey"], "Any");
        assert_eq!(body["grabLyrics"], false);
    }

    #[test]
    fn music_routes_to_generate_endpoint() {
        let mut o = opts(AudioKind::Music, "V6");
        o.provider_options
            .insert("customMode".into(), serde_yaml::Value::Bool(true));
        o.duration_seconds = Some(999.0);
        let (url, body) = build_request("a slow ballad", "V6", &o);
        assert!(url.ends_with("/api/v1/generate"));
        assert_eq!(body["model"], "V6");
        assert_eq!(body["duration"], 360); // clamped to the 10–360 window
    }

    #[test]
    fn duration_clamped_to_10_360() {
        assert_eq!(clamp_music_duration(5), 10);
        assert_eq!(clamp_music_duration(180), 180);
        assert_eq!(clamp_music_duration(400), 360);
    }

    #[test]
    fn duration_omitted_outside_custom_mode() {
        let mut o = opts(AudioKind::Music, "V6");
        o.duration_seconds = Some(120.0);
        let (_, body) = build_request("a song", "V6", &o);
        assert!(body.get("duration").is_none());
    }

    #[test]
    fn rate_limit_retries_once_then_fails() {
        assert_eq!(retry_after_rate_limit(429, 0), Some(RATE_LIMIT_RETRY_SECS));
        assert_eq!(retry_after_rate_limit(429, 1), None);
        assert_eq!(retry_after_rate_limit(500, 0), None);
        assert_eq!(retry_after_rate_limit(200, 0), None);
    }

    #[test]
    fn weights_round_to_two_decimals() {
        assert_eq!(round2(0.333_333_3), 0.33);
        assert_eq!(round2(0.8), 0.8);
        assert_eq!(round2(0.456_7), 0.46);
    }
}
