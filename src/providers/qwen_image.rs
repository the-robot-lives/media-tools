use std::path::Path;
use std::time::Duration;

use serde_json::json;

use crate::attachments::LoadedAttachment;
use crate::providers::dashscope;
use crate::providers::http;
use crate::providers::{GenerationOptions, MediaProvider};
use crate::telemetry as tel;
use crate::telemetry::progress;

const DEFAULT_MODEL: &str = "qwen-image-3.0";

/// Client-level ceiling for a qwen-image call. The old code built a bare
/// `reqwest::Client::new()` and set only a per-request timeout, so a render that ran past
/// the connection-level defaults died at roughly a minute with a bare
/// "error sending request" — well before the nominal 180s. Override with
/// `MEDIA_QWEN_TIMEOUT_SECS`.
const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// How long to wait on the *submission* leg when async task mode is on. The submit
/// returns a task id in about a second, so this stays short.
const SUBMIT_TIMEOUT_SECS: u64 = 60;

/// Poll cadence and ceiling for async task mode. 120 x 5s = 10 minutes.
const POLL_INTERVAL_SECS: u64 = 5;
const MAX_POLL_ATTEMPTS: u32 = 120;

/// Total request timeout for a qwen-image call.
fn request_timeout() -> Duration {
    http::env_secs("MEDIA_QWEN_TIMEOUT_SECS")
        .unwrap_or(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
}

/// Async task mode (`X-DashScope-Async: enable`) is the default: the render then happens
/// off a short submit request and is collected by polling, so a long render never depends
/// on holding one connection open past a gateway's idle limit.
///
/// Disable per prompt with `provider_options: {async: false}` or globally with
/// `MEDIA_QWEN_ASYNC=0`.
fn async_mode(options: &GenerationOptions) -> bool {
    if let Some(v) = options
        .provider_options
        .get("async")
        .and_then(|v| v.as_bool())
    {
        return v;
    }
    std::env::var("MEDIA_QWEN_ASYNC").ok().as_deref() != Some("0")
}

/// Pull an image URL out of either response shape: the synchronous multimodal payload or
/// an async task result.
fn extract_image_url(v: &serde_json::Value) -> Option<&str> {
    const POINTERS: &[&str] = &[
        "/output/choices/0/message/content/0/image",
        "/output/results/0/url",
        "/output/results/0/image",
        "/output/task_result/results/0/url",
        "/output/url",
    ];
    POINTERS.iter().find_map(|p| v.pointer(p).and_then(|v| v.as_str()))
}

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
            tel::verbose(&format!("POST {}", api_url));
            tel::verbose(&format!("Model: {}", model));
        }

        progress::provider_request("qwen-image", &options.model, &api_url, 1);

        let use_async = async_mode(options);
        // In async mode the connection only has to survive the submit; in sync mode it has
        // to survive the whole render, so the client-level ceiling follows the mode.
        let total = if use_async {
            Duration::from_secs(SUBMIT_TIMEOUT_SECS)
        } else {
            request_timeout()
        };
        let client = http::client_with_timeout(total);

        let mut request = client
            .post(&api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json");
        if use_async {
            request = request.header("X-DashScope-Async", "enable");
        }
        let resp = request.json(&body).timeout(total).send().await;

        let response = match resp {
            Ok(r) => r,
            Err(e) => {
                progress::provider_response("qwen-image", 0, false, 1);
                tel::fail_msg(&format!("Network error calling Qwen Image: {}", e));
                return Ok(false);
            }
        };

        let status = response.status();
        progress::provider_response("qwen-image", status.as_u16(), status.is_success(), 1);
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
            tel::fail_msg(&format!(
                "Qwen Image error ({}): {}",
                status.as_u16(),
                &body_text[..body_text.len().min(300)]
            ));
            return Ok(false);
        }

        let result: serde_json::Value = response.json().await?;

        // Some deployments ignore the async header and answer inline; take the image if it
        // is already here rather than hunting for a task id that will never exist.
        if let Some(image_url) = extract_image_url(&result) {
            let image_url = image_url.to_string();
            return download_to(client, &image_url, output_path, options.verbose).await;
        }

        let task_id = result
            .pointer("/output/task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                color_eyre::eyre::eyre!("No image URL or task_id in Qwen Image response: {}", result)
            })?
            .to_string();

        poll_task(&client, &task_id, api_key, output_path, options).await
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

