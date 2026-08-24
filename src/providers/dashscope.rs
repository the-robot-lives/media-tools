use crate::providers::GenerationOptions;

const INTL: &str = "https://dashscope-intl.aliyuncs.com";
const CN: &str = "https://dashscope.aliyuncs.com";
const TOKEN_PLAN: &str = "https://token-plan.ap-southeast-1.maas.aliyuncs.com";

const KEY_ENVS: &[&str] = &["DASHSCOPE_API_KEY", "QWEN_API_KEY", "QWEN_TOKEN_KEY"];

/// DashScope / Model Studio API root from provider_options `plan` / `region`.
pub fn api_root(options: &GenerationOptions) -> &'static str {
    let plan = options
        .provider_options
        .get("plan")
        .and_then(|v| v.as_str())
        .or_else(|| {
            options
                .provider_options
                .get("region")
                .and_then(|v| v.as_str())
        })
        .unwrap_or("intl");

    match plan {
        "token" | "token-plan" | "token_plan" => TOKEN_PLAN,
        "cn" | "beijing" => CN,
        _ => INTL,
    }
}

pub fn multimodal_url(options: &GenerationOptions) -> String {
    format!(
        "{}/api/v1/services/aigc/multimodal-generation/generation",
        api_root(options)
    )
}

pub fn video_synthesis_url(options: &GenerationOptions) -> String {
    format!(
        "{}/api/v1/services/aigc/video-generation/video-synthesis",
        api_root(options)
    )
}

pub fn task_url(options: &GenerationOptions, task_id: &str) -> String {
    format!("{}/api/v1/tasks/{}", api_root(options), task_id)
}

/// First non-empty DashScope-family env var (on-demand, then Qwen, then token plan).
pub fn resolve_key() -> Option<String> {
    first_env(KEY_ENVS)
}

pub fn first_env(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|n| std::env::var(n).ok().filter(|v| !v.is_empty()))
}

/// Prefer token-plan key when `plan` is token-plan.
pub fn resolve_key_for(options: &GenerationOptions) -> Option<String> {
    let plan = options
        .provider_options
        .get("plan")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if matches!(plan, "token" | "token-plan" | "token_plan") {
        first_env(&["QWEN_TOKEN_KEY", "DASHSCOPE_API_KEY"])
    } else {
        resolve_key()
    }
}
