//! Prep/eval inference (Alibaba `qwen3.8-flash` by default) and the vision
//! observer behind the Phase 2 observe -> decide judge path.
//!
//! Credentials and base URLs resolve through [`crate::secrets`] (env first,
//! then the dc store) and never enter logs or error strings. Chat completions
//! ride [`crate::llm_profiles::complete_with_key`] so the profile stays a
//! pure wire description.
//!
//! The observer turns a generated image (or extracted video frames) into a
//! factual per-criterion description — the decision judge itself is text-only.
//! The observer model chain is probed in order at runtime: the first model
//! that answers with a well-formed observations object wins, and the choice is
//! recorded in the judge metadata sidecar.

use std::collections::HashMap;

use color_eyre::eyre::{bail, eyre, Context, Result};
use serde_json::Value;

use crate::attachments::LoadedAttachment;
use crate::llm_profiles::{self, Profile};
use crate::schema::EvalCriterion;
use crate::secrets::{
    DcKey, Secret, SecretSource, SecretSpec,
};

/// DashScope intl OpenAI-compatible root — final inference fallback when
/// neither env (`QWEN_BASE_URL`) nor config supplies a base.
pub const DEFAULT_BASE: &str = "https://dashscope-intl.aliyuncs.com/compatible-mode/v1";

/// Default prep/eval chat model.
pub const DEFAULT_MODEL: &str = "qwen3.8-flash";

/// Vision-observer chain, best first. `qwen3.8-flash` is tried first because
/// reusing the prep/eval chat model keeps one credential and one endpoint in
/// play; the VL models are the fallback when it declines image input. The
/// first model that returns a well-formed observations object wins — see
/// `docs/quality-selection-and-eval.md` for the live probe evidence.
pub const DEFAULT_OBSERVER_MODELS: &[&str] = &["qwen3.8-flash", "qwen-vl-plus", "qwen-vl-max"];

/// A resolved inference connection. The key is a redacting [`Secret`].
#[derive(Debug, Clone)]
pub struct InferenceConfig {
    pub base: String,
    pub model: String,
    pub key: Secret,
    /// Observer chain, best first (may equal `[model]`).
    pub observer_models: Vec<String>,
}

impl InferenceConfig {
    /// `inference:` config section > environment > dc store > defaults.
    pub fn resolve(config: Option<&crate::provider_config::InferenceSection>) -> Result<Self> {
        Self::resolve_with(config, &|name| std::env::var(name).ok(), &real_dc())
    }

    /// Injectable core (tests): explicit env + dc runners.
    pub(crate) fn resolve_with(
        config: Option<&crate::provider_config::InferenceSection>,
        env: &dyn Fn(&str) -> Option<String>,
        dc: &dyn Fn(&str, &str) -> std::io::Result<crate::secrets::DcOutput>,
    ) -> Result<Self> {
        let config = config.cloned().unwrap_or_default();
        let model = config
            .model
            .or_else(|| env("MEDIA_INFERENCE_MODEL"))
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let base = match config.base.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            Some(url) => Some(url.to_string()),
            None => {
                let spec = SecretSpec::new(vec![
                    SecretSource::Env { env: "QWEN_BASE_URL".into() },
                    SecretSource::Dc {
                        dc: DcKey {
                            subject: "services".into(),
                            key: "apps.watching_qwen_base_url".into(),
                        },
                    },
                ]);
                resolve_with(&spec, env, dc).map(|s| s.expose().to_string())
            }
        };
        let base = base.unwrap_or_else(|| DEFAULT_BASE.to_string());
        let base = if base.starts_with("http://") || base.starts_with("https://") {
            base
        } else {
            format!("https://{base}")
        };
        let key_spec = config
            .key
            .clone()
            .map(SecretSpec::new)
            .unwrap_or_else(default_key_spec);
        let key = resolve_with(&key_spec, env, dc).ok_or_else(|| {
            eyre!(
                "no inference API key resolved; candidates: {}",
                key_spec.describe().join(", ")
            )
        })?;
        let mut observer_models = config.observer_models.clone().unwrap_or_default();
        if observer_models.is_empty() {
            if let Some(env_model) = env("MEDIA_OBSERVER_MODEL").filter(|m| !m.trim().is_empty())
            {
                observer_models.push(env_model.trim().to_string());
            }
            observer_models
                .extend(DEFAULT_OBSERVER_MODELS.iter().map(|m| (*m).to_string()));
        }
        Ok(InferenceConfig {
            base,
            model,
            key,
            observer_models,
        })
    }

    /// The wire profile for [`crate::llm_profiles::complete_with_key`].
    pub fn profile(&self) -> Profile {
        Profile {
            name: "alibaba-inference".into(),
            provider: "alibaba".into(),
            base_url: self.base.clone(),
            model: self.model.clone(),
            api_key_env: None,
            local: false,
        }
    }

    /// One chat completion through the inference profile. The credential is
    /// the resolved [`Secret`] — env references stay untouched.
    pub async fn complete(
        &self,
        system: &str,
        user: &str,
        options: &HashMap<String, serde_yaml::Value>,
        attachments: &[LoadedAttachment],
    ) -> Result<String> {
        llm_profiles::complete_with_key(
            &self.profile(),
            Some(self.key.expose()),
            system,
            user,
            &self.model,
            options,
            attachments,
        )
        .await
    }
}

