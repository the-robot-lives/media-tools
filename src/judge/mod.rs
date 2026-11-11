//! Decision-model judge: Alibaba `decision-model-preview` (default) and hosted
//! Jev over the shared TypeSafe System One wire contract.
//!
//! [`JudgeConfig::resolve`] layers CLI overrides > runtime `judge:` config >
//! environment > dc store > backend defaults; credentials and base URLs resolve
//! through [`crate::secrets`] and never enter logs or error strings. [`classify`]
//! posts one System One request per output and strictly validates the returned
//! distributions before they are remapped onto the prompt's eval rubric.
//!
//! Phase 1 scope: text-like outputs only (svg/html/txt/md/code). Image, video
//! and audio need an observe -> describe step that lands in Phase 2.

pub mod system_one;

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::Path;
use std::time::Duration;

use color_eyre::eyre::{bail, eyre, Context, Result};
use serde_json::{json, Value};

use crate::eval::{build_score, EvalScore};
use crate::schema::EvalSection;
use crate::secrets::{resolve_with as resolve_secret_with, DcOutput, Secret, SecretSource, SecretSpec};
pub use system_one::{
    decode, encode, Answer, AnswerKind, JudgeResult, PolicyOverride, Question, QuestionKind,
    ScoreLevel,
};

// ---------------------------------------------------------------------------
// Backend identity + defaults
// ---------------------------------------------------------------------------

/// Which decision-judge service answers the System One request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionBackend {
    Alibaba,
    Jev,
}

/// Literal Jev fallback when neither env nor config supplies a base URL.
const JEV_DEFAULT_BASE: &str = "https://api.typesafe.ai/v1";

/// DashScope intl OpenAI-compatible root — final fallback when neither env
/// (`QWEN_DECISION_BASE_URL`) nor the dc store (`services
/// apps.watching_qwen_base_url`, the dedicated decision workspace endpoint)
/// carries a base. The workspace System One endpoint hangs off the same
/// compatible-mode surface the rest of the DashScope traffic uses.
const ALIBABA_DEFAULT_BASE: &str = "https://dashscope-intl.aliyuncs.com/compatible-mode/v1";

impl DecisionBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionBackend::Alibaba => "alibaba",
            DecisionBackend::Jev => "jev",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            DecisionBackend::Alibaba => "decision-model-preview",
            DecisionBackend::Jev => "jev-latest",
        }
    }

    /// Ordered key sources: env vars first, then the dc store (names only —
    /// values are resolved through [`crate::secrets`]).
    pub fn key_spec(&self) -> SecretSpec {
        let sources: Vec<SecretSource> = match self {
            DecisionBackend::Alibaba => vec![
                SecretSource::Env { env: "QWEN_API_KEY".into() },
                SecretSource::Env { env: "DASHSCOPE_API_KEY".into() },
                SecretSource::Dc {
                    dc: crate::secrets::DcKey {
                        subject: "services".into(),
                        key: "apps.npl_mcp_qwen_api_key".into(),
                    },
                },
                SecretSource::Dc {
                    dc: crate::secrets::DcKey {
                        subject: "services".into(),
                        key: "apps.npl_mcp_dashscope_api_key".into(),
                    },
                },
            ],
            DecisionBackend::Jev => vec![
                SecretSource::Env { env: "JEV_API_KEY".into() },
                SecretSource::Env { env: "TYPESAFE_API_KEY".into() },
                SecretSource::Dc {
                    dc: crate::secrets::DcKey {
                        subject: "defaults".into(),
                        key: "inference.jev.key".into(),
                    },
                },
            ],
        };
        SecretSpec::new(sources)
    }

    /// Ordered base-URL sources. The decision workspace rides the dedicated
    /// Alibaba Model Studio compatible-mode endpoint (the one the codemoshiach
    /// C1 classifier uses); Jev falls back to the hosted TypeSafe base.
    pub fn base_spec(&self) -> SecretSpec {
        let sources: Vec<SecretSource> = match self {
            DecisionBackend::Alibaba => vec![
                SecretSource::Env { env: "QWEN_DECISION_BASE_URL".into() },
                SecretSource::Dc {
                    dc: crate::secrets::DcKey {
                        subject: "services".into(),
                        key: "apps.watching_qwen_base_url".into(),
                    },
                },
            ],
            DecisionBackend::Jev => vec![SecretSource::Env { env: "JEV_BASE_URL".into() }],
        };
        SecretSpec::new(sources)
    }
}

/// Parse a `--judge`/config backend name (case-insensitive).
pub fn parse_backend(value: &str) -> Result<DecisionBackend> {
    match value.trim().to_ascii_lowercase().as_str() {
        "alibaba" | "qwen" => Ok(DecisionBackend::Alibaba),
        "jev" | "typesafe" => Ok(DecisionBackend::Jev),
        other => return Err(eyre!("unknown judge backend '{other}'; expected alibaba or jev")),
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// A fully resolved judge connection. The key is a redacting [`Secret`].
#[derive(Debug, Clone)]
pub struct JudgeConfig {
    pub backend: DecisionBackend,
    pub model: String,
    pub base: String,
    pub key: Secret,
    /// Distribution policy: `Auto` gates the rounded-hundredths policy by
    /// backend/model; `Strict` / `RoundedHundredths` force one explicitly
    /// (`judge: policy: strict|rounded` in the config section).
    pub policy: PolicyOverride,
}

/// Parse the `judge: policy:` config knob.
fn parse_policy(value: Option<&str>) -> Result<PolicyOverride> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(PolicyOverride::Auto),
        Some("strict") => Ok(PolicyOverride::Strict),
        Some("rounded" | "rounded-hundredths" | "rounded_hundredths") => {
            Ok(PolicyOverride::RoundedHundredths)
        }
        Some(other) => {
            return Err(eyre!("unknown judge policy '{other}'; expected strict or rounded"))
        }
    }
}

