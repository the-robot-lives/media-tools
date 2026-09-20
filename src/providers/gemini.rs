use std::path::Path;
use std::time::Duration;

use base64::Engine;
use serde_json::json;

use crate::attachments::LoadedAttachment;
use crate::imagefmt;
use crate::providers::{GenerationOptions, MediaProvider};
use crate::telemetry as tel;
use crate::telemetry::progress;

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_SECS: u64 = 2;
/// Legacy predict-endpoint path; kept for reference, no longer dispatched
/// (Google removed `:predict` support for image models).
#[allow(dead_code)]
const GENERATE_CONTENT_MODEL: &str = "gemini-3.1-flash-image";

pub struct GeminiProvider;


/// Gemini API root. `provider_options.base_url` overrides it, which is how the tests point the
/// provider at a stub server; it also serves as an escape hatch for a proxy or mirror.
fn api_root(options: &GenerationOptions) -> String {
    options
        .provider_options
        .get("base_url")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_end_matches('/'))
        .filter(|s| !s.is_empty())
        .unwrap_or("https://generativelanguage.googleapis.com")
        .to_string()
}

/// Write provider bytes so the file's contents match its name.
///
/// Gemini returns JPEG whatever `output.format` a prompt declares. Writing those bytes to a
/// `.png` path used to report success and then break post-processing one step later with
/// "cannot decode". Transcode to what was asked for, and if that is impossible, write under
/// the true extension and say so.
fn write_reconciled(output_path: &Path, bytes: &[u8]) -> color_eyre::Result<std::path::PathBuf> {
    let (written, outcome) = imagefmt::write_image_reconciled(output_path, bytes)?;
    match outcome {
        imagefmt::WriteOutcome::AsRequested => {}
        imagefmt::WriteOutcome::Transcoded { from, to } => {
            tel::verbose(&format!(
                "Gemini returned {:?}; transcoded to {:?} as declared by the prompt",
                from, to
            ));
        }
        imagefmt::WriteOutcome::Renamed { ref to, kind } => {
            tel::warn_msg(&format!(
                "Gemini returned {:?}, which cannot be written as '{}' \u{2014} wrote {} instead",
                kind,
                output_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("(none)"),
                to.display()
            ));
        }
        imagefmt::WriteOutcome::Unknown => {
            tel::warn_msg(&format!(
                "Gemini returned bytes in an unrecognised container; wrote {} verbatim",
                written.display()
            ));
        }
    }
    Ok(written)
}

#[async_trait::async_trait]
impl MediaProvider for GeminiProvider {
    async fn generate(
        &self,
        prompt_text: &str,
        output_path: &Path,
        api_key: &str,
        options: &GenerationOptions,
        attachments: &[LoadedAttachment],
    ) -> color_eyre::Result<bool> {
        // All gemini image models now require generateContent (`:predict` was
        // removed from the API — returns 404 NOT_FOUND for image models).
        self.generate_content(prompt_text, output_path, api_key, options, attachments)
            .await
    }

    fn name(&self) -> &str {
        "gemini"
    }
}