/// Ordered key sources for inference: env first, then the dc store (names
/// only — values are resolved through [`crate::secrets`]).
fn default_key_spec() -> SecretSpec {
    SecretSpec::new(vec![
        SecretSource::Env { env: "QWEN_API_KEY".into() },
        SecretSource::Env { env: "DASHSCOPE_API_KEY".into() },
        SecretSource::Dc {
            dc: DcKey {
                subject: "services".into(),
                key: "apps.npl_mcp_qwen_api_key".into(),
            },
        },
        SecretSource::Dc {
            dc: DcKey {
                subject: "services".into(),
                key: "apps.npl_mcp_dashscope_api_key".into(),
            },
        },
    ])
}

fn resolve_with(
    spec: &SecretSpec,
    env: &dyn Fn(&str) -> Option<String>,
    dc: &dyn Fn(&str, &str) -> std::io::Result<crate::secrets::DcOutput>,
) -> Option<Secret> {
    crate::secrets::resolve_with(spec, env, dc)
}

/// The real `dc` runner handed to [`crate::secrets`].
fn real_dc() -> impl Fn(&str, &str) -> std::io::Result<crate::secrets::DcOutput> {
    |subject: &str, key: &str| {
        let output = std::process::Command::new("dc")
            .args(["get", subject, key, "--reveal", "--raw"])
            .output()?;
        Ok(crate::secrets::DcOutput {
            success: output.status.success(),
            stdout: output.stdout,
        })
    }
}

// ---------------------------------------------------------------------------
// Vision observer
// ---------------------------------------------------------------------------

/// A successful observation: the model that answered and the factual
/// per-criterion description (JSON text).
#[derive(Debug, Clone)]
pub struct Observed {
    pub model: String,
    pub description: String,
}

pub struct Observer {
    pub inference: InferenceConfig,
}

impl Observer {
    pub fn new(inference: InferenceConfig) -> Self {
        Observer { inference }
    }