impl JudgeConfig {
    /// CLI > `judge:` config section > environment > dc store > backend
    /// default. Missing credentials fail closed with source *names* in the
    /// message, never values.
    pub fn resolve(
        backend_override: Option<&str>,
        model_override: Option<&str>,
        config: Option<&crate::provider_config::JudgeSection>,
    ) -> Result<JudgeConfig> {
        let env = |name: &str| std::env::var(name).ok();
        Self::resolve_with(backend_override, model_override, config, &env, &real_dc())
    }

    /// Injectable core (tests): explicit env + dc runners.
    pub(crate) fn resolve_with(
        backend_override: Option<&str>,
        model_override: Option<&str>,
        config: Option<&crate::provider_config::JudgeSection>,
        env: &dyn Fn(&str) -> Option<String>,
        dc: &dyn Fn(&str, &str) -> io::Result<DcOutput>,
    ) -> Result<JudgeConfig> {
        let config = config.cloned().unwrap_or_default();
        let backend = backend_override
            .or(config.backend.as_deref())
            .map(parse_backend)
            .transpose()?
            .unwrap_or(DecisionBackend::Alibaba);
        let model = model_override
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_string)
            .or_else(|| config.model.clone())
            .unwrap_or_else(|| backend.default_model().to_string());
        let base = match config.base.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            Some(url) => url.to_string(),
            None => {
                let spec = backend.base_spec();
                match resolve_secret_with(&spec, env, dc) {
                    Some(secret) => secret.expose().to_string(),
                    None => match backend {
                        DecisionBackend::Jev => JEV_DEFAULT_BASE.to_string(),
                        DecisionBackend::Alibaba => ALIBABA_DEFAULT_BASE.to_string(),
                    },
                }
            }
        };
        // The dc store keeps some apibases scheme-less (bare MaaS hosts);
        // assume https so endpoint validation stays strict downstream.
        let base = if base.starts_with("http://") || base.starts_with("https://") {
            base
        } else {
            format!("https://{base}")
        };
        let key_spec = config
            .key
            .clone()
            .map(SecretSpec::new)
            .unwrap_or_else(|| backend.key_spec());
        let key = resolve_secret_with(&key_spec, env, dc).ok_or_else(|| {
            eyre!(
                "no judge API key resolved; candidates: {}",
                key_spec.describe().join(", ")
            )
        })?;
        let policy = parse_policy(config.policy.as_deref())?;
        Ok(JudgeConfig {
            backend,
            model,
            base,
            key,
            policy,
        })
    }
}

