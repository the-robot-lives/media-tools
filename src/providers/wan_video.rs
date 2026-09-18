use std::path::Path;
use std::time::Duration;

use serde_json::json;

use crate::attachments::LoadedAttachment;
use crate::providers::dashscope;
use crate::providers::qwen_image::download_to;
use crate::providers::{GenerationOptions, MediaProvider};
use crate::telemetry as tel;
use crate::telemetry::progress;

const DEFAULT_MODEL: &str = "wan2.7-t2v";
const POLL_INTERVAL_SECS: u64 = 10;
const MAX_POLL_ATTEMPTS: u32 = 60;

pub struct WanVideoProvider;

#[async_trait::async_trait]
impl MediaProvider for WanVideoProvider {
    async fn generate(
        &self,
        prompt_text: &str,
        output_path: &Path,
        api_key: &str,
        options: &GenerationOptions,
        attachments: &[LoadedAttachment],
    ) -> color_eyre::Result<bool> {
        let model = if options.model.is_empty() || options.model == "default" {
            if attachments.is_empty() {
                DEFAULT_MODEL
            } else {
                "wan2.7-i2v"
            }
        } else {
            &options.model
        };

        let mut input = json!({ "prompt": prompt_text });
        if let Some(neg) = options.negative_prompt.as_deref() {
            input["negative_prompt"] = json!(neg);
        }
        if let Some(att) = attachments.first() {
            let mime = if att.mime_type.is_empty() {
                "image/png"
            } else {
                att.mime_type.as_str()
            };
            input["img_url"] = json!(format!("data:{};base64,{}", mime, att.data_b64));
        }

        let duration = options
            .duration_seconds
            .map(|d| d as u64)
            .unwrap_or(5);
        let ratio = options.aspect_ratio.as_deref().unwrap_or("16:9");
        let resolution = options
            .provider_options
            .get("resolution")
            .and_then(|v| v.as_str())
            .unwrap_or("720P");

        let body = json!({
            "model": model,
            "input": input,
            "parameters": {
                "resolution": resolution,
                "ratio": ratio,
                "duration": duration,
                "prompt_extend": true
            }
        });

        let api_url = dashscope::video_synthesis_url(options);
        if options.verbose {
            tel::verbose(&format!("POST {}", api_url));
            tel::verbose(&format!(
                "Model: {}, duration: {}s, ratio: {}",
                model, duration, ratio
            ));
        }

        progress::provider_request("wan-video", &options.model, &api_url, 1);
        let client = reqwest::Client::new();
        let resp = client
            .post(&api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("X-DashScope-Async", "enable")
            .json(&body)
            .timeout(Duration::from_secs(60))
            .send()
            .await;

        let response = match resp {
            Ok(r) => r,
            Err(e) => {
                progress::provider_response("wan-video", 0, false, 1);
                tel::fail_msg(&format!("Network error submitting Wan video: {}", e));
                return Ok(false);
            }
        };

        let status = response.status();
        progress::provider_response("wan-video", status.as_u16(), status.is_success(), 1);
        if status.as_u16() == 401 || status.as_u16() == 403 {
            let body_text = response.text().await.unwrap_or_default();
            color_eyre::eyre::bail!(
                "Qwen/DashScope authentication failed ({}): {}\n  Check DASHSCOPE_API_KEY / QWEN_API_KEY / QWEN_TOKEN_KEY",
                status.as_u16(),
                crate::text::truncate(&body_text, 200)
            );
        }
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            tel::fail_msg(&format!(
                "Wan video error ({}): {}",
                status.as_u16(),
                crate::text::truncate(&body_text, 300)
            ));
            return Ok(false);
        }

        let result: serde_json::Value = response.json().await?;
        let task_id = result
            .pointer("/output/task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                color_eyre::eyre::eyre!("No task_id in Wan video response: {}", result)
            })?;

        tel::info(&format!("Wan task {} — polling for completion", task_id));
        let poll_url = dashscope::task_url(options, task_id);

        for poll_attempt in 1..=MAX_POLL_ATTEMPTS {
            tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
            progress::provider_retry(
                "wan-video",
                poll_attempt as usize,
                POLL_INTERVAL_SECS * 1000,
                "polling for completion",
            );
            let poll_resp = client
                .get(&poll_url)
                .header("Authorization", format!("Bearer {}", api_key))
                .timeout(Duration::from_secs(30))
                .send()
                .await;

            let poll_response = match poll_resp {
                Ok(r) => r,
                Err(e) => {
                    tel::fail_msg(&format!("Wan poll error: {}", e));
                    continue;
                }
            };
            if !poll_response.status().is_success() {
                continue;
            }
            let poll: serde_json::Value = poll_response.json().await?;
            let st = poll
                .pointer("/output/task_status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match st {
                "SUCCEEDED" => {
                    let video_url = poll
                        .pointer("/output/video_url")
                        .and_then(|v| v.as_str())
                        .or_else(|| {
                            poll.pointer("/output/results/0/url")
                                .and_then(|v| v.as_str())
                        })
                        .ok_or_else(|| {
                            color_eyre::eyre::eyre!("No video URL in Wan result: {}", poll)
                        })?;
                    return download_to(client, video_url, output_path, options.verbose).await;
                }
                "FAILED" | "CANCELED" | "UNKNOWN" => {
                    tel::fail_msg(&format!("Wan task {} {}", task_id, st));
                    return Ok(false);
                }
                _ => {
                    if options.verbose {
                        tel::verbose(&format!("Wan task {} status {}", task_id, st));
                    }
                }
            }
        }

        tel::fail_msg(&format!("Wan task {} timed out", task_id));
        Ok(false)
    }

    fn name(&self) -> &str {
        "wan-video"
    }
}