impl GeminiProvider {
    /// Aspect ratios accepted by generateContent's imageConfig; snap unknown
    /// ratios (e.g. 16:10) to the nearest supported one.
    const SUPPORTED_ASPECT_RATIOS: [(&'static str, f64); 14] = [
        ("1:1", 1.0),
        ("1:4", 0.25),
        ("1:8", 0.125),
        ("2:3", 2.0 / 3.0),
        ("3:2", 1.5),
        ("3:4", 0.75),
        ("4:1", 4.0),
        ("4:3", 4.0 / 3.0),
        ("4:5", 0.8),
        ("5:4", 1.25),
        ("8:1", 8.0),
        ("9:16", 9.0 / 16.0),
        ("16:9", 16.0 / 9.0),
        ("21:9", 21.0 / 9.0),
    ];

    fn snap_aspect_ratio(requested: &str) -> Option<&'static str> {
        let value = Self::parse_ratio(requested)?;
        Self::SUPPORTED_ASPECT_RATIOS
            .iter()
            .min_by(|a, b| {
                (a.1 - value)
                    .abs()
                    .partial_cmp(&(b.1 - value).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(name, _)| *name)
    }

    fn parse_ratio(ratio: &str) -> Option<f64> {
        let (w, h) = ratio.split_once(':')?;
        let w: f64 = w.trim().parse().ok()?;
        let h: f64 = h.trim().parse().ok()?;
        if h == 0.0 {
            return None;
        }
        Some(w / h)
    }

    /// Plain generation via the Imagen predict endpoint (no attachments).
    #[allow(dead_code)]
    async fn generate_predict(
        &self,
        prompt_text: &str,
        output_path: &Path,
        api_key: &str,
        options: &GenerationOptions,
    ) -> color_eyre::Result<bool> {
        let url = format!(
            "{}/v1beta/models/{}:predict?key={}",
            api_root(options),
            options.model,
            api_key
        );

        let mut params = json!({ "sampleCount": 1 });
        if let Some(ref ar) = options.aspect_ratio {
            params["aspectRatio"] = json!(ar);
        }
        for (k, v) in &options.provider_options {
            match k.as_str() {
                "safety_filter_level" => {
                    if let Some(s) = v.as_str() {
                        params["safetyFilterLevel"] = json!(s);
                    }
                }
                "person_generation" => {
                    if let Some(s) = v.as_str() {
                        params["personGeneration"] = json!(s);
                    }
                }
                _ => {}
            }
        }

        let body = json!({
            "instances": [{ "prompt": prompt_text }],
            "parameters": params,
        });

        if options.verbose {
            tel::verbose(&format!(
                "POST {}?key=***",
                url.split('?').next().unwrap_or(&url)
            ));
            let preview: String = prompt_text.chars().take(120).collect();
            tel::verbose(&format!(
                "Prompt: {}{}",
                preview,
                if prompt_text.len() > 120 { "..." } else { "" }
            ));
        }

        let result = self
            .post_with_retry(&url, &body, output_path, options.verbose)
            .await?;
        let Some(result) = result else {
            return Ok(false);
        };

        let predictions = result["predictions"].as_array();
        if predictions.is_none() || predictions.unwrap().is_empty() {
            tel::fail_msg(&format!(
                "No predictions returned for {}",
                output_path.display()
            ));
            return Ok(false);
        }

        let image_b64 = predictions.unwrap()[0]["bytesBase64Encoded"]
            .as_str()
            .unwrap_or("");
        if image_b64.is_empty() {
            tel::fail_msg(&format!("Empty image data for {}", output_path.display()));
            return Ok(false);
        }

        let image_bytes = base64::engine::general_purpose::STANDARD.decode(image_b64)?;
        write_reconciled(output_path, &image_bytes)?;
        Ok(true)
    }

    /// Generation with reference images via the generateContent endpoint.
    async fn generate_content(
        &self,
        prompt_text: &str,
        output_path: &Path,
        api_key: &str,
        options: &GenerationOptions,
        attachments: &[LoadedAttachment],
    ) -> color_eyre::Result<bool> {
        let model = options
            .provider_options
            .get("generate_content_model")
            .and_then(|v| v.as_str())
            .unwrap_or(&options.model);

        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            api_root(options),
            model,
            api_key
        );

        let mut parts: Vec<serde_json::Value> = Vec::new();

        // Text prompt first
        parts.push(json!({ "text": prompt_text }));

        // Attachment images as inline_data parts
        for att in attachments {
            parts.push(json!({
                "inline_data": {
                    "mime_type": att.mime_type,
                    "data": att.data_b64,
                }
            }));
        }

        let mut generation_config = json!({ "responseModalities": ["TEXT", "IMAGE"] });
        if let Some(ref ar) = options.aspect_ratio {
            if let Some(snapped) = Self::snap_aspect_ratio(ar) {
                generation_config["imageConfig"] = json!({ "aspectRatio": snapped });
            }
        }
        let body = json!({
            "contents": [{ "parts": parts }],
            "generationConfig": generation_config,
        });

        if options.verbose {
            tel::verbose(&format!(
                "POST {}?key=*** (generateContent with {} attachment(s))",
                url.split('?').next().unwrap_or(&url),
                attachments.len()
            ));
            let preview: String = prompt_text.chars().take(120).collect();
            tel::verbose(&format!(
                "Prompt: {}{}",
                preview,
                if prompt_text.len() > 120 { "..." } else { "" }
            ));
            tel::verbose(&format!("Model: {} (generateContent)", model));
        }

        let result = self
            .post_with_retry(&url, &body, output_path, options.verbose)
            .await?;
        let Some(result) = result else {
            return Ok(false);
        };

        // Extract image from generateContent response
        let candidates = result["candidates"].as_array();
        if candidates.is_none() || candidates.unwrap().is_empty() {
            tel::fail_msg(&format!(
                "No candidates in response for {}",
                output_path.display()
            ));
            return Ok(false);
        }

        let parts = candidates.unwrap()[0]["content"]["parts"].as_array();
        if let Some(parts) = parts {
            for part in parts {
                if let Some(inline) = part.get("inlineData").or(part.get("inline_data")) {
                    let image_b64 = inline["data"].as_str().unwrap_or("");
                    if !image_b64.is_empty() {
                        let image_bytes =
                            base64::engine::general_purpose::STANDARD.decode(image_b64)?;
                        write_reconciled(output_path, &image_bytes)?;
                        return Ok(true);
                    }
                }
            }
        }

        // If we got here, no image was found in the response
        if options.verbose {
            let resp_preview = serde_json::to_string_pretty(&result).unwrap_or_default();
            let truncated: String = resp_preview.chars().take(500).collect();
            tel::verbose(&format!("Response (no image found): {}", truncated));
        }
        tel::fail_msg(&format!(
            "No image data in response for {}",
            output_path.display()
        ));
        Ok(false)
    }

    /// Shared HTTP POST with retry/backoff logic.
    async fn post_with_retry(
        &self,
        url: &str,
        body: &serde_json::Value,
        output_path: &Path,
        verbose: bool,
    ) -> color_eyre::Result<Option<serde_json::Value>> {
        let client = reqwest::Client::new();
        let mut backoff = INITIAL_BACKOFF_SECS;

        for attempt in 1..=MAX_RETRIES {
            progress::provider_request(
                "gemini",
                "",
                url.split('?').next().unwrap_or(url),
                attempt as usize,
            );
            let resp = client
                .post(url)
                .header("Content-Type", "application/json")
                .json(body)
                .timeout(Duration::from_secs(120))
                .send()
                .await;

            match resp {
                Ok(response) => {
                    let status = response.status();
                    progress::provider_response(
                        "gemini",
                        status.as_u16(),
                        status.is_success(),
                        attempt as usize,
                    );
                    if status.is_success() {
                        let result: serde_json::Value = response.json().await?;
                        return Ok(Some(result));
                    }

                    let status_code = status.as_u16();
                    let error_body = response.text().await.unwrap_or_default();

                    match status_code {
                        429 if attempt < MAX_RETRIES => {
                            progress::provider_retry(
                                "gemini",
                                attempt as usize,
                                backoff * 1000,
                                "rate limited (429)",
                            );
                            tel::warn_msg(&format!(
                                "Rate limited (429), retrying in {}s (attempt {}/{})",
                                backoff, attempt, MAX_RETRIES
                            ));
                            tokio::time::sleep(Duration::from_secs(backoff)).await;
                            backoff *= 2;
                            continue;
                        }
                        429 => {
                            tel::fail_msg(&format!(
                                "Rate limited after {} retries: {}",
                                MAX_RETRIES,
                                output_path.display()
                            ));
                            return Ok(None);
                        }
                        400 => {
                            let preview: String = error_body.chars().take(300).collect();
                            tel::fail_msg(&format!(
                                "Bad request (400) for {}: {}",
                                output_path.display(),
                                preview
                            ));
                            if verbose {
                                tel::verbose(&error_body);
                            }
                            return Ok(None);
                        }
                        401 | 403 => {
                            let preview: String = error_body.chars().take(200).collect();
                            color_eyre::eyre::bail!(
                                "Authentication failed ({}): {}\n  Check your GEMINI_API_KEY",
                                status_code,
                                preview
                            );
                        }
                        _ => {
                            let preview: String = error_body.chars().take(200).collect();
                            tel::fail_msg(&format!(
                                "HTTP {} for {}: {}",
                                status_code,
                                output_path.display(),
                                preview
                            ));
                            return Ok(None);
                        }
                    }
                }
                Err(e) => {
                    progress::provider_response("gemini", 0, false, attempt as usize);
                    tel::fail_msg(&format!(
                        "Network error for {}: {}",
                        output_path.display(),
                        e
                    ));
                    if attempt < MAX_RETRIES {
                        progress::provider_retry(
                            "gemini",
                            attempt as usize,
                            backoff * 1000,
                            "network error",
                        );
                        tel::warn_msg(&format!(
                            "Retrying in {}s (attempt {}/{})",
                            backoff, attempt, MAX_RETRIES
                        ));
                        tokio::time::sleep(Duration::from_secs(backoff)).await;
                        backoff *= 2;
                        continue;
                    }
                    return Ok(None);
                }
            }
        }

        Ok(None)
    }
}