    /// Describe the artifact against each criterion, trying the observer
    /// chain in order. Vision-only input: the caller attaches raster images
    /// (a generated image, or frames extracted from a video).
    pub async fn describe(
        &self,
        criteria: &[(String, String)],
        attachments: &[LoadedAttachment],
        verbose: bool,
    ) -> Result<Observed> {
        if attachments.is_empty() {
            bail!("observer needs at least one image attachment");
        }
        let criteria_list: String = criteria
            .iter()
            .map(|(name, description)| format!("  - {name}: {description}\n"))
            .collect();
        let instruction = format!(
            "You are a factual visual observer. Do NOT judge quality and do NOT assign scores.\n\
             For EACH criterion below, describe precisely what the image(s) actually show that is \
             relevant to it — objects, shapes, text, colors, layout, visible defects. Quote any \
             visible text verbatim. Stay factual; evaluation happens elsewhere.\n\n\
             Criteria:\n{}\
             Reply ONLY with valid JSON (no markdown, no fences):\n\
             {{\"observations\": {{\"<criterion name>\": \"<description>\", ...}}}}",
            criteria_list
        );
        let options =HashMap::from([(
            "max_tokens".to_string(),
            serde_yaml::Value::Number(2048.into()),
        )]);
        let mut last_error: Option<color_eyre::Report> = None;
        for model in &self.inference.observer_models {
            let mut profile = self.inference.profile();
            profile.model = model.clone();
            match llm_profiles::complete_with_key(
                &profile,
                Some(self.inference.key.expose()),
                "You are a factual visual observer. You describe exactly what images show.",
                &instruction,
                model,
                &options,
                attachments,
            )
            .await
            {
                Ok(raw) => match validate_observations(&raw, criteria) {
                    Ok(description) => {
                        return Ok(Observed {
                            model: model.clone(),
                            description,
                        })
                    }
                    Err(e) => {
                        if verbose {
                            crate::telemetry::verbose(&format!(
                                "Observer {} returned unusable observations ({e:#}); trying next candidate",
                                model
                            ));
                        }
                        last_error = Some(e);
                    }
                },
                Err(e) => {
                    if verbose {
                        crate::telemetry::verbose(&format!(
                            "Observer {} failed ({e:#}); trying next candidate",
                            model
                        ));
                    }
                    last_error = Some(e);
                }
            }
        }
        Err(last_error
            .unwrap_or_else(|| eyre!("no observer model configured")))
        .wrap_err("observe step failed for every configured observer model")
    }
}

/// The observations object must be real JSON covering every criterion; the
/// description keeps only the JSON text so the judge state stays compact.
fn validate_observations(raw: &str, criteria: &[(String, String)]) -> Result<String> {
    let cleaned = strip_fences(raw.trim());
    let value: Value = serde_json::from_str(&cleaned)
        .map_err(|e| eyre!("observations are not valid JSON: {e}"))?;
    let observations = value
        .get("observations")
        .and_then(|o| o.as_object())
        .ok_or_else(|| eyre!("observations object missing"))?;
    let mut missing: Vec<&str> = Vec::new();
    for (name, _) in criteria {
        match observations.get(name).and_then(|v| v.as_str()) {
            Some(text) if !text.trim().is_empty() => {}
            _ => missing.push(name),
        }
    }
    if !missing.is_empty() {
        bail!("criteria without observations: {}", missing.join(", "));
    }
    Ok(cleaned)
}

fn strip_fences(s: &str) -> String {
    let s = s.trim();
    if s.starts_with("```") {
        let after_fence = if let Some(nl) = s.find('\n') {
            &s[nl + 1..]
        } else {
            s
        };
        return after_fence.trim_end_matches("```").trim_end().to_string();
    }
    s.to_string()
}

// ---------------------------------------------------------------------------
// Content helpers
// ---------------------------------------------------------------------------

/// Pull text and image parts out of an OpenAI-style `content` value so the
/// same builders keep serving both the legacy POST path and the inference
/// profile path.
pub(crate) fn split_chat_content(content: &Value) -> (String, Vec<LoadedAttachment>) {
    match content {
        Value::String(text) => (text.clone(), Vec::new()),
        Value::Array(parts) => {
            let mut text = String::new();
            let mut attachments = Vec::new();
            for part in parts {
                if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(t);
                } else if let Some(url) = part
                    .get("image_url")
                    .and_then(|i| i.get("url"))
                    .and_then(|v| v.as_str())
                {
                    if let Some(attachment) = attachment_from_data_url(url) {
                        attachments.push(attachment);
                    }
                }
            }
            (text, attachments)
        }
        _ => (String::new(), Vec::new()),
    }
}