/// The real `dc` runner handed to [`crate::secrets`].
fn real_dc() -> impl Fn(&str, &str) -> io::Result<DcOutput> {
    |subject: &str, key: &str| {
        let output = std::process::Command::new("dc")
            .args(["get", subject, key, "--reveal", "--raw"])
            .output()?;
        Ok(DcOutput {
            success: output.status.success(),
            stdout: output.stdout,
        })
    }
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// One System One request: `POST {base}/systemone` with bearer auth, then
/// strict decode (the rounded-hundredths policy applies itself when the
/// Alibaba decision model answers).
pub async fn classify(
    cfg: &JudgeConfig,
    state: &Value,
    questions: &[Question],
) -> Result<JudgeResult> {
    let endpoint = system_one::build_endpoint(&cfg.base).map_err(|e| eyre!("{e}"))?;
    let wire = system_one::encode(&cfg.model, state, questions).map_err(|e| eyre!("{e}"))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .build()?;
    let response = client
        .post(&endpoint)
        .bearer_auth(cfg.key.expose())
        .json(&wire)
        .send()
        .await
        .map_err(|_| eyre!("judge endpoint unreachable: {endpoint}"))?;
    let status = response.status();
    if !status.is_success() {
        // The body may echo credentials; the status code is the only detail.
        bail!(
            "judge endpoint returned HTTP {} (check credentials and model access)",
            status.as_u16()
        );
    }
    let raw: Value = response
        .json()
        .await
        .map_err(|_| eyre!("judge endpoint returned invalid JSON"))?;
    decode(&raw, &cfg.model, cfg.backend, questions, cfg.policy).map_err(|e| eyre!("{e}"))
}

// ---------------------------------------------------------------------------
// Rubric builder
// ---------------------------------------------------------------------------

/// An eval section flattened into wire questions.
#[derive(Debug, Clone, Default)]
pub struct Rubric {
    pub questions: Vec<Question>,
    /// wire question id -> criterion name (score questions).
    pub criteria: BTreeMap<String, String>,
    /// wire question id -> reject_if statement (noul questions).
    pub rejects: BTreeMap<String, String>,
}

/// One `score` question per criterion (10 levels mapped 0-1), one `noul`
/// question per reject_if entry.
pub fn build_rubric(eval: &EvalSection) -> Rubric {
    let mut names: Vec<&String> = eval.criteria.keys().collect();
    names.sort();
    let mut rubric = Rubric::default();
    for name in names {
        let criterion = &eval.criteria[name];
        let description = criterion
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .unwrap_or(name);
        let levels: Vec<ScoreLevel> = (0..system_one::MAX_SCORE_LEVELS)
            .map(|index| ScoreLevel {
                score: index as f64 / 9.0,
                label: score_level_label(index, description, &criterion.fail_signals),
            })
            .collect();
        let instructions = score_instructions(description, &criterion.fail_signals);
        let id = format!("criterion:{name}");
        rubric.questions.push(
            Question::new(id.clone(), QuestionKind::Score(levels))
                .with_instructions(instructions),
        );
        rubric.criteria.insert(id, name.clone());
    }
    for (index, condition) in eval.reject_if.iter().enumerate() {
        let id = format!("reject:{index:02}");
        rubric.questions.push(
            Question::new(id.clone(), QuestionKind::Noul).with_instructions(format!(
                "Answer true if the output violates this rejection condition: \"{condition}\"; otherwise false."
            )),
        );
        rubric.rejects.insert(id, condition.clone());
    }
    rubric
}

fn score_level_label(index: usize, description: &str, fail_signals: &[String]) -> String {
    match index {
        0 => {
            let mut label = format!("0 — total failure: {description}");
            if !fail_signals.is_empty() {
                label.push_str(&format!("; fail signals: {}", fail_signals.join("; ")));
            }
            label
        }
        9 => format!("9 — excellent: {description}"),
        _ => format!("{index} — partial: {description} (level {index} of 9)"),
    }
}

fn score_instructions(description: &str, fail_signals: &[String]) -> String {
    let mut text = format!(
        "Grade the output against this criterion: \"{description}\". Pick the rubric level \
         (0 = total failure, 9 = excellent) that best matches."
    );
    if !fail_signals.is_empty() {
        text.push_str(&format!(
            " Any of these fail signals pushes the grade toward the lowest levels: {}.",
            fail_signals.join("; ")
        ));
    }
    text
}

/// Map judge answers onto the prompt's eval semantics: per-criterion expected
/// scores (already 0-1) are scaled to the 0-10 wire format [`build_score`]
/// normalizes, noul "true" answers become reject hits, and the resulting
/// [`EvalScore`] keeps `passes`/threshold behavior unchanged.
pub fn score_result(
    result: &JudgeResult,
    rubric: &Rubric,
    eval: &EvalSection,
    backend_label: &str,
) -> EvalScore {
    let mut scores = serde_json::Map::new();
    for answer in &result.answers {
        if let Some(criterion) = rubric.criteria.get(&answer.question_id) {
            let expected = match answer.kind {
                AnswerKind::Score { expected_score } => expected_score,
                _ => 0.0,
            };
            scores.insert(
                criterion.clone(),
                json!(expected.clamp(0.0, 1.0) * 10.0),
            );
        }
    }
    let reject_hits: Vec<String> = result
        .answers
        .iter()
        .filter_map(|answer| match (&answer.kind, rubric.rejects.get(&answer.question_id)) {
            (AnswerKind::Noul { value: true, .. }, Some(condition)) => Some(condition.clone()),
            _ => None,
        })
        .collect();
    let notes = format!(
        "judge={}:{} request_id={}",
        backend_label,
        result.model,
        result.request_id.as_deref().unwrap_or("none")
    );
    let parsed = json!({
        "scores": Value::Object(scores),
        "reject_hits": reject_hits,
        "notes": notes,
    });
    build_score(&parsed, eval, false).unwrap_or(EvalScore {
        weighted: 0.0,
        per_criterion: HashMap::new(),
        reject_hits,
        notes,
    })
}

// ---------------------------------------------------------------------------
// CLI surface
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Text,
    Image,
    Video,
    Audio,
    Unknown,
}

/// Phase 1 judges text-like outputs only. svg is text (it *is* code); raster
/// formats, video and audio need the Phase 2 observe step.
pub fn output_kind(extension: &str) -> OutputKind {
    match extension.to_ascii_lowercase().as_str() {
        "svg" | "html" | "htm" | "txt" | "md" | "markdown" | "css" | "js" | "jsx" | "ts"
        | "tsx" | "mjs" | "cjs" | "json" | "yaml" | "yml" | "py" | "rb" | "rs" | "go" | "sh"
        | "ex" | "exs" | "erl" | "h" | "c" | "hpp" | "cpp" | "java" | "kt" | "swift" | "lua"
        | "php" | "tape" | "toml" | "semtext" => OutputKind::Text,
        "png" | "jpg" | "jpeg" | "webp" | "gif" => OutputKind::Image,
        "mp3" | "wav" | "ogg" | "flac" => OutputKind::Audio,
        "mp4" | "webm" | "avi" | "mov" => OutputKind::Video,
        _ => OutputKind::Unknown,
    }
}