/// Poll a DashScope task until it succeeds, fails, or the ceiling is hit.
async fn poll_task(
    client: &reqwest::Client,
    task_id: &str,
    api_key: &str,
    output_path: &Path,
    options: &GenerationOptions,
) -> color_eyre::Result<bool> {
    tel::info(&format!(
        "Qwen Image task {} \u{2014} polling for completion",
        task_id
    ));
    let poll_url = dashscope::task_url(options, task_id);
    let interval = http::env_secs("MEDIA_QWEN_POLL_SECS")
        .unwrap_or(Duration::from_secs(POLL_INTERVAL_SECS));
    let max_attempts = std::env::var("MEDIA_QWEN_POLL_ATTEMPTS")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(MAX_POLL_ATTEMPTS);

    let mut last_status = String::new();
    for attempt in 1..=max_attempts {
        tokio::time::sleep(interval).await;
        progress::provider_retry(
            "qwen-image",
            attempt as usize,
            interval.as_millis() as u64,
            "polling for completion",
        );

        let poll_resp = client
            .get(&poll_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .timeout(Duration::from_secs(60))
            .send()
            .await;

        let poll_response = match poll_resp {
            Ok(r) => r,
            Err(e) => {
                // A transient poll failure is not a failed render; the task keeps running.
                tel::warn_msg(&format!("Qwen Image poll error (retrying): {}", e));
                continue;
            }
        };
        let status = poll_response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            let body_text = poll_response.text().await.unwrap_or_default();
            color_eyre::eyre::bail!(
                "Qwen/DashScope authentication failed while polling ({}): {}",
                status.as_u16(),
                &body_text[..body_text.len().min(200)]
            );
        }
        if !status.is_success() {
            tel::warn_msg(&format!(
                "Qwen Image poll HTTP {} (retrying)",
                status.as_u16()
            ));
            continue;
        }

        let poll: serde_json::Value = match poll_response.json().await {
            Ok(v) => v,
            Err(e) => {
                tel::warn_msg(&format!("Qwen Image poll decode error (retrying): {}", e));
                continue;
            }
        };
        let st = poll
            .pointer("/output/task_status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        match st.as_str() {
            "SUCCEEDED" => {
                let image_url = extract_image_url(&poll)
                    .ok_or_else(|| {
                        color_eyre::eyre::eyre!("No image URL in Qwen Image result: {}", poll)
                    })?
                    .to_string();
                return download_to(client.clone(), &image_url, output_path, options.verbose)
                    .await;
            }
            "FAILED" | "CANCELED" | "UNKNOWN" => {
                let msg = poll
                    .pointer("/output/message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("no message");
                tel::fail_msg(&format!("Qwen Image task {} {}: {}", task_id, st, msg));
                return Ok(false);
            }
            other => {
                if options.verbose && other != last_status {
                    tel::verbose(&format!("Qwen Image task {} status {}", task_id, other));
                }
                last_status = st;
            }
        }
    }

    tel::fail_msg(&format!(
        "Qwen Image task {} did not finish within {}s",
        task_id,
        interval.as_secs() * max_attempts as u64
    ));
    Ok(false)
}

pub(crate) async fn download_to(
    client: reqwest::Client,
    url: &str,
    output_path: &Path,
    verbose: bool,
) -> color_eyre::Result<bool> {
    if verbose {
        tel::verbose(&format!("Downloading {}", url));
    }
    let audio_resp = client.get(url).timeout(Duration::from_secs(120)).send().await;
    match audio_resp {
        Ok(r) => {
            if !r.status().is_success() {
                tel::fail_msg(&format!("Download failed: HTTP {}", r.status()));
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
            tel::fail_msg(&format!("Download error: {}", e));
            Ok(false)
        }
    }
}
