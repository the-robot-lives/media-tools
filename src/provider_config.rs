//! Runtime provider/model configuration loaded from YAML (local file or remote URL).
//!
//! Lets model tiers, defaults, constraints, prompt-guidance docs, and the refine
//! model be tweaked without rebuilding. All knobs fall back to the compiled-in
//! defaults in `providers::mod` when absent or invalid.
//!
//! Resolution order:
//! 1. `MEDIA_TOOL_CONFIG` — path to a YAML file OR an http(s) URL
//! 2. `MEDIA_TOOL_CONFIG_URL` — http(s) URL (remote override)
//! 3. `./media-tool.yaml` (cwd)
//! 4. `$HOME/.config/media-tool/media-tool.yaml`
//!
//! See `media-tool.yaml` at the repo root for the full schema.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use serde::Deserialize;

/// Parsed runtime overrides. Every field optional — absent keys defer to
/// compiled-in defaults.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProviderConfig {
    #[serde(default)]
    pub version: Option<u32>,
    /// Default model per service (overrides `providers::default_model` match).
    #[serde(default)]
    pub defaults: BTreeMap<String, String>,
    /// Image tier ladders, quality key ("low"/"medium"/"high") -> ordered
    /// candidates, best-first. Each entry is "service:model"
    /// (e.g. "gemini:gemini-3.1-flash-image"). Replaces the built-in ladder
    /// for that tier when present.
    #[serde(default)]
    pub image_tiers: BTreeMap<String, Vec<String>>,
    /// Max prompt chars per service (overrides `providers::constraints`).
    #[serde(default)]
    pub max_prompt_chars: BTreeMap<String, usize>,
    /// Refine-loop chat model (overrides refine.rs REFINE_MODEL fallback).
    #[serde(default)]
    pub refine_model: Option<String>,
    /// Per-service prompt-guidance doc, path relative to the solutions dir
    /// (overrides fim.rs provider_solution mapping, e.g. gemini: providers/gemini-image.md).
    #[serde(default)]
    pub prompt_guidance: BTreeMap<String, String>,
}

static CONFIG: OnceLock<Option<ProviderConfig>> = OnceLock::new();

/// The loaded config, if a valid one was found at startup.
pub fn loaded() -> Option<&'static ProviderConfig> {
    CONFIG.get().and_then(|c| c.as_ref())
}

/// Load config at startup (call once, early in main). Never fails — invalid or
/// missing config just means compiled-in defaults, with a stderr warning when
/// a config was found but couldn't be parsed.
pub async fn init() {
    if CONFIG.get().is_some() {
        return;
    }
    let _ = CONFIG.set(load().await);
}

async fn load() -> Option<ProviderConfig> {
    // 1. Explicit MEDIA_TOOL_CONFIG: local path or URL
    if let Ok(src) = std::env::var("MEDIA_TOOL_CONFIG") {
        if src.starts_with("http://") || src.starts_with("https://") {
            return fetch_url(&src).await;
        }
        return read_file(&PathBuf::from(&src));
    }
    // 2. Dedicated remote override
    if let Ok(url) = std::env::var("MEDIA_TOOL_CONFIG_URL") {
        if !url.trim().is_empty() {
            return fetch_url(url.trim()).await;
        }
    }
    // 3./4. Conventional local locations
    if let Some(home) = std::env::var_os("HOME") {
        let mut candidates = vec![PathBuf::from("media-tool.yaml")];
        candidates.push(
            PathBuf::from(home)
                .join(".config")
                .join("media-tool")
                .join("media-tool.yaml"),
        );
        for path in candidates {
            if path.is_file() {
                return read_file(&path);
            }
        }
    }
    None
}

async fn fetch_url(url: &str) -> Option<ProviderConfig> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    match client.get(url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(text) => match serde_yaml::from_str::<ProviderConfig>(&text) {
                Ok(cfg) => Some(cfg),
                Err(e) => {
                    eprintln!("⚠ media-tool config: failed to parse remote config: {e}");
                    None
                }
            },
            Err(e) => {
                eprintln!("⚠ media-tool config: remote fetch failed ({url}): {e}");
                None
            }
        },
        Err(e) => {
            eprintln!("⚠ media-tool config: remote fetch failed ({url}): {e}");
            None
        }
    }
}

fn read_file(path: &PathBuf) -> Option<ProviderConfig> {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_yaml::from_str::<ProviderConfig>(&text) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                eprintln!("⚠ media-tool config: invalid YAML at {}: {e}", path.display());
                None
            }
        },
        Err(e) => {
            eprintln!("⚠ media-tool config: cannot read {}: {e}", path.display());
            None
        }
    }
}

/// Quality-tier key used in the YAML `image_tiers` map.
pub fn tier_key(quality: crate::schema::Quality) -> &'static str {
    match quality {
        crate::schema::Quality::Low => "low",
        crate::schema::Quality::Medium => "medium",
        crate::schema::Quality::High => "high",
    }
}

/// Parse "service:model" ladder entries into Candidates.
/// Entries without a ':' are treated as gemini model ids (common case).
pub fn parse_candidates(entries: &[String]) -> Vec<crate::providers::Candidate> {
    entries
        .iter()
        .filter_map(|e| {
            let full: &'static str = Box::leak(e.clone().into_boxed_str());
            match full.split_once(':') {
                Some((svc, model)) => Some(crate::providers::Candidate {
                    service: svc,
                    model,
                }),
                None => Some(crate::providers::Candidate {
                    service: "gemini",
                    model: full,
                }),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_service_model_entries() {
        let entries = vec![
            "gemini:gemini-3.1-flash-image".to_string(),
            "qwen-image:qwen-image-3.0".to_string(),
            "bare-model-id".to_string(),
        ];
        let cands = parse_candidates(&entries);
        assert_eq!(cands.len(), 3);
        assert_eq!(cands[0].service, "gemini");
        assert_eq!(cands[0].model, "gemini-3.1-flash-image");
        assert_eq!(cands[1].service, "qwen-image");
        assert_eq!(cands[2].service, "gemini");
        assert_eq!(cands[2].model, "bare-model-id");
    }

    #[test]
    fn parses_sample_yaml() {
        let sample = r#"
version: 1
defaults:
  gemini: gemini-3.1-flash-image
refine_model: gemini-3.7-flash
image_tiers:
  high:
    - gemini:gemini-3-pro-image
    - gemini:gemini-3.1-flash-image
max_prompt_chars:
  gemini: 4000
prompt_guidance:
  gemini: providers/gemini-image.md
"#;
        let cfg: ProviderConfig = serde_yaml::from_str(sample).unwrap();
        assert_eq!(cfg.defaults.get("gemini").unwrap(), "gemini-3.1-flash-image");
        assert_eq!(cfg.refine_model.as_deref(), Some("gemini-3.7-flash"));
        assert_eq!(cfg.image_tiers.get("high").unwrap().len(), 2);
        assert_eq!(cfg.max_prompt_chars.get("gemini"), Some(&4000));
        assert!(cfg.prompt_guidance.contains_key("gemini"));
    }
}
