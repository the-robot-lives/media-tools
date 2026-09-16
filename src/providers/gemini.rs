use std::path::Path;
use std::time::Duration;

use base64::Engine;
use serde_json::json;

use crate::attachments::LoadedAttachment;
use crate::providers::{GenerationOptions, MediaProvider};
use crate::telemetry as tel;
use crate::telemetry::progress;

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_SECS: u64 = 2;
/// Legacy predict-endpoint path; kept for reference, no longer dispatched
/// (Google removed `:predict` support for image models).
#[allow(dead_code)]
const GENERATE_CONTENT_MODEL: &str = "gemini-3.1-flash-image";

/// Lowest Gemini generation permitted for image generation.
///
/// The floor is a **major** version, deliberately. The obvious reading of "use the
/// current models" would be `>= 3.1`, and that is wrong here: `gemini-3-pro-image`
/// (Nano Banana Pro) is the highest-quality image model in the lineup, but its id
/// carries no minor version, so any `>= 3.1` comparison sorts it *below*
/// `gemini-3.1-flash-image` and rejects it. Gemini image ids are not ordered by
/// quality — the "pro" tier numbers lower than the "flash" tier — so comparing
/// minor versions across the family is meaningless. The guard exists to stop spend
/// on *stale* models, so it gates on the generation only: the whole 3.x family is
/// in, 2.x and older are out, and a future 4.x needs no change here.
const MINIMUM_MAJOR_VERSION: u32 = 3;

/// Offline catalog verified against Google's image-generation guide, 2026-09-07.
pub const IMAGE_MODELS: &str = "Gemini image models (documented catalog; account availability may vary):\n  gemini-3-pro-image           supported (Nano Banana Pro)\n  gemini-3.1-flash-image       supported (Nano Banana 2; default)\n  gemini-3.1-flash-lite-image  supported (Nano Banana 2 Lite)\n  gemini-2.5-flash-image       blocked: Gemini 2 (Nano Banana)\nSource: https://ai.google.dev/gemini-api/docs/image-generation\nRun: generate-media-prompt models";

/// Major version of a Gemini model id, or `None` when the id carries no explicit one.
///
/// Accepts both `gemini-3-pro-image` (bare major) and `gemini-3.1-flash-image`
/// (major.minor); only the leading integer is read. An optional `models/` prefix is
/// tolerated because that is how the API spells ids in some responses.
fn major_version(model: &str) -> Option<u32> {
    model
        .strip_prefix("models/")
        .unwrap_or(model)
        .strip_prefix("gemini-")?
        .split('-')
        .next()?
        .split('.')
        .next()?
        .parse::<u32>()
        .ok()
}

/// Whether a model id names an image model rather than a text/chat one.
fn is_image_model(model: &str) -> bool {
    model.split('-').any(|segment| segment == "image")
}

pub fn validate_image_model(model: &str) -> color_eyre::Result<()> {
    let id = model.strip_prefix("models/").unwrap_or(model);
    // An unversioned alias (`nano-banana`, `gemini-flash-image-latest`) returns None:
    // it may resolve to anything, so it can never satisfy a minimum.
    if major_version(id).is_some_and(|major| major >= MINIMUM_MAJOR_VERSION) && is_image_model(id) {
        return Ok(());
    }
    color_eyre::eyre::bail!("Gemini image model '{model}' is not allowed. Use Gemini {MINIMUM_MAJOR_VERSION} or newer. Give an explicit versioned image model ID; unversioned aliases cannot guarantee the minimum.\n{IMAGE_MODELS}");
}

pub fn validate_image_options(options: &GenerationOptions) -> color_eyre::Result<()> {
    validate_image_model(&options.model)?;
    if let Some(value) = options.provider_options.get("generate_content_model") {
        let model = value.as_str().ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "generate_content_model must be a versioned Gemini image model string"
            )
        })?;
        validate_image_model(model)?;
    }
    Ok(())
}

pub struct GeminiProvider;

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
        validate_image_options(options)?;
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
            "https://generativelanguage.googleapis.com/v1beta/models/{}:predict",
            options.model
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
            .post_with_retry(&url, api_key, &body, output_path, options.verbose)
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
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output_path, &image_bytes)?;
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
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            model.strip_prefix("models/").unwrap_or(model)
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
            .post_with_retry(&url, api_key, &body, output_path, options.verbose)
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
                        if let Some(parent) = output_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(output_path, &image_bytes)?;
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
        api_key: &str,
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
                .header("x-goog-api-key", api_key)
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
                    let error_body = response
                        .text()
                        .await
                        .unwrap_or_default()
                        .replace(api_key, "[REDACTED]");

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
                        e.without_url()
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

#[cfg(test)]
mod image_policy_tests {
    use super::*;

    /// The whole policy in one test: the best model in the lineup is in, the stale
    /// generation is out. `gemini-3-pro-image` has no minor version and must not be
    /// read as "older than 3.1".
    #[test]
    fn admits_gemini_3_pro_and_rejects_gemini_2() {
        assert!(validate_image_model("gemini-3-pro-image").is_ok());
        assert!(validate_image_model("gemini-2.5-flash-image").is_err());
    }

    #[test]
    fn accepts_the_whole_3x_family_and_later() {
        for id in [
            "gemini-3-pro-image",
            "gemini-3.0-flash-image",
            "gemini-3.1-flash-image",
            "gemini-3.1-flash-lite-image",
            "gemini-3.10-flash-image",
            "models/gemini-3.1-flash-image-preview",
            "gemini-4-flash-image",
            "gemini-4-pro-image",
        ] {
            assert!(validate_image_model(id).is_ok(), "{id}");
        }
    }

    #[test]
    fn rejects_gemini_2_aliases_and_non_image_models() {
        for id in [
            // stale generation
            "gemini-2.5-flash-image",
            "gemini-2-pro-image",
            "gemini-1.5-flash-image",
            // unversioned aliases: could resolve to anything
            "nano-banana",
            "nano-banana-pro",
            "gemini-flash-image-latest",
            // text models are out of scope for the image guard
            "gemini-3.1-flash",
            "gemini-3-pro",
        ] {
            let error = validate_image_model(id).unwrap_err().to_string();
            assert!(error.contains("Use Gemini 3 or newer"), "{id}: {error}");
            assert!(error.contains("gemini-3-pro-image"), "{id}");
        }
    }

    #[test]
    fn catalog_lists_3_pro_as_supported() {
        assert!(IMAGE_MODELS.contains("gemini-3-pro-image           supported"));
        assert!(IMAGE_MODELS.contains("gemini-2.5-flash-image       blocked"));
    }
}