/// Raised so the CLI shell can map image/video/audio grading to exit code 2.
#[derive(Debug)]
pub struct ObserveRequired(pub &'static str);

impl std::fmt::Display for ObserveRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} outputs need an observe step — observe step lands in Phase 2",
            self.0
        )
    }
}

impl std::error::Error for ObserveRequired {}

/// One graded criterion row for rendering.
#[derive(Debug, Clone)]
pub struct CriterionRow {
    pub name: String,
    pub weight: f64,
    /// Expected score normalized to 0-1.
    pub score: f64,
    /// Ordered level probabilities (10 levels, low -> high).
    pub probabilities: Vec<f64>,
    pub level_labels: Vec<String>,
    pub wire_expected_index: Option<f64>,
    pub rounded: bool,
    pub provider_confidence: Option<f64>,
}

/// The full judged verdict for a single output file.
#[derive(Debug, Clone)]
pub struct JudgeVerdict {
    pub backend: &'static str,
    pub model: String,
    pub request_id: Option<String>,
    pub latency_ms: Option<f64>,
    pub usage: BTreeMap<String, u64>,
    pub weighted: f64,
    pub threshold: f64,
    pub passes: bool,
    pub criteria: Vec<CriterionRow>,
    pub reject_hits: Vec<String>,
    pub notes: String,
}

