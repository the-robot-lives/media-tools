use std::path::Path;
use std::time::Duration;

use serde_json::json;

use crate::attachments::LoadedAttachment;
use crate::providers::dashscope;
use crate::providers::{GenerationOptions, MediaProvider};
use crate::ui;

const DEFAULT_MODEL: &str = "qwen-image-3.0";

pub struct QwenImageProvider;

#[async_trait::async_trait]
impl MediaProvider for QwenImageProvider {
    async fn generate(
        &self,
        prompt_text: &str,
        output_path: &Path,
        api_key: &str,
        options: &GenerationOptions,
        attachments: &[LoadedAttachment],
    ) -> color_eyre::Result<bool> {
        let model = if options.model.is_empty() || options.model == "default" {
            DEFAULT_MODEL
        } else {
            &options.model
        };

        let mut content = Vec::new();
        for att in attachments.iter().take(3) {
            let mime = if att.mime_type.is_empty() {
                "image/png"
            } else {
                att.mime_type.as_str()
            };
            content.push(json!({
                "image": format!("data:{};base64,{}", mime, att.data_b64)
            }));
        }
        content.push(json!({ "text": prompt_text }));

        let mut parameters = json!({});
        if let Some(neg) = options.negative_prompt.as_deref() {
            parameters["negative_prompt"] = json!(neg);
        }
        if let Some(size) = size_param(options) {
            parameters["size"] = json!(size);
        }
        if let Some(n) = options
            .provider_options
            .get("n")
            .and_then(|v| v.as_u64())
        {
            parameters["n"] = json!(n);
        }

        let body = json!({
            "model": model,
            "input": {
                "messages": [{ "role": "user", "content": content }]
            },
            "parameters": parameters,
        });

        let api_url = dashscope::multimodal_url(options);
        if options.verbose {
            ui::verbose(&format!("POST {}", api_url));
            ui::verbose(&format!("Model: {}", model));
        }

        let client = reqwest::Client::new();
        let resp = client
            .post(&api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(Duration::from_secs(180))
            .send()
            .await;

        let response = match resp {
            Ok(r) => r,
            Err(e) => {
                ui::fail_msg(&format!("Network error calling Qwen Image: {}", e));
                return Ok(false);
            }
        };

        let status = response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            let body_text = response.text().await.unwrap_or_default();
            color_eyre::eyre::bail!(
                "Qwen/DashScope authentication failed ({}): {}\n  Check DASHSCOPE_API_KEY / QWEN_API_KEY / QWEN_TOKEN_KEY",
                status.as_u16(),
                &body_text[..body_text.len().min(200)]
            );
        }
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            ui::fail_msg(&format!(
                "Qwen Image error ({}): {}",
                status.as_u16(),
                &body_text[..body_text.len().min(300)]
            ));
            return Ok(false);
        }

        let result: serde_json::Value = response.json().await?;
        let image_url = result
            .pointer("/output/choices/0/message/content/0/image")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                color_eyre::eyre::eyre!("No image URL in Qwen Image response: {}", result)
            })?;

        download_to(client, image_url, output_path, options.verbose).await
    }

    fn name(&self) -> &str {
        "qwen-image"
    }
}

fn size_param(options: &GenerationOptions) -> Option<String> {
    if let Some(s) = options
        .provider_options
        .get("size")
        .and_then(|v| v.as_str())
    {
        return Some(s.replace('x', "*"));
    }
    options.aspect_ratio.as_deref().map(|ar| match ar {
        "1:1" => "1024*1024".into(),
        "16:9" => "1280*720".into(),
        "9:16" => "720*1280".into(),
        "4:3" => "1024*768".into(),
        "3:4" => "768*1024".into(),
        _ => "1024*1024".into(),
    })
}

pub(crate) async fn download_to(
    client: reqwest::Client,
    url: &str,
    output_path: &Path,
    verbose: bool,
) -> color_eyre::Result<bool> {
    if verbose {
        ui::verbose(&format!("Downloading {}", url));
    }
    let audio_resp = client.get(url).timeout(Duration::from_secs(120)).send().await;
    match audio_resp {
        Ok(r) => {
            if !r.status().is_success() {
                ui::fail_msg(&format!("Download failed: HTTP {}", r.status()));
                return Ok(false);
            }
            let bytes = r.bytes().await?;
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