/// Decode a `data:<mime>;base64,<payload>` URL into an attachment.
fn attachment_from_data_url(url: &str) -> Option<LoadedAttachment> {
    let rest = url.strip_prefix("data:")?;
    let (mime, payload) = rest.split_once(";base64,")?;
    Some(LoadedAttachment {
        path: String::new(),
        role: "eval".into(),
        mime_type: mime.to_string(),
        description: String::new(),
        data_b64: payload.to_string(),
    })
}

/// Load a raster artifact (image or extracted video frame) for the observer.
pub fn load_image_attachment(path: &std::path::Path) -> Result<LoadedAttachment> {
    let data = std::fs::read(path).wrap_err_with(|| {
        format!("cannot read artifact for observation: {}", path.display())
    })?;
    use base64::Engine;
    Ok(LoadedAttachment {
        path: path.display().to_string(),
        role: "eval".into(),
        mime_type: crate::attachments::resolve_mime_type(path, None).to_string(),
        description: String::new(),
        data_b64: base64::engine::general_purpose::STANDARD.encode(&data),
    })
}

/// Criterion names + descriptions for the observer instruction.
pub fn criterion_brief(eval: &crate::schema::EvalSection) -> Vec<(String, String)> {
    let mut names: Vec<&String> = eval.criteria.keys().collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let criterion: &EvalCriterion = &eval.criteria[name];
            (
                name.clone(),
                criterion
                    .description
                    .clone()
                    .unwrap_or_else(|| name.clone()),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;
    use crate::secrets::DcOutput;
    use std::collections::HashMap as StdMap;
    use std::io;

    fn env_lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: StdMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| pairs.get(name).cloned()
    }

    fn dc_none() -> impl Fn(&str, &str) -> io::Result<DcOutput> {
        |_s, _k| Err(io::Error::new(io::ErrorKind::NotFound, "no dc"))
    }

    fn dc_ok(value: &str) -> impl Fn(&str, &str) -> io::Result<DcOutput> + '_ {
        let value = value.to_string();
        move |_s, _k| {
            Ok(DcOutput {
                success: true,
                stdout: value.clone().into_bytes(),
            })
        }
    }

    #[test]
    fn resolution_defaults_to_qwen_flash_and_dashscope_base() {
        let cfg = InferenceConfig::resolve_with(None, &env_lookup(&[("QWEN_API_KEY", "k")]), &dc_none())
            .unwrap();
        assert_eq!(cfg.model, "qwen3.8-flash");
        assert_eq!(cfg.base, DEFAULT_BASE);
        assert_eq!(cfg.key.expose(), "k");
        assert_eq!(cfg.observer_models[0], "qwen3.8-flash");
        assert!(cfg.observer_models.contains(&"qwen-vl-plus".to_string()));
    }

    #[test]
    fn env_and_dc_precede_compiled_defaults() {
        let env = env_lookup(&[
            ("QWEN_BASE_URL", "workspace.example/compatible-mode/v1"),
            ("MEDIA_INFERENCE_MODEL", "cfg-chat-model"),
            ("MEDIA_OBSERVER_MODEL", "qwen-vl-max"),
            ("QWEN_API_KEY", "env-key"),
        ]);
        let cfg = InferenceConfig::resolve_with(None, &env, &dc_none()).unwrap();
        // Scheme-less dc/env hosts normalize to https (real apibases are bare).
        assert_eq!(cfg.base, "https://workspace.example/compatible-mode/v1");
        assert_eq!(cfg.model, "cfg-chat-model");
        assert_eq!(cfg.key.expose(), "env-key");
        // Env observer model prepends the compiled chain.
        assert_eq!(cfg.observer_models[0], "qwen-vl-max");
        assert!(cfg.observer_models.contains(&"qwen3.8-flash".to_string()));

        // No env at all: base and key fall through to the dc store in order.
        let cfg = InferenceConfig::resolve_with(None, &env_lookup(&[]), &dc_ok("dc-fallback"))
            .unwrap();
        assert_eq!(cfg.base, "https://dc-fallback");
        assert_eq!(cfg.key.expose(), "dc-fallback");
    }

    #[test]
    fn config_section_overrides_the_resolution_chain() {
        let config = crate::provider_config::InferenceSection {
            model: Some("section-model".into()),
            base: Some("https://literal.example/v1".into()),
            key: Some(vec![SecretSource::Env { env: "CUSTOM_INFER_KEY".into() }]),
            observer_models: Some(vec!["vl-a".into(), "vl-b".into()]),
        };
        let cfg = InferenceConfig::resolve_with(
            Some(&config),
            &env_lookup(&[("CUSTOM_INFER_KEY", "custom-key")]),
            &dc_ok("ignored"),
        )
        .unwrap();
        assert_eq!(cfg.model, "section-model");
        assert_eq!(cfg.base, "https://literal.example/v1");
        assert_eq!(cfg.key.expose(), "custom-key");
        assert_eq!(cfg.observer_models, vec!["vl-a".to_string(), "vl-b".to_string()]);
    }

    #[test]
    fn missing_credentials_fail_closed_with_source_names_only() {
        let err = InferenceConfig::resolve_with(None, &env_lookup(&[]), &dc_none())
            .expect_err("must fail");
        let text = format!("{err:#}");
        assert!(text.contains("QWEN_API_KEY"), "{text}");
        assert!(text.contains("apps.npl_mcp_dashscope_api_key"), "{text}");
    }

    #[test]
    fn observer_rejects_observations_missing_criteria() {
        let criteria = vec![
            ("validity".to_string(), "well-formed".to_string()),
            ("relevance".to_string(), "matches brief".to_string()),
        ];
        let good = validate_observations(
            "```json\n{\"observations\": {\"validity\": \"circle present\", \"relevance\": \"a circle\"}}\n```",
            &criteria,
        )
        .unwrap();
        assert!(good.contains("\"relevance\""));
        assert!(!good.contains("```"));
        assert!(validate_observations(
            "{\"observations\": {\"validity\": \"circle present\"}}",
            &criteria,
        )
        .is_err());
        assert!(validate_observations("not json at all", &criteria).is_err());
        assert!(validate_observations(
            "{\"observations\": {\"validity\": \"\", \"relevance\": \"x\"}}",
            &criteria,
        )
        .is_err());
    }

    #[test]
    fn split_chat_content_extracts_text_and_images() {
        let (text, attachments) = split_chat_content(&json!("plain instruction"));
        assert_eq!(text, "plain instruction");
        assert!(attachments.is_empty());

        let url = "data:image/png;base64,aGVsbG8=";
        let (text, attachments) = split_chat_content(&json!([
            {"type": "text", "text": "part one"},
            {"type": "text", "text": "part two"},
            {"type": "image_url", "image_url": {"url": url}},
        ]));
        assert_eq!(text, "part one\npart two");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].mime_type, "image/png");
        assert_eq!(attachments[0].data_b64, "aGVsbG8=");
    }

    #[test]
    fn criterion_brief_is_sorted_and_defaults_description() {
        let mut criteria = std::collections::HashMap::new();
        criteria.insert(
            "z-crit".to_string(),
            EvalCriterion {
                weight: None,
                scale: None,
                description: None,
                fail_signals: vec![],
            },
        );
        criteria.insert(
            "a-crit".to_string(),
            EvalCriterion {
                weight: None,
                scale: None,
                description: Some("first".into()),
                fail_signals: vec![],
            },
        );
        let eval = crate::schema::EvalSection {
            criteria,
            ..Default::default()
        };
        let brief = criterion_brief(&eval);
        assert_eq!(brief[0].0, "a-crit");
        assert_eq!(brief[0].1, "first");
        assert_eq!(brief[1].0, "z-crit");
        assert_eq!(brief[1].1, "z-crit");
    }
}