impl JudgeVerdict {
    pub fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Judge: {}:{} request_id={} latency_ms={}\n",
            self.backend,
            self.model,
            self.request_id.as_deref().unwrap_or("none"),
            self.latency_ms
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "n/a".to_string()),
        ));
        for row in &self.criteria {
            let probs = row
                .probabilities
                .iter()
                .map(|p| format!("{p:.3}"))
                .collect::<Vec<_>>()
                .join(" ");
            out.push_str(&format!(
                "  {:<24} score {:>5.2}/1.00 (weight {:.1}){} {}\n    probabilities: {}\n",
                row.name,
                row.score,
                row.weight,
                if row.rounded { " [rounded]" } else { "" },
                row.provider_confidence
                    .map(|c| format!("(confidence {c:.2})"))
                    .unwrap_or_default(),
                probs,
            ));
        }
        if !self.reject_hits.is_empty() {
            out.push_str(&format!("  reject hits: {}\n", self.reject_hits.join(", ")));
        } else {
            out.push_str("  reject hits: (none)\n");
        }
        out.push_str(&format!(
            "Weighted score {:.3} / threshold {:.2} — {}\n",
            self.weighted,
            self.threshold,
            if self.passes { "PASS" } else { "FAIL" }
        ));
        out.push_str(&format!("  {}\n", self.notes));
        out
    }

    pub fn to_json_pretty(&self) -> String {
        let value = json!({
            "backend": self.backend,
            "model": self.model,
            "request_id": self.request_id,
            "latency_ms": self.latency_ms,
            "usage": self.usage,
            "weighted": self.weighted,
            "threshold": self.threshold,
            "passes": self.passes,
            "reject_hits": self.reject_hits,
            "notes": self.notes,
            "criteria": self.criteria.iter().map(|row| json!({
                "name": row.name,
                "weight": row.weight,
                "score": row.score,
                "probabilities": row.probabilities,
                "level_labels": row.level_labels,
                "wire_expected_index": row.wire_expected_index,
                "rounded": row.rounded,
                "provider_confidence": row.provider_confidence,
            })).collect::<Vec<_>>(),
        });
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Grade one generated output file against its prompt's eval rubric.
/// Text-like outputs only; image/video/audio raise [`ObserveRequired`].
pub async fn run_cli(
    prompt_path: &Path,
    output_path: &Path,
    backend_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<JudgeVerdict> {
    let parsed = crate::schema::parse_prompt_file(prompt_path)
        .wrap_err("cannot load prompt file for judging")?;
    let extension = output_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match output_kind(extension) {
        OutputKind::Text => {}
        OutputKind::Image => return Err(ObserveRequired("image").into()),
        OutputKind::Video => return Err(ObserveRequired("video").into()),
        OutputKind::Audio => return Err(ObserveRequired("audio").into()),
        OutputKind::Unknown => {
            return Err(eyre!(
                "output extension '.{extension}' is not judgeable text; the observe step lands in Phase 2"
            ))
        }
    }
    let content = std::fs::read_to_string(output_path)
        .wrap_err_with(|| format!("cannot read output file {}", output_path.display()))?;
    if content.trim().is_empty() {
        bail!("output file {} is empty", output_path.display());
    }
    let eval = parsed.payload.eval.clone().unwrap_or_default();
    if eval.criteria.is_empty() {
        bail!(
            "{} has no eval criteria to judge against",
            prompt_path.display()
        );
    }

    let rubric = build_rubric(&eval);
    let mut prompt_text = String::new();
    if let Some(system) = parsed.payload.prompt.system.as_deref().map(str::trim) {
        if !system.is_empty() {
            prompt_text.push_str(system);
            prompt_text.push('\n');
        }
    }
    prompt_text.push_str(parsed.payload.prompt.text.trim());
    let state = json!({
        "prompt": prompt_text,
        "output": content,
        "asset_type": format!("{:?}", parsed.meta.asset_type),
        "output_format": extension,
    });

    let config = crate::provider_config::loaded().map(|loaded| &loaded.judge);
    let cfg = JudgeConfig::resolve(backend_override, model_override, config)?;
    let result = classify(&cfg, &state, &rubric.questions).await?;

    let score = score_result(&result, &rubric, &eval, cfg.backend.as_str());
    let threshold = eval.effective_pass_threshold();
    let passes = score.passes(&eval);

    let mut criteria = Vec::new();
    for (question_id, name) in &rubric.criteria {
        let Some(answer) = result
            .answers
            .iter()
            .find(|a| &a.question_id == question_id)
        else {
            continue;
        };
        let weight = eval
            .criteria
            .get(name)
            .and_then(|c| c.weight)
            .unwrap_or(1.0);
        let AnswerKind::Score { expected_score } = answer.kind else {
            continue;
        };
        criteria.push(CriterionRow {
            name: name.clone(),
            weight,
            score: expected_score.clamp(0.0, 1.0),
            probabilities: answer.distribution.clone(),
            level_labels: eval
                .criteria
                .get(name)
                .map(|criterion| {
                    let description = criterion
                        .description
                        .as_deref()
                        .unwrap_or(name)
                        .trim()
                        .to_string();
                    (0..system_one::MAX_SCORE_LEVELS)
                        .map(|index| score_level_label(index, &description, &criterion.fail_signals))
                        .collect()
                })
                .unwrap_or_default(),
            wire_expected_index: answer.wire_expected_index,
            rounded: answer.rounded.is_some(),
            provider_confidence: answer.provider_confidence,
        });
    }
    criteria.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(JudgeVerdict {
        backend: cfg.backend.as_str(),
        model: result.model.clone(),
        request_id: result.request_id.clone(),
        latency_ms: result.latency_ms,
        usage: result.usage.clone(),
        weighted: score.weighted,
        threshold,
        passes,
        criteria,
        reject_hits: score.reject_hits.clone(),
        notes: score.notes.clone(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{EvalCriterion, EvalSection};
    use serde_json::json;
    use std::collections::HashMap;

    fn sample_eval() -> EvalSection {
        let mut criteria = HashMap::new();
        criteria.insert(
            "validity".to_string(),
            EvalCriterion {
                weight: Some(3.0),
                scale: None,
                description: Some("well-formed markup with no syntax errors".into()),
                fail_signals: vec!["unclosed tags".into()],
            },
        );
        criteria.insert(
            "relevance".to_string(),
            EvalCriterion {
                weight: Some(1.0),
                scale: None,
                description: Some("matches the brief".into()),
                fail_signals: vec![],
            },
        );
        EvalSection {
            pass_threshold: Some(0.7),
            max_attempts: None,
            required_pass: vec!["validity".into()],
            criteria,
            reject_if: vec!["watermark".into()],
            mode: None,
            visual: None,
        }
    }

    fn criterion_answer(question_id: &str, expected: f64, probabilities: Vec<f64>) -> Answer {
        Answer {
            question_id: question_id.to_string(),
            kind: AnswerKind::Score { expected_score: expected },
            distribution: probabilities,
            selected_probability: None,
            provider_confidence: Some(0.8),
            wire_expected_index: Some(8.1),
            policy: system_one::DistributionPolicy::Strict,
            rounded: None,
        }
    }

    fn noul_answer(question_id: &str, value: bool) -> Answer {
        Answer {
            question_id: question_id.to_string(),
            kind: AnswerKind::Noul {
                value,
                positive_probability: if value { 0.9 } else { 0.1 },
            },
            distribution: vec![0.1, 0.9],
            selected_probability: None,
            provider_confidence: None,
            wire_expected_index: None,
            policy: system_one::DistributionPolicy::Strict,
            rounded: None,
        }
    }

    #[test]
    fn rubric_maps_criteria_and_rejects_deterministically() {
        let rubric = build_rubric(&sample_eval());
        // Sorted criteria first, then reject_if noul questions.
        assert_eq!(rubric.questions.len(), 3);
        assert_eq!(rubric.questions[0].id, "criterion:relevance");
        assert_eq!(rubric.questions[1].id, "criterion:validity");
        assert_eq!(rubric.questions[2].id, "reject:00");
        assert_eq!(rubric.criteria["criterion:validity"], "validity");
        assert_eq!(rubric.rejects["reject:00"], "watermark");
        // Score levels: 10, mapped 0-1 in 1/9 steps.
        let QuestionKind::Score(levels) = &rubric.questions[1].kind else {
            panic!("expected score question");
        };
        assert_eq!(levels.len(), system_one::MAX_SCORE_LEVELS);
        assert_eq!(levels[0].score, 0.0);
        assert_eq!(levels[9].score, 1.0);
        // Labels are built from the criterion description and fail signals.
        assert!(levels[0].label.contains("total failure"));
        assert!(levels[0].label.contains("unclosed tags"));
        assert!(levels[9].label.contains("well-formed markup"));
        assert!(!rubric.questions[1]
            .instructions
            .as_deref()
            .unwrap_or_default()
            .contains("rejection"));
        assert!(rubric.questions[2]
            .instructions
            .as_deref()
            .unwrap_or_default()
            .contains("rejection condition: \"watermark\""));
    }

    #[test]
    fn score_result_reuses_build_score_and_passes_semantics() {
        let eval = sample_eval();
        let rubric = build_rubric(&eval);
        let result = JudgeResult {
            model: "decision-model-preview".into(),
            request_id: Some("req-1".into()),
            latency_ms: Some(12.0),
            usage: BTreeMap::new(),
            answers: vec![
                criterion_answer("criterion:validity", 0.9, vec![0.05; 10]),
                criterion_answer("criterion:relevance", 0.8, vec![0.1; 10]),
                noul_answer("reject:00", false),
            ],
        };
        let score = score_result(&result, &rubric, &eval, "alibaba");
        // (3 * 0.9 + 1 * 0.8) / 4
        assert!((score.weighted - 0.875).abs() < 1e-9, "{}", score.weighted);
        assert_eq!(score.per_criterion.get("validity"), Some(&0.9));
        assert!(score.reject_hits.is_empty());
        assert!(score.passes(&eval));
        assert!(score.notes.contains("judge=alibaba:decision-model-preview"));
        assert!(score.notes.contains("req-1"));

        // A true noul answer becomes a reject hit and fails the gate.
        let rejected = JudgeResult {
            answers: vec![
                criterion_answer("criterion:validity", 0.9, vec![0.05; 10]),
                criterion_answer("criterion:relevance", 0.8, vec![0.1; 10]),
                noul_answer("reject:00", true),
            ],
            ..result.clone()
        };
        let score = score_result(&rejected, &rubric, &eval, "alibaba");
        assert_eq!(score.reject_hits, vec!["watermark".to_string()]);
        assert!(!score.passes(&eval));
    }

    // ----- config resolution -----

    fn env_lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| pairs.get(name).cloned()
    }

    fn dc_none() -> impl Fn(&str, &str) -> io::Result<DcOutput> {
        |_subject, _key| Err(io::Error::new(io::ErrorKind::NotFound, "no dc"))
    }

    fn dc_ok(value: &str) -> impl Fn(&str, &str) -> io::Result<DcOutput> + '_ {
        let value = value.to_string();
        move |_subject, _key| {
            Ok(DcOutput {
                success: true,
                stdout: value.clone().into_bytes(),
            })
        }
    }

    #[test]
    fn backend_resolution_cli_over_config_over_default() {
        let config = crate::provider_config::JudgeSection {
            backend: Some("jev".into()),
            ..Default::default()
        };
        let cfg = JudgeConfig::resolve_with(
            Some("alibaba"),
            None,
            Some(&config),
            &env_lookup(&[("QWEN_API_KEY", "fixture-key"), ("JEV_API_KEY", "fixture-jev")]),
            &dc_none(),
        )
        .expect("resolve");
        assert_eq!(cfg.backend, DecisionBackend::Alibaba);
        assert_eq!(cfg.model, "decision-model-preview");
        assert_eq!(cfg.key.expose(), "fixture-key");

        let cfg = JudgeConfig::resolve_with(
            None,
            None,
            Some(&config),
            &env_lookup(&[("JEV_API_KEY", "fixture-jev")]),
            &dc_none(),
        )
        .expect("resolve");
        assert_eq!(cfg.backend, DecisionBackend::Jev);
        assert_eq!(cfg.model, "jev-latest");
        assert_eq!(cfg.base, JEV_DEFAULT_BASE);

        // Default backend is alibaba with no CLI/config input.
        let cfg = JudgeConfig::resolve_with(
            None,
            None,
            None,
            &env_lookup(&[("QWEN_API_KEY", "fixture-key")]),
            &dc_none(),
        )
        .expect("resolve");
        assert_eq!(cfg.backend, DecisionBackend::Alibaba);
    }

    #[test]
    fn model_resolution_cli_over_config_over_backend_default() {
        let config = crate::provider_config::JudgeSection {
            model: Some("cfg-model".into()),
            ..Default::default()
        };
        let cfg = JudgeConfig::resolve_with(
            None,
            Some("cli-model"),
            Some(&config),
            &env_lookup(&[("QWEN_API_KEY", "k")]),
            &dc_none(),
        )
        .unwrap();
        assert_eq!(cfg.model, "cli-model");
        let cfg = JudgeConfig::resolve_with(
            None,
            None,
            Some(&config),
            &env_lookup(&[("QWEN_API_KEY", "k")]),
            &dc_none(),
        )
        .unwrap();
        assert_eq!(cfg.model, "cfg-model");
    }

    #[test]
    fn alibaba_base_and_key_env_precede_dc_store() {
        let env = env_lookup(&[
            ("QWEN_DECISION_BASE_URL", "https://workspace.example/compatible-mode/v1"),
            ("QWEN_API_KEY", "env-key"),
            ("DASHSCOPE_API_KEY", "dash-key"),
        ]);
        let cfg = JudgeConfig::resolve_with(None, None, None, &env, &dc_ok("dc-value")).unwrap();
        assert_eq!(cfg.base, "https://workspace.example/compatible-mode/v1");
        assert_eq!(cfg.key.expose(), "env-key");

        // No env at all: base and key fall through to the dc store, in order.
        let cfg = JudgeConfig::resolve_with(None, None, None, &env_lookup(&[]), &dc_ok("dc-fallback")).unwrap();
        // Scheme-less dc values are normalized to https (real apibases are
        // stored as bare MaaS hosts).
        assert_eq!(cfg.base, "https://dc-fallback");
        assert_eq!(cfg.key.expose(), "dc-fallback");
    }

    #[test]
    fn config_base_and_key_override_the_resolution_chain() {
        let config = crate::provider_config::JudgeSection {
            base: Some("https://literal.example/v1/".into()),
            key: Some(vec![SecretSource::Env { env: "CUSTOM_JUDGE_KEY".into() }]),
            ..Default::default()
        };
        let cfg = JudgeConfig::resolve_with(
            None,
            None,
            Some(&config),
            &env_lookup(&[("CUSTOM_JUDGE_KEY", "custom-key")]),
            &dc_ok("ignored"),
        )
        .unwrap();
        assert_eq!(cfg.base, "https://literal.example/v1/");
        assert_eq!(cfg.key.expose(), "custom-key");
    }

    #[test]
    fn missing_credentials_fail_closed_with_source_names_only() {
        let err = JudgeConfig::resolve_with(None, None, None, &env_lookup(&[]), &dc_none())
            .expect_err("must fail");
        let text = format!("{err:#}");
        assert!(text.contains("QWEN_API_KEY"), "{text}");
        assert!(text.contains("apps.npl_mcp_dashscope_api_key"), "{text}");
        // No resolved values could appear here because resolution failed — the
        // guarantee is that source *names* are what diagnostics carry.
    }

    #[test]
    fn invalid_backend_names_are_rejected() {
        assert!(parse_backend("claude").is_err());
        assert_eq!(parse_backend("Jev").unwrap(), DecisionBackend::Jev);
        assert_eq!(parse_backend("alibaba").unwrap(), DecisionBackend::Alibaba);
    }

    #[test]
    fn policy_knob_resolves_strict_rounded_and_auto() {
        let strict = crate::provider_config::JudgeSection {
            policy: Some("strict".into()),
            ..Default::default()
        };
        let cfg = JudgeConfig::resolve_with(
            Some("jev"),
            None,
            Some(&strict),
            &env_lookup(&[("JEV_API_KEY", "k")]),
            &dc_none(),
        )
        .unwrap();
        assert_eq!(cfg.policy, PolicyOverride::Strict);

        let rounded = crate::provider_config::JudgeSection {
            policy: Some("rounded-hundredths".into()),
            ..Default::default()
        };
        let cfg = JudgeConfig::resolve_with(
            None,
            None,
            Some(&rounded),
            &env_lookup(&[("QWEN_API_KEY", "k")]),
            &dc_none(),
        )
        .unwrap();
        assert_eq!(cfg.policy, PolicyOverride::RoundedHundredths);

        // Absent/blank policy = auto-gated by backend/model.
        let cfg = JudgeConfig::resolve_with(
            None,
            None,
            None,
            &env_lookup(&[("QWEN_API_KEY", "k")]),
            &dc_none(),
        )
        .unwrap();
        assert_eq!(cfg.policy, PolicyOverride::Auto);
    }

    #[test]
    fn unknown_policy_names_are_rejected() {
        let bad = crate::provider_config::JudgeSection {
            policy: Some("lenient".into()),
            ..Default::default()
        };
        assert!(JudgeConfig::resolve_with(
            None,
            None,
            Some(&bad),
            &env_lookup(&[("QWEN_API_KEY", "k")]),
            &dc_none(),
        )
        .is_err());
    }
    // ----- end-to-end over an in-process mock System One endpoint -----

    #[tokio::test]
    async fn classify_via_mock_systemone_server() {
        use axum::routing::post;
        use axum::{Json, Router};

        // Legend labels must match the rubric's byte-for-byte — build them
        // with the same helper the rubric builder uses.
        let legend_for = |description: &str, fail_signals: &[&str]| -> Value {
            let signals: Vec<String> = fail_signals.iter().map(|s| s.to_string()).collect();
            let mut map = serde_json::Map::new();
            for index in 0..system_one::MAX_SCORE_LEVELS {
                map.insert(
                    index.to_string(),
                    json!(score_level_label(index, description, &signals)),
                );
            }
            Value::Object(map)
        };
        let levels_object = |entries: &[(&str, f64)]| -> Value {
            let mut map = serde_json::Map::new();
            for index in 0..system_one::MAX_SCORE_LEVELS {
                let key = index.to_string();
                let value = entries
                    .iter()
                    .find(|(id, _)| *id == key)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                map.insert(key, json!(value));
            }
            Value::Object(map)
        };

        let response = json!({
            "model": "decision-model-preview",
            "request_id": "mock-req-1",
            "latency_ms": 3.0,
            "usage": {"input_tokens": 40, "output_tokens": 8, "total_tokens": 48},
            "answers": {
                // 0.99 displayed mass -> rounded policy renormalizes; the
                // native score 8.2 sits inside the attainable band.
                "criterion:validity": {
                    "type": "score",
                    "score": 8.2,
                    "probabilities": levels_object(&[("8", 0.69), ("9", 0.3)]),
                    "legend": legend_for(
                        "well-formed markup with no syntax errors",
                        &["unclosed tags"],
                    ),
                    "confidence": 0.8
                },
                // Exact unit mass -> strict fast path, no renormalization.
                "criterion:relevance": {
                    "type": "score",
                    "score": 8.0,
                    "probabilities": levels_object(&[("8", 1.0)]),
                    "legend": legend_for("matches the brief", &[]),
                    "confidence": 0.9
                },
                "reject:00": {"type": "noul", "noul": 0.05}
            }
        });

        let captured: std::sync::Arc<
            std::sync::Mutex<Option<(Option<String>, Value)>>,
        > = std::sync::Arc::new(std::sync::Mutex::new(None));
        let state = captured.clone();
        let app = Router::new()
            .route(
                "/v1/systemone",
                post(
                    |axum::extract::State(sink): axum::extract::State<
                        std::sync::Arc<std::sync::Mutex<Option<(Option<String>, Value)>>>,
                    >,
                     headers: axum::http::HeaderMap,
                     Json(body): Json<Value>| async move {
                        let auth = headers
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_string);
                        *sink.lock().unwrap() = Some((auth, body));
                        Json(response.clone())
                    },
                ),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let env = env_lookup(&[("QWEN_API_KEY", "fixture-key")]);
        let cfg = JudgeConfig::resolve_with(None, None, None, &env, &dc_none()).unwrap();
        // Base carries the /v1 segment; classify appends /systemone exactly once.
        let cfg = JudgeConfig {
            base: format!("http://{addr}/v1"),
            ..cfg
        };

        let eval = sample_eval();
        let rubric = build_rubric(&eval);
        let result = classify(
            &cfg,
            &json!({"prompt": "fixture", "output": "<svg/>"}),
            &rubric.questions,
        )
        .await
        .expect("classify");

        // Captured request: bearer header, model id, wire shapes.
        let captured = captured.lock().unwrap().take().expect("handler ran");
        assert_eq!(captured.0.as_deref(), Some("Bearer fixture-key"));
        assert_eq!(captured.1["model"], "decision-model-preview");
        assert!(captured.1["questions"]["criterion:validity"]["criteria"]
            .as_array()
            .is_some_and(|levels| levels.len() == 10));
        assert_eq!(captured.1["questions"]["reject:00"]["type"], "noul");

        // Decoded result: identity, safe usage, auto-rounded policy.
        assert_eq!(result.request_id.as_deref(), Some("mock-req-1"));
        assert_eq!(result.model, "decision-model-preview");
        assert_eq!(result.usage.get("total_tokens"), Some(&48));
        let validity = result
            .answers
            .iter()
            .find(|a| a.question_id == "criterion:validity")
            .unwrap();
        assert!(validity.rounded.is_some(), "0.99 mass must renormalize");
        let sum = validity.distribution.iter().sum::<f64>();
        assert!((sum - 1.0).abs() < 1e-12);
        assert_eq!(validity.provider_confidence, Some(0.8));

        // Weighted score reuses build_score semantics: (3*v + 1*r) / 4 where
        // v = canonical index / 9 = 8.22/0.99/9 and r = 8/9.
        let score = score_result(&result, &rubric, &eval, "alibaba");
        let expected_validity = (0.69 * 8.0 + 0.3 * 9.0) / 0.99 / 9.0;
        let expected_weighted = (3.0 * expected_validity + 8.0 / 9.0) / 4.0;
        assert!(
            (score.weighted - expected_weighted).abs() < 1e-9,
            "weighted={} expected={}",
            score.weighted,
            expected_weighted
        );
        assert!(score.passes(&eval), "weighted {} must pass 0.7", score.weighted);
        assert!(score.reject_hits.is_empty());

        let _ = tx.send(());
    }

    #[tokio::test]
    async fn http_errors_report_status_without_secrets() {
        use axum::http::StatusCode;
        use axum::routing::post;
        use axum::{Json, Router};

        async fn systemone() -> (StatusCode, Json<Value>) {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "fixture-key was invalid"})),
            )
        }

        let app = Router::new().route("/systemone", post(systemone));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let env = env_lookup(&[("QWEN_API_KEY", "fixture-key")]);
        let cfg = JudgeConfig::resolve_with(None, None, None, &env, &dc_none()).unwrap();
        let cfg = JudgeConfig {
            base: format!("http://{addr}"),
            ..cfg
        };
        let eval = sample_eval();
        let rubric = build_rubric(&eval);
        let err = classify(&cfg, &json!("state"), &rubric.questions)
            .await
            .expect_err("401 must fail");
        let text = format!("{err:#}");
        assert!(text.contains("401"), "{text}");
        assert!(!text.contains("fixture-key"), "leaked: {text}");
        let _ = tx.send(());
    }

    // ----- output kind gate -----

    #[test]
    fn output_kind_covers_text_raster_and_media() {
        assert_eq!(output_kind("svg"), OutputKind::Text);
        assert_eq!(output_kind("HTML"), OutputKind::Text);
        assert_eq!(output_kind("md"), OutputKind::Text);
        assert_eq!(output_kind("tsx"), OutputKind::Text);
        assert_eq!(output_kind("png"), OutputKind::Image);
        assert_eq!(output_kind("jpg"), OutputKind::Image);
        assert_eq!(output_kind("mp3"), OutputKind::Audio);
        assert_eq!(output_kind("mp4"), OutputKind::Video);
        assert_eq!(output_kind("pdf"), OutputKind::Unknown);
    }
}
