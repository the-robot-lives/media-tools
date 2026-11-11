use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use base64::Engine;
use serde_json::json;

use crate::attachments::resolve_mime_type;
use crate::schema::{EvalCriterion, EvalSection};
use crate::structural::{is_structural_candidate, score_structural};
use crate::telemetry as tel;
use crate::telemetry::progress;

// ---------------------------------------------------------------------------
// Groq last-resort variant selection (kept as fallback)
// ---------------------------------------------------------------------------

const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const GROQ_API_BASE: &str = "https://api.groq.com/openai/v1";
const DEFAULT_GROQ_MODEL: &str = "openai/gpt-oss-120b";

// <REMOVED UUID HERE> evaluate_candidates :: auto-generated pointer for public function evaluate_candidates
pub async fn evaluate_candidates(
    candidate_paths: &[&Path],
    prompt_text: &str,
    eval_criteria: Option<&HashMap<String, EvalCriterion>>,
    api_key: &str,
    model: Option<&str>,
    verbose: bool,
) -> usize {
    if candidate_paths.len() <= 1 {
        return 0;
    }

    if api_key.is_empty() {
        tel::warn_msg("No GROQ_API_KEY \u{2014} selecting first candidate");
        return 0;
    }

    let model = model.unwrap_or(DEFAULT_GROQ_MODEL);
    let mut content_parts: Vec<serde_json::Value> = Vec::new();

    let mut criteria_text = String::new();
    if let Some(criteria) = eval_criteria {
        criteria_text.push_str("\nEvaluation criteria:\n");
        for (name, spec) in criteria {
            let desc = spec.description.as_deref().unwrap_or(name);
            let weight = spec.weight.map(|w| format!("{}", w)).unwrap_or_default();
            criteria_text.push_str(&format!("  - {} (weight {}): {}\n", name, weight, desc));
        }
    }

    content_parts.push(json!({
        "type": "text",
        "text": format!(
            "You are evaluating {} AI-generated images.\n\
             The generation prompt was:\n\"{}\"\n\
             {}\n\n\
             Each image is labelled [Image 1], [Image 2], etc.\n\
             Pick the single best image. Reply with ONLY the number (e.g. 2).",
            candidate_paths.len(),
            prompt_text,
            criteria_text
        )
    }));

    for (i, cpath) in candidate_paths.iter().enumerate() {
        let mime = resolve_mime_type(cpath, None);
        let data = match std::fs::read(cpath) {
            Ok(d) => d,
            Err(e) => {
                tel::warn_msg(&format!("Cannot read candidate {}: {}", cpath.display(), e));
                continue;
            }
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

        content_parts.push(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:{};base64,{}", mime, b64) }
        }));
        content_parts.push(json!({
            "type": "text",
            "text": format!("[Image {}]", i + 1)
        }));
    }

    let body = json!({
        "model": model,
        "messages": [{ "role": "user", "content": content_parts }],
        "temperature": 0,
        "max_tokens": 16,
    });

    if verbose {
        tel::verbose(&format!(
            "Groq vision eval: {} candidates via {}",
            candidate_paths.len(),
            model
        ));
    }

    let client = reqwest::Client::new();
    let result = client
        .post(GROQ_API_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .timeout(Duration::from_secs(120))
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(val) = resp.json::<serde_json::Value>().await {
                if let Some(text) = val["choices"][0]["message"]["content"].as_str() {
                    let text = strip_think(text);
                    let text = text.trim();
                    if verbose {
                        tel::verbose(&format!("Groq response: {:?}", text));
                    }
                    if let Ok(num) = text
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<usize>()
                    {
                        let idx = num.saturating_sub(1);
                        if idx < candidate_paths.len() {
                            return idx;
                        }
                    }
                    tel::warn_msg(&format!(
                        "Could not parse Groq selection ({:?}) \u{2014} defaulting to first",
                        text
                    ));
                }
            }
            0
        }
        Ok(resp) => {
            tel::warn_msg(&format!(
                "Groq vision eval failed (HTTP {}) \u{2014} selecting first candidate",
                resp.status()
            ));
            0
        }
        Err(e) => {
            tel::warn_msg(&format!(
                "Groq vision eval failed: {} \u{2014} selecting first candidate",
                e
            ));
            0
        }
    }
}

// ---------------------------------------------------------------------------
// EvalScore
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EvalScore {
    pub weighted: f64,
    pub per_criterion: HashMap<String, f64>,
    pub reject_hits: Vec<String>,
    pub notes: String,
    /// Present when the decision judge produced this score — carries judge
    /// identity, request id and per-criterion probability distributions for
    /// the metadata sidecar.
    pub judge: Option<crate::judge::JudgeRunDetail>,
}

impl EvalScore {
    /// True if this score constitutes a pass against the eval section's thresholds.
    // <REMOVED UUID HERE> passes :: True if this score constitutes a pass against the eval section's thresholds.
    pub fn passes(&self, eval: &EvalSection) -> bool {
        let threshold = eval.effective_pass_threshold();

        // Global weighted score
        if self.weighted < threshold {
            return false;
        }

        // All required_pass criteria must individually reach threshold
        for key in &eval.required_pass {
            let score = self.per_criterion.get(key).copied().unwrap_or(0.0);
            if score < threshold {
                return false;
            }
        }

        // No reject_if hits allowed
        if !self.reject_hits.is_empty() {
            return false;
        }

        true
    }
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

pub struct Evaluator {
    /// Legacy text/vision LLM endpoint (explicit `MEDIA_EVAL_BASE_URL` or the
    /// Groq fallback profile). Empty when the inference profile drives grading.
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    /// Alibaba `qwen3.8-flash` inference profile — the default grading path
    /// (Phase 2). Its vision chain doubles as the observer.
    pub inference: Option<crate::inference::InferenceConfig>,
    /// Vision observer over the inference profile (observe -> decide).
    pub observer: Option<crate::inference::Observer>,
    /// Decision judge resolved once at startup; `None` = no backend resolved.
    pub judge: Option<crate::judge::JudgeConfig>,
}

impl Evaluator {
    /// Resolution order (Phase 2): explicit `MEDIA_EVAL_BASE_URL` /
    /// `--eval-url` endpoint first (an explicitly pointed-at server always
    /// wins), then the qwen3.8-flash inference profile, then the Groq
    /// fallback. The old LM Studio probe list is gone — pointing at an LM
    /// Studio endpoint is what `MEDIA_EVAL_BASE_URL` is for. The decision
    /// judge config resolves independently (either grader can ride it).
    // <REMOVED UUID HERE> resolve :: Resolution order (Phase 2): explicit endpoint first.
    pub async fn resolve(
        cli_url: Option<&str>,
        cli_model: Option<&str>,
        verbose: bool,
    ) -> Option<Evaluator> {
        let override_url = cli_url
            .map(|s| s.to_string())
            .or_else(|| std::env::var("MEDIA_EVAL_BASE_URL").ok().filter(|v| !v.is_empty()));
        let model = cli_model
            .map(|m| m.to_string())
            .or_else(|| std::env::var("MEDIA_EVAL_MODEL").ok().filter(|m| !m.is_empty()));

        // Decision judge + inference profile resolve first; both are optional.
        let loaded = crate::provider_config::loaded();
        let judge = crate::judge::JudgeConfig::resolve(None, None, loaded.map(|l| &l.judge))
            .map_err(|e| {
                if verbose {
                    tel::verbose(&format!("Decision judge unavailable: {e:#}"));
                }
                e
            })
            .ok();
        let inference = crate::inference::InferenceConfig::resolve(loaded.map(|l| &l.inference))
            .map_err(|e| {
                if verbose {
                    tel::verbose(&format!("Inference profile unavailable: {e:#}"));
                }
                e
            })
            .ok();

        // 1. Explicit endpoint (legacy text/vision LLM grader)
        if let Some(url) = override_url {
            let api_key = std::env::var("MEDIA_EVAL_API_KEY").unwrap_or_else(|_| "lm-studio".into());
            let model = model.unwrap_or_else(|| "qwen3.6".to_string());
            if verbose {
                tel::verbose(&format!(
                    "Eval via explicit endpoint: {} (model: {}, judge: {})",
                    url,
                    model,
                    if judge.is_some() { "decision" } else { "llm" }
                ));
            }
            return Some(Evaluator {
                base_url: url,
                model,
                api_key,
                inference: inference.clone(),
                observer: inference.map(crate::inference::Observer::new),
                judge,
            });
        }

        // 2. qwen3.8-flash inference profile (default)
        if let Some(inference_cfg) = inference {
            if verbose {
                tel::verbose(&format!(
                    "Eval via inference profile: {} @ {} (judge: {})",
                    inference_cfg.model,
                    inference_cfg.base,
                    if judge.is_some() { "decision" } else { "llm" }
                ));
            }
            return Some(Evaluator {
                base_url: String::new(),
                model: model.unwrap_or_else(|| inference_cfg.model.clone()),
                api_key: String::new(),
                inference: Some(inference_cfg.clone()),
                observer: Some(crate::inference::Observer::new(inference_cfg)),
                judge,
            });
        }

        // 3. Groq fallback profile (explicit GROQ_API_KEY)
        if let Some(key) = std::env::var("GROQ_API_KEY").ok().filter(|k| !k.is_empty()) {
            let model = model.unwrap_or_else(|| DEFAULT_GROQ_MODEL.to_string());
            if verbose {
                tel::verbose(&format!(
                    "Eval via Groq fallback (model: {}, judge: {})",
                    model,
                    if judge.is_some() { "decision" } else { "llm" }
                ));
            }
            return Some(Evaluator {
                base_url: GROQ_API_BASE.to_string(),
                model,
                api_key: key,
                inference: None,
                observer: None,
                judge,
            });
        }

        if verbose {
            tel::verbose("No eval endpoint or inference profile — grading disabled");
        }
        None
    }

    /// Score a single output artifact against the eval criteria.
    ///
    /// Mode (`eval.mode`):
    /// - `structural` — ffprobe/ffmpeg heuristics only (audio/video)
    /// - `hybrid` — structural + LLM when both available
    /// - `llm` (default) — vision/text LLM; for audio falls back to structural
    ///
    /// When `eval.visual` is true, HTML may be screenshot'd via Puppeteer for vision.
    /// Returns None only when no scorer can produce a result.
    // <REMOVED UUID HERE> score_output :: Score a single output artifact against the eval criteria.
    pub async fn score_output(
        &self,
        path: &Path,
        prompt_text: &str,
        eval: &EvalSection,
        verbose: bool,
    ) -> Option<EvalScore> {
        self.score_output_with_duration(path, prompt_text, eval, None, verbose)
            .await
    }

    /// Like [`score_output`] but passes expected duration for structural checks.
    ///
    /// Emits `eval.started` / `eval.completed` on the telemetry progress channel around the
    /// scoring itself, which lives in [`Self::score_output_inner`].
    // <REMOVED UUID HERE> score_output_with_duration :: Like [`score_output`] but passes expected duration for structural checks.
    pub async fn score_output_with_duration(
        &self,
        path: &Path,
        prompt_text: &str,
        eval: &EvalSection,
        expected_duration: Option<f64>,
        verbose: bool,
    ) -> Option<EvalScore> {
        let output_display = path.display().to_string();
        progress::eval_started(
            &output_display,
            effective_mode_for_path(eval, path),
            &self.model,
        );
        let score = self
            .score_output_inner(path, prompt_text, eval, expected_duration, verbose)
            .await;
        let threshold = eval.effective_pass_threshold();
        match &score {
            Some(s) => progress::eval_completed(
                &output_display,
                s.weighted,
                threshold,
                s.passes(eval),
                s.reject_hits.len(),
            ),
            None => progress::eval_completed(
                &output_display,
                progress::NO_SCORE,
                threshold,
                false,
                0,
            ),
        }
        score
    }

    async fn score_output_inner(
        &self,
        path: &Path,
        prompt_text: &str,
        eval: &EvalSection,
        expected_duration: Option<f64>,
        verbose: bool,
    ) -> Option<EvalScore> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let mode = effective_mode_for_path(eval, path);

        // Structural-only path
        if mode == "structural" {
            return score_structural(path, eval, expected_duration, verbose).await;
        }

        // Hybrid: run structural first for AV; keep for fallback / blend
        let structural = if mode == "hybrid" || is_structural_candidate(path) {
            score_structural(path, eval, expected_duration, verbose).await
        } else {
            None
        };

        if mode == "structural" {
            return structural;
        }

        // The LLM arm: which grader and what the grader sees.
        let observation = match ext.as_str() {
            "png" | "jpg" | "jpeg" | "webp" | "gif" => Observation::Images(vec![
                crate::inference::load_image_attachment(path).ok()?,
            ]),
            "svg" | "mmd" | "puml" | "dot" | "html" | "htm" | "tsx" | "ts" | "js" | "md"
            | "json" | "txt" | "css" => {
                // Optional visual path for HTML/SVG when eval.visual: rasterize
                // and observe instead of reading the markup as text.
                let rasterized = match ext.as_str() {
                    "html" | "htm" if eval.wants_visual() => {
                        try_html_screenshot_attachment(path, verbose).await
                    }
                    "svg" if eval.wants_visual() => {
                        try_svg_raster_attachment(path, verbose).await
                    }
                    _ => None,
                };
                match rasterized {
                    Some(attachment) => Observation::Images(vec![attachment]),
                    None => Observation::Text(text_artifact(path)?),
                }
            }
            "mp4" | "webm" | "mov" => match extract_frames(path, verbose).await {
                Some(frames) => Observation::Frames(frames),
                None => {
                    if verbose {
                        tel::verbose(&format!(
                            "Video {} frames failed — using structural if available",
                            path.display()
                        ));
                    }
                    return structural;
                }
            },
            "mp3" | "wav" | "ogg" | "flac" | "m4a" => {
                // No audio-capable grader wired — structural only
                if verbose {
                    tel::verbose(&format!(
                        "Audio {} — using structural scoring (no audio grader)",
                        path.display()
                    ));
                }
                return structural;
            }
            _ => {
                if verbose {
                    tel::verbose(&format!(
                        "Unknown extension '{}' for {} — un-scorable",
                        ext,
                        path.display()
                    ));
                }
                return structural;
            }
        };

        // Grader selection: `decision` (System One judge; observe -> decide for
        // raster/video) when a backend resolved, `llm` (legacy text/vision
        // grader) otherwise or when forced.
        let grader = match eval.effective_judge_request() {
            Some("decision") => Grader::Decision,
            Some("llm") => Grader::LegacyLlm,
            Some(other) => {
                if verbose {
                    tel::verbose(&format!(
                        "Unknown eval judge '{other}' — using the legacy LLM grader"
                    ));
                }
                Grader::LegacyLlm
            }
            None => {
                if self.judge.is_some() {
                    Grader::Decision
                } else {
                    Grader::LegacyLlm
                }
            }
        };

        let llm = match grader {
            Grader::Decision => {
                self.score_decision(&observation, path, prompt_text, ext.as_str(), eval, verbose)
                    .await
            }
            Grader::LegacyLlm => {
                self.score_legacy_llm(&observation, prompt_text, eval, verbose).await
            }
        };

        match (mode, llm, structural) {
            (_, Some(l), Some(s)) if mode == "hybrid" => Some(blend_scores(&l, &s)),
            (_, Some(l), _) => Some(l),
            (_, None, Some(s)) => Some(s),
            _ => None,
        }
    }

    /// System One decision judge. Raster images and video frames are observed
    /// first (factual per-criterion description), text outputs go directly.
    async fn score_decision(
        &self,
        observation: &Observation,
        path: &Path,
        prompt_text: &str,
        ext: &str,
        eval: &EvalSection,
        verbose: bool,
    ) -> Option<EvalScore> {
        let judge_cfg = match &self.judge {
            Some(cfg) => cfg,
            None => {
                if verbose {
                    tel::verbose("eval judge=decision but no judge backend resolved — falling back to the LLM grader");
                }
                return self
                    .score_legacy_llm(observation, prompt_text, eval, verbose)
                    .await;
            }
        };
        let kind = match observation {
            Observation::Text(_) => crate::judge::OutputKind::Text,
            Observation::Images(_) => crate::judge::OutputKind::Image,
            Observation::Frames(_) => crate::judge::OutputKind::Video,
        };
        let observer = self.observer.as_ref();
        let outcome = crate::judge::judge_output(
            judge_cfg,
            observer,
            prompt_text,
            ext,
            ext,
            kind,
            path,
            eval,
            verbose,
        )
        .await;
        match outcome {
            Ok(outcome) => {
                if verbose {
                    tel::verbose(&format!(
                        "Decision judge: weighted={:.3} backend={}:{} request_id={}{}",
                        outcome.score.weighted,
                        outcome.detail.backend,
                        outcome.detail.model,
                        outcome
                            .detail
                            .request_id
                            .as_deref()
                            .unwrap_or("none"),
                        outcome
                            .detail
                            .observer_model
                            .as_deref()
                            .map(|m| format!(" observer={m}"))
                            .unwrap_or_default(),
                    ));
                }
                let mut score = outcome.score;
                score.judge = Some(outcome.detail);
                Some(score)
            }
            Err(e) => {
                tel::warn_msg(&format!("Decision judge failed: {e:#} — falling back"));
                None
            }
        }
    }

    /// Legacy text/vision grader: one chat completion that replies with the
    /// scored JSON payload. Rides the inference profile when present, else the
    /// explicit `base_url` endpoint.
    async fn score_legacy_llm(
        &self,
        observation: &Observation,
        prompt_text: &str,
        eval: &EvalSection,
        verbose: bool,
    ) -> Option<EvalScore> {
        let mut content_parts = content_parts_for(observation);
        // Build criteria description
        let mut criteria_list = String::new();
        let criterion_names: Vec<&str> = eval.criteria.keys().map(|s| s.as_str()).collect();
        for (name, spec) in &eval.criteria {
            let desc = spec.description.as_deref().unwrap_or(name);
            let weight = spec.weight.unwrap_or(1.0);
            let signals = if spec.fail_signals.is_empty() {
                String::new()
            } else {
                format!(" [fail signals: {}]", spec.fail_signals.join(", "))
            };
            criteria_list.push_str(&format!(
                "  - {}: {} (weight {:.1}){}\n",
                name, desc, weight, signals
            ));
        }

        let reject_list = if eval.reject_if.is_empty() {
            String::new()
        } else {
            format!(
                "\nReject if any of these are visible:\n{}\n",
                eval.reject_if
                    .iter()
                    .map(|r| format!("  - {}", r))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        // Prepend instruction text
        let instruction = json!({
            "type": "text",
            "text": format!(
                "Grade this generated artifact against the following criteria.\n\
                 Generation prompt: \"{}\"\n\n\
                 Criteria (score each 0-10):\n\
                 {}\
                 {}\n\
                 Reply ONLY with valid JSON (no markdown, no fences):\n\
                 {{\"scores\": {{{}}}, \"reject_hits\": [\"...\"], \"notes\": \"...\"}}",
                prompt_text,
                criteria_list,
                reject_list,
                criterion_names.iter().map(|n| format!("\"{}\": 0-10", n)).collect::<Vec<_>>().join(", ")
            )
        });
        content_parts.insert(0, instruction);

        let raw = if let Some(inference) = &self.inference {
            // Inference profile path: same wire shape, credentials from the
            // secrets chain.
            let (text, attachments) = crate::inference::split_chat_content(&json!(content_parts));
            let options = HashMap::from([
                ("max_tokens".to_string(), serde_yaml::Value::Number(4096.into())),
                ("temperature".to_string(), serde_yaml::Value::Number(0.into())),
            ]);
            if verbose {
                tel::verbose(&format!(
                    "Eval via inference profile {} (model {})",
                    inference.base, inference.model
                ));
            }
            inference
                .complete(
                    "You are a strict evaluation judge. You reply with the exact JSON schema requested.",
                    &text,
                    &options,
                    &attachments,
                )
                .await
                .ok()?
        } else {
            if self.base_url.is_empty() {
                if verbose {
                    tel::verbose("No legacy eval endpoint configured");
                }
                return None;
            }
            self.post_legacy_chat(&content_parts, verbose).await?
        };

        if verbose {
            tel::verbose(&format!(
                "Eval raw response: {}",
                crate::text::truncate(&raw, 400)
            ));
        }

        // Strip reasoning output and markdown code fences defensively
        let cleaned = strip_fences(&strip_think(&raw));

        match serde_json::from_str::<serde_json::Value>(&cleaned) {
            Ok(parsed) => build_score(&parsed, eval, verbose),
            Err(e) => {
                if verbose {
                    tel::verbose(&format!("Eval JSON parse failed: {}", e));
                }
                None
            }
        }
    }

    /// One legacy OpenAI-compatible chat POST with a single JSON-parse retry.
    async fn post_legacy_chat(
        &self,
        content_parts: &[serde_json::Value],
        verbose: bool,
    ) -> Option<String> {
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": content_parts }],
            "temperature": 0,
            "max_tokens": 4096,
        });

        let client = reqwest::Client::new();
        let url = format!("{}/chat/completions", self.base_url);

        if verbose {
            tel::verbose(&format!("Eval POST {}", url));
        }

        for attempt in 0..2 {
            let resp = match client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .timeout(eval_request_timeout())
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    if verbose {
                        tel::verbose(&format!("Eval request failed: {}", e));
                    }
                    return None;
                }
            };

            if !resp.status().is_success() {
                if verbose {
                    tel::verbose(&format!("Eval returned HTTP {}", resp.status()));
                }
                return None;
            }

            let val: serde_json::Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => {
                    if verbose {
                        tel::verbose(&format!("Eval response JSON error: {}", e));
                    }
                    return None;
                }
            };

            let raw = val["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string();

            if raw.is_empty() && attempt == 0 {
                if verbose {
                    tel::verbose("Eval response empty (attempt 1) — retrying");
                }
                continue;
            }
            return Some(raw);
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Observation helpers
// ---------------------------------------------------------------------------

/// What the grader sees for one artifact: markup/text content, a raster
/// image, or labeled video frames.
enum Observation {
    Text(String),
    Images(Vec<crate::attachments::LoadedAttachment>),
    Frames(Vec<Frame>),
}

/// Which grader scores the LLM arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grader {
    /// System One decision judge (observe -> decide for raster/video).
    Decision,
    /// Legacy text/vision LLM chat grader.
    LegacyLlm,
}

#[derive(Debug, Clone)]
pub(crate) struct Frame {
    pub label: String,
    pub attachment: crate::attachments::LoadedAttachment,
}

fn content_parts_for(observation: &Observation) -> Vec<serde_json::Value> {
    match observation {
        Observation::Text(text) => vec![json!({
            "type": "text",
            "text": format!("[artifact content]\n{}", text)
        })],
        Observation::Images(attachments) => attachments
            .iter()
            .map(|a| {
                json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:{};base64,{}", a.mime_type, a.data_b64) }
                })
            })
            .collect(),
        Observation::Frames(frames) => {
            let mut parts = Vec::new();
            for frame in frames {
                parts.push(json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:{};base64,{}", frame.attachment.mime_type, frame.attachment.data_b64) }
                }));
                parts.push(json!({"type": "text", "text": frame.label.clone()}));
            }
            parts
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Default audio/video to hybrid so demos without mode still get structural scores.
fn effective_mode_for_path(eval: &EvalSection, path: &Path) -> &'static str {
    let declared = eval.effective_mode();
    if declared != "llm" {
        return match declared {
            "structural" => "structural",
            "hybrid" => "hybrid",
            _ => "llm",
        };
    }
    if is_structural_candidate(path) {
        return "hybrid";
    }
    "llm"
}

fn text_artifact(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    const MAX_BYTES: usize = 32 * 1024;
    Some(if raw.len() > MAX_BYTES {
        raw[..MAX_BYTES].to_string()
    } else {
        raw
    })
}

fn blend_scores(llm: &EvalScore, structural: &EvalScore) -> EvalScore {
    let weighted = 0.7 * llm.weighted + 0.3 * structural.weighted;
    let mut per = llm.per_criterion.clone();
    for (k, v) in &structural.per_criterion {
        let entry = per.entry(k.clone()).or_insert(0.0);
        *entry = 0.7 * *entry + 0.3 * v;
    }
    let mut reject = llm.reject_hits.clone();
    for r in &structural.reject_hits {
        if !reject.contains(r) {
            reject.push(r.clone());
        }
    }
    EvalScore {
        weighted,
        per_criterion: per,
        reject_hits: reject,
        notes: format!(
            "[hybrid] llm: {} | structural: {}",
            llm.notes, structural.notes
        ),
        judge: llm.judge.clone(),
    }
}

/// Screenshot HTML via Puppeteer when available; returns a raster attachment
/// for the observe path.
async fn try_html_screenshot_attachment(
    path: &Path,
    verbose: bool,
) -> Option<crate::attachments::LoadedAttachment> {
    use crate::renderers;
    use std::collections::HashMap;

    let renderer = renderers::get_renderer("puppeteer")?;
    if !renderer.is_available() {
        if verbose {
            tel::verbose("eval.visual: puppeteer not available — text fallback");
        }
        return None;
    }
    let tmp = std::env::temp_dir().join(format!(
        "media_eval_html_{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis()
    ));
    let params = HashMap::new();
    match renderer.render(path, &tmp, &params) {
        Ok(true) if tmp.is_file() => {
            let attachment = crate::inference::load_image_attachment(&tmp).ok();
            let _ = std::fs::remove_file(&tmp);
            if attachment.is_none() && verbose {
                tel::verbose("eval.visual: screenshot unreadable");
            }
            attachment
        }
        _ => {
            if verbose {
                tel::verbose("eval.visual: puppeteer screenshot failed");
            }
            let _ = std::fs::remove_file(&tmp);
            None
        }
    }
}

/// Best-effort SVG → PNG via `rsvg-convert` or ImageMagick `convert`.
async fn try_svg_raster_attachment(
    path: &Path,
    verbose: bool,
) -> Option<crate::attachments::LoadedAttachment> {
    let tmp = std::env::temp_dir().join(format!(
        "media_eval_svg_{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis()
    ));

    let rsvg = tokio::process::Command::new("rsvg-convert")
        .args(["-o", tmp.to_str()?, path.to_str()?])
        .output()
        .await;
    let ok = match rsvg {
        Ok(o) if o.status.success() && tmp.is_file() => true,
        _ => {
            let conv = tokio::process::Command::new("convert")
                .args([path.to_str()?, tmp.to_str()?])
                .output()
                .await;
            matches!(conv, Ok(o) if o.status.success() && tmp.is_file())
        }
    };
    if !ok {
        if verbose {
            tel::verbose("eval.visual: SVG rasterize failed (rsvg-convert/convert)");
        }
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    let attachment = crate::inference::load_image_attachment(&tmp).ok();
    let _ = std::fs::remove_file(&tmp);
    attachment
}

/// Scoring/selection chat-completion timeout: 300s default, overridable via
/// MEDIA_EVAL_TIMEOUT (seconds). The hosted Qwen 3.6 is a reasoning model and
/// can take minutes per completion.
fn eval_request_timeout() -> Duration {
    let secs = std::env::var("MEDIA_EVAL_TIMEOUT")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(300);
    Duration::from_secs(secs)
}


/// Strip a reasoning model's <think>...</think> block(s) from the response.
/// Handles an unterminated <think> (truncated thinking) by dropping everything
/// from the opening tag onward.
#[cfg(test)]
pub(crate) fn strip_think_for_test(s: &str) -> String {
    strip_think(s)
}

#[cfg(test)]
pub(crate) fn strip_fences_for_test(s: &str) -> String {
    strip_fences(s)
}

#[cfg(test)]
pub(crate) fn build_score_for_test(
    parsed: &serde_json::Value,
    eval: &EvalSection,
) -> Option<EvalScore> {
    build_score(parsed, eval, false)
}

fn strip_think(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    loop {
        match rest.find("<think>") {
            Some(start) => {
                out.push_str(&rest[..start]);
                match rest[start..].find("</think>") {
                    Some(end_rel) => {
                        rest = &rest[start + end_rel + "</think>".len()..];
                    }
                    None => break, // unterminated — drop the tail
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    out.trim().to_string()
}

fn strip_fences(s: &str) -> String {
    let s = s.trim();
    if s.starts_with("```") {
        let after_fence = if let Some(nl) = s.find('\n') {
            &s[nl + 1..]
        } else {
            s
        };
        let trimmed = after_fence.trim_end_matches("```").trim_end();
        return trimmed.to_string();
    }
    s.to_string()
}

/// Remap a scored JSON payload (`scores` 0-10 per criterion, `reject_hits`,
/// `notes`) onto the eval section's weights; shared by the legacy LLM grader
/// and the decision-judge bridge (`judge::score_result`).
pub(crate) fn build_score(parsed: &serde_json::Value, eval: &EvalSection, verbose: bool) -> Option<EvalScore> {
    let scores_map = parsed["scores"].as_object()?;

    let mut weighted_sum = 0.0f64;
    let mut weight_total = 0.0f64;
    let mut per_criterion: HashMap<String, f64> = HashMap::new();

    let threshold = eval.effective_pass_threshold();
    for (name, criterion) in &eval.criteria {
        let raw_score = scores_map.get(name).and_then(|v| v.as_f64()).unwrap_or(0.0);
        // Normalize 0-10 → 0-1
        let normalized = (raw_score / 10.0).clamp(0.0, 1.0);
        let weight = criterion.weight.unwrap_or(1.0);
        weighted_sum += weight * normalized;
        weight_total += weight;
        crate::telemetry::progress::eval_criterion(
            name,
            weight,
            normalized,
            threshold,
            normalized >= threshold,
        );
        per_criterion.insert(name.clone(), normalized);
    }

    let weighted = if weight_total > 0.0 {
        weighted_sum / weight_total
    } else {
        0.0
    };

    // Collect reject hits
    let reject_hits: Vec<String> = parsed["reject_hits"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let notes = parsed["notes"].as_str().unwrap_or("").to_string();

    if verbose {
        tel::verbose(&format!(
            "Eval score: weighted={:.3}, reject_hits={:?}, notes={}",
            weighted,
            reject_hits,
            crate::text::truncate(&notes, 120)
        ));
    }

    Some(EvalScore {
        weighted,
        per_criterion,
        reject_hits,
        notes,
        judge: None,
    })
}

/// Extract up to 4 evenly-spaced frames from a video file using ffmpeg.
/// Returns labeled raster attachments for the observe path, or None if
/// ffmpeg is unavailable or no frame could be written.
pub(crate) async fn extract_frames(path: &Path, verbose: bool) -> Option<Vec<Frame>> {
    // Check ffmpeg is on PATH
    let ffprobe_check = tokio::process::Command::new("ffprobe")
        .arg("-version")
        .output()
        .await;
    if ffprobe_check.is_err() {
        if verbose {
            tel::verbose("ffprobe not found — video eval skipped");
        }
        return None;
    }

    // Get video duration via ffprobe
    let duration_output = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            path.to_str()?,
        ])
        .output()
        .await
        .ok()?;

    let duration_secs: f64 = String::from_utf8_lossy(&duration_output.stdout)
        .trim()
        .parse()
        .unwrap_or(10.0);

    // Create temp dir for frames
    let temp_dir = std::env::temp_dir().join(format!(
        "media_eval_frames_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ));
    std::fs::create_dir_all(&temp_dir).ok()?;

    const NUM_FRAMES: usize = 4;
    let mut frames: Vec<Frame> = Vec::new();

    for i in 0..NUM_FRAMES {
        let t = if NUM_FRAMES <= 1 {
            duration_secs / 2.0
        } else {
            duration_secs * (i as f64 + 1.0) / (NUM_FRAMES as f64 + 1.0)
        };

        let frame_path = temp_dir.join(format!("frame_{:02}.png", i));

        let result = tokio::process::Command::new("ffmpeg")
            .args([
                "-ss",
                &format!("{:.2}", t),
                "-i",
                path.to_str()?,
                "-vframes",
                "1",
                "-q:v",
                "2",
                frame_path.to_str()?,
                "-y",
            ])
            .output()
            .await;

        if let Ok(out) = result {
            if out.status.success() {
                if let Ok(attachment) = crate::inference::load_image_attachment(&frame_path) {
                    frames.push(Frame {
                        label: format!("[Frame at {:.1}s]", t),
                        attachment,
                    });
                }
            } else if verbose {
                tel::verbose(&format!("ffmpeg frame extract failed at {:.1}s", t));
            }
        }
    }

    // Cleanup temp dir (best-effort) — attachments are already in memory.
    let _ = std::fs::remove_dir_all(&temp_dir);

    if frames.is_empty() {
        None
    } else {
        Some(frames)
    }
}

// ---------------------------------------------------------------------------
// Rubric defaults + best-of-N selection
// ---------------------------------------------------------------------------

/// Default rubric for prompts without an `eval:` block. Best-of-N and other
/// comparative scoring MUST use this instead of `EvalSection::default()` —
/// an empty criteria map makes every candidate score 0.0, which silently
/// pins the win on candidate 0 (the legacy bug this replaces).
// <REMOVED UUID HERE> default_eval_for_type :: Default rubric for prompts without an eval block.
pub fn default_eval_for_type() -> crate::schema::EvalSection {
    use crate::schema::{EvalCriterion, EvalSection};
    let mut criteria = HashMap::new();
    criteria.insert(
        "relevance".into(),
        EvalCriterion {
            weight: Some(3.0),
            scale: None,
            description: Some("Matches the generation brief".into()),
            fail_signals: vec![],
        },
    );
    criteria.insert(
        "quality".into(),
        EvalCriterion {
            weight: Some(2.0),
            scale: None,
            description: Some("Overall technical and aesthetic quality".into()),
            fail_signals: vec![],
        },
    );
    EvalSection {
        pass_threshold: Some(0.7),
        max_attempts: None,
        required_pass: vec!["relevance".into()],
        criteria,
        reject_if: vec![],
        mode: None,
        visual: None,
        judge: None,
    }
}

/// Score each candidate against `eval`; return the best candidate's index plus
/// each candidate's score (positionally aligned with `refs`; `None` when a
/// candidate was un-scorable). Shared by best-of-N selection so the rubric
/// resolution (prompt eval block vs [`default_eval_for_type`]) stays in one place.
pub(crate) async fn select_best_candidate(
    refs: &[&Path],
    prompt_text: &str,
    eval: &crate::schema::EvalSection,
    evaluator: &Evaluator,
    verbose: bool,
) -> (usize, Vec<Option<EvalScore>>) {
    let mut best_i = 0usize;
    let mut best_score = -1.0f64;
    let mut scores: Vec<Option<EvalScore>> = Vec::with_capacity(refs.len());
    for (i, p) in refs.iter().enumerate() {
        match evaluator.score_output(p, prompt_text, eval, verbose).await {
            Some(score) => {
                if verbose {
                    tel::verbose(&format!(
                        "Candidate {} scored {:.3} ({} criteria)",
                        i + 1,
                        score.weighted,
                        score.per_criterion.len()
                    ));
                }
                if score.weighted > best_score {
                    best_score = score.weighted;
                    best_i = i;
                }
                scores.push(Some(score));
            }
            None => scores.push(None),
        }
    }
    if best_score < 0.0 {
        tel::warn_msg("No candidate scored — defaulting to first");
    }
    (best_i, scores)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::EvalCriterion;
    use crate::structural;
    use std::sync::Arc;
    use serde_json::Value;


    /// Full mock answer for one criterion: exact 10-level probabilities (unit
    /// mass at `peak`), legend byte-identical to the real label builder, and a
    /// native wire score equal to the weighted index (strict fast path).
    fn mock_score_answer(description: &str, peak: usize, confidence: f64) -> Value {
        let mut probs = serde_json::Map::new();
        let mut legend = serde_json::Map::new();
        for i in 0..crate::judge::system_one::MAX_SCORE_LEVELS {
            probs.insert(i.to_string(), json!(if i == peak { 1.0 } else { 0.0 }));
            legend.insert(
                i.to_string(),
                json!(crate::judge::score_level_label(i, description, &[])),
            );
        }
        json!({
            "type": "score",
            "score": peak as f64,
            "probabilities": Value::Object(probs),
            "legend": Value::Object(legend),
            "confidence": confidence
        })
    }

    /// Mock System One response covering every question `sample_eval`'s rubric
    /// produces: one score question per criterion plus `reject:00` (noul).
    fn mock_judge_response(
        request_id: &str,
        criteria: &[(&str, &str, usize, f64)],
    ) -> Value {
        let mut answers = serde_json::Map::new();
        for (name, description, peak, confidence) in criteria {
            answers.insert(
                format!("criterion:{name}"),
                mock_score_answer(description, *peak, *confidence),
            );
        }
        answers.insert(
            "reject:00".to_string(),
            json!({"type": "noul", "noul": 0.05}),
        );
        json!({
            "model": "decision-model-preview",
            "request_id": request_id,
            "answers": Value::Object(answers),
        })
    }

    fn sample_eval(threshold: f64, required: &[&str]) -> EvalSection {
        let mut criteria = HashMap::new();
        criteria.insert(
            "relevance".into(),
            EvalCriterion {
                weight: Some(3.0),
                scale: None,
                description: Some("matches brief".into()),
                fail_signals: vec![],
            },
        );
        criteria.insert(
            "technical".into(),
            EvalCriterion {
                weight: Some(1.0),
                scale: None,
                description: Some("sharp".into()),
                fail_signals: vec![],
            },
        );
        EvalSection {
            pass_threshold: Some(threshold),
            max_attempts: None,
            required_pass: required.iter().map(|s| s.to_string()).collect(),
            criteria,
            reject_if: vec!["watermark".into()],
            mode: None,
            visual: None,
            judge: None,
        }
    }

    #[test]
    fn passes_when_weighted_and_required_ok() {
        let eval = sample_eval(0.7, &["relevance"]);
        let mut per = HashMap::new();
        per.insert("relevance".into(), 0.9);
        per.insert("technical".into(), 0.8);
        let score = EvalScore {
            weighted: 0.875,
            per_criterion: per,
            reject_hits: vec![],
            notes: String::new(),
            judge: None,
        };
        assert!(score.passes(&eval));
    }

    #[test]
    fn fails_when_required_criterion_low() {
        let eval = sample_eval(0.7, &["relevance"]);
        let mut per = HashMap::new();
        per.insert("relevance".into(), 0.4);
        per.insert("technical".into(), 1.0);
        let score = EvalScore {
            weighted: 0.85,
            per_criterion: per,
            reject_hits: vec![],
            notes: String::new(),
            judge: None,
        };
        assert!(!score.passes(&eval));
    }

    #[test]
    fn fails_on_reject_hits() {
        let eval = sample_eval(0.5, &[]);
        let score = EvalScore {
            weighted: 0.95,
            per_criterion: HashMap::new(),
            reject_hits: vec!["watermark".into()],
            notes: String::new(),
            judge: None,
        };
        assert!(!score.passes(&eval));
    }

    #[test]
    fn build_score_normalizes_0_10_to_0_1() {
        let eval = sample_eval(0.7, &[]);
        let parsed = json!({
            "scores": { "relevance": 8, "technical": 10 },
            "reject_hits": [],
            "notes": "ok"
        });
        let score = build_score_for_test(&parsed, &eval).expect("score");
        assert!((score.per_criterion["relevance"] - 0.8).abs() < 1e-9);
        assert!((score.per_criterion["technical"] - 1.0).abs() < 1e-9);
        // weighted = (3*0.8 + 1*1.0) / 4 = 0.85
        assert!((score.weighted - 0.85).abs() < 1e-9);
    }

    #[test]
    fn strip_think_and_fences() {
        let raw = "<think>secret</think>```json\n{\"a\":1}\n```";
        let cleaned = strip_fences_for_test(&strip_think_for_test(raw));
        assert!(cleaned.contains("\"a\":1"));
        assert!(!cleaned.contains("secret"));
        assert!(!cleaned.contains("```"));
    }

    /// In-process mock OpenAI-compatible eval endpoint + score_output on a text fixture.
    #[tokio::test]
    async fn score_output_via_mock_eval_server() {
        use axum::routing::{get, post};
        use axum::{Json, Router};
        use std::net::SocketAddr;
        use tokio::sync::oneshot;

        async fn models() -> Json<serde_json::Value> {
            Json(json!({
                "data": [{"id": "mock-eval-model"}]
            }))
        }

        async fn chat(
            Json(_body): Json<serde_json::Value>,
        ) -> Json<serde_json::Value> {
            Json(json!({
                "choices": [{
                    "message": {
                        "content": r#"{"scores":{"relevance":9,"technical":8},"reject_hits":[],"notes":"mock ok"}"#
                    }
                }]
            }))
        }

        let app = Router::new()
            .route("/v1/models", get(models))
            .route("/v1/chat/completions", post(chat));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .ok();
        });

        // Small settle
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let dir = std::env::temp_dir().join(format!(
            "media_mock_eval_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let svg = dir.join("fixture.svg");
        std::fs::write(
            &svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4"/></svg>"#,
        )
        .unwrap();

        let evaluator = Evaluator {
            base_url: format!("http://{}/v1", addr),
            model: "mock-eval-model".into(),
            api_key: "test".into(),
            inference: None,
            observer: None,
            judge: None,
        };

        let score = evaluator
            .score_output(&svg, "a simple circle icon", &sample_eval(0.7, &[]), true)
            .await
            .expect("mock score");

        assert!(
            (score.weighted - 0.875).abs() < 0.01,
            "weighted={}",
            score.weighted
        ); // (3*0.9 + 1*0.8)/4
        assert!(score.notes.contains("mock") || score.notes.contains("ok") || !score.notes.is_empty());

        let _ = tx.send(());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = addr; // silence
        let _: SocketAddr = addr;
    }

    #[tokio::test]
    async fn structural_mode_scores_demo_audio() {
        if !structural::ffprobe_available() {
            return;
        }
        let path = Path::new("demos/music/sample-lofi-beat.mp3");
        if !path.is_file() {
            return;
        }
        let mut eval = sample_eval(0.4, &[]);
        eval.mode = Some("structural".into());
        let evaluator = Evaluator {
            base_url: "http://127.0.0.1:1/v1".into(), // unused
            model: "x".into(),
            api_key: "x".into(),
            inference: None,
            observer: None,
            judge: None,
        };
        let score = evaluator
            .score_output_with_duration(path, "lofi beat", &eval, Some(60.0), false)
            .await
            .expect("structural audio score");
        assert!(score.weighted > 0.3, "score={}", score.weighted);
        assert!(score.notes.contains("structural") || score.notes.contains("duration"));
    }

    // ----- Phase 2: rubric defaults + decision judge -----

    fn env_lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| pairs.get(name).cloned()
    }

    fn dc_none() -> impl Fn(&str, &str) -> std::io::Result<crate::secrets::DcOutput> {
        |_s, _k| Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no dc"))
    }

    /// The legacy best-of-N bug: `EvalSection::default()` carries no criteria,
    /// so every candidate scored 0.0 and candidate 0 won regardless of
    /// quality. `default_eval_for_type` must provide a real rubric instead.
    #[test]
    fn default_eval_fixes_all_zero_best_of_n() {
        let payload = json!({
            "scores": { "relevance": 9, "quality": 8 },
            "reject_hits": [],
            "notes": "ok"
        });
        let buggy = build_score_for_test(&payload, &crate::schema::EvalSection::default())
            .expect("score");
        assert_eq!(buggy.weighted, 0.0, "empty criteria must collapse to 0");

        let fixed = build_score_for_test(&payload, &default_eval_for_type()).expect("score");
        assert!(
            (fixed.weighted - (3.0 * 0.9 + 2.0 * 0.8) / 5.0).abs() < 1e-9,
            "weighted={}",
            fixed.weighted
        );
        assert!(fixed.passes(&default_eval_for_type()));
    }

    /// Pin the fix at selection level: with per-candidate scores served by a
    /// mock grader, selection must follow the scored best — not candidate 0.
    #[tokio::test]
    async fn select_best_candidate_scores_against_given_eval() {
        use axum::routing::post;
        use axum::{Json, Router};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        // First call scores 6/10, second scores 9/10 — candidate 2 must win.
        let call = Arc::new(AtomicUsize::new(0));
        let call_for_handler = call.clone();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move |_body: Json<serde_json::Value>| {
                let n = call_for_handler.fetch_add(1, Ordering::SeqCst);
                let scores = if n == 0 { 6 } else { 9 };
                async move {
                    Json(json!({
                        "choices": [{
                            "message": {
                                "content": format!(
                                    "{{\"scores\":{{\"relevance\":{scores},\"quality\":{scores}}},\"reject_hits\":[],\"notes\":\"mock\"}}"
                                )
                            }
                        }]
                    }))
                }
            }),
        );
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

        let dir = std::env::temp_dir().join(format!("media_select_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.svg");
        let b = dir.join("b.svg");
        std::fs::write(&a, "<svg xmlns='x'/>").unwrap();
        std::fs::write(&b, "<svg xmlns='x'/>").unwrap();
        let refs = [a.as_path(), b.as_path()];

        let evaluator = Evaluator {
            base_url: format!("http://{addr}/v1"),
            model: "mock".into(),
            api_key: "fixture".into(),
            inference: None,
            observer: None,
            judge: None,
        };

        let (best, scores) =
            select_best_candidate(&refs, "brief", &default_eval_for_type(), &evaluator, false)
            .await;
        assert_eq!(best, 1, "candidate 2 scored higher and must win");
        assert_eq!(scores.len(), 2);
        let s0 = scores[0].as_ref().expect("candidate 1 scored");
        let s1 = scores[1].as_ref().expect("candidate 2 scored");
        assert!(
            s1.weighted > s0.weighted,
            "scores must reflect the mock responses (6/10 then 9/10): {} vs {}",
            s0.weighted,
            s1.weighted
        );

        // The legacy behavior for contrast: an empty eval block would have
        // pinned candidate 0 (all scores 0.0, first-wins tie-break).
        let _ = tx.send(());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// observe -> decide end to end over mocks: a mock vision endpoint
    /// observes the raster, a mock /systemone judges the observation text.
    #[tokio::test]
    async fn observe_then_decide_via_mocks() {
        use axum::routing::post;
        use axum::{Json, Router};

        let judge_response = mock_judge_response(
            "mock-obs-1",
            &[("relevance", "matches brief", 8, 0.9), ("technical", "sharp", 7, 0.85)],
        );

        // Judge mock: captures the judged state and asserts the observation
        // replaced raw pixels.
        let judged_state: Arc<std::sync::Mutex<Option<Value>>> =
            Arc::new(std::sync::Mutex::new(None));
        let sink = judged_state.clone();
        let judge_app = Router::new().route(
            "/v1/systemone",
            post(
                |axum::extract::State(sink): axum::extract::State<
                    Arc<std::sync::Mutex<Option<Value>>>,
                >,
                 Json(body): Json<Value>| async move {
                    *sink.lock().unwrap() = Some(body);
                    Json(judge_response.clone())
                },
            )
            .with_state(sink),
        );

        // Observer mock: a chat endpoint that "sees" the image and answers
        // with the observations JSON.
        let observed_request: Arc<std::sync::Mutex<Option<Value>>> =
            Arc::new(std::sync::Mutex::new(None));
        let sink2 = observed_request.clone();
        let observer_app = Router::new().route(
            "/v1/chat/completions",
            post(
                |axum::extract::State(sink): axum::extract::State<
                    Arc<std::sync::Mutex<Option<Value>>>,
                >,
                 Json(body): Json<Value>| async move {
                    *sink.lock().unwrap() = Some(body);
                    Json(json!({
                        "choices": [{
                            "message": {
                                "content": "{\"observations\": {\"relevance\": \"a red circle on white\", \"technical\": \"clean edges, no artifacts\"}}"
                            }
                        }]
                    }))
                },
            )
            .with_state(sink2),
        );

        let spawn = |app: Router| async move {
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
            (addr, tx)
        };
        let (judge_addr, judge_tx) = spawn(judge_app).await;
        let (obs_addr, obs_tx) = spawn(observer_app).await;

        // Evaluator wired to both mocks.
        let inference_cfg = {
            let mut cfg = crate::inference::InferenceConfig::resolve_with(
                None,
                &env_lookup(&[("QWEN_API_KEY", "fixture")]),
                &dc_none(),
            )
            .unwrap();
            cfg.base = format!("http://{obs_addr}/v1");
            cfg.model = "mock-observer".into();
            cfg.observer_models = vec!["mock-observer".into()];
            cfg
        };
        let judge_cfg = {
            let mut cfg = crate::judge::JudgeConfig::resolve_with(
                None,
                None,
                None,
                &env_lookup(&[("QWEN_API_KEY", "fixture")]),
                &dc_none(),
            )
            .unwrap();
            cfg.base = format!("http://{judge_addr}/v1");
            cfg
        };
        let evaluator = Evaluator {
            base_url: String::new(),
            model: "mock-observer".into(),
            api_key: String::new(),
            inference: Some(inference_cfg.clone()),
            observer: Some(crate::inference::Observer::new(inference_cfg)),
            judge: Some(judge_cfg),
        };

        // Tiny valid PNG (1x1 red pixel).
        let png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        use base64::Engine;
        let dir = std::env::temp_dir().join(format!("media_observe_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("out.png");
        std::fs::write(&png, base64::engine::general_purpose::STANDARD.decode(png_b64).unwrap())
            .unwrap();

        let mut eval = sample_eval(0.6, &[]);
        eval.judge = Some("decision".into());

        let score = evaluator
            .score_output(&png, "a red circle", &eval, false)
            .await
            .expect("observed+judged score");

        // The judge saw a textual observation, not pixels.
        let captured = judged_state.lock().unwrap().take().expect("judge called");
        assert_eq!(captured["model"], "decision-model-preview");
        let state = &captured["state"];
        assert!(
            state["observations"]
                .as_str()
                .unwrap_or_default()
                .contains("red circle on white"),
            "judge must receive the observer description: {state}"
        );
        assert!(state.get("output").is_none(), "raw content must not ride along");
        assert_eq!(state["observed_by"], "mock-observer");

        // The observer mock received image parts, not bare text: the user
        // message content must be a part array carrying the image_url part.
        let observed = observed_request.lock().unwrap().take().expect("observer called");
        let content = &observed["messages"].as_array().unwrap().last().unwrap()["content"];
        assert!(content.is_array(), "observer input carries image parts: {content}");
        assert!(
            content
                .as_array()
                .unwrap()
                .iter()
                .any(|part| part["type"] == "image_url"),
            "image part must be present: {content}"
        );

        // Score mapped from judge answers, detail attached for the sidecar.
        assert!(
            (score.weighted - (3.0 * (8.0 / 9.0) + 1.0 * (7.0 / 9.0)) / 4.0).abs() < 1e-9,
            "weighted={}",
            score.weighted
        );
        assert_eq!(
            score
                .judge
                .as_ref()
                .map(|d| d.observer_model.as_deref()),
            Some(Some("mock-observer"))
        );
        assert_eq!(
            score.judge.as_ref().and_then(|d| d.request_id.as_deref()),
            Some("mock-obs-1")
        );

        let _ = judge_tx.send(());
        let _ = obs_tx.send(());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Text outputs skip the observer and go to the judge directly.
    #[tokio::test]
    async fn text_output_judged_without_observer() {
        use axum::routing::post;
        use axum::{Json, Router};

        let judge_response = mock_judge_response(
            "mock-txt-1",
            &[("relevance", "matches brief", 9, 0.9), ("technical", "sharp", 8, 0.85)],
        );
        let judged_state: Arc<std::sync::Mutex<Option<Value>>> =
            Arc::new(std::sync::Mutex::new(None));
        let sink = judged_state.clone();
        let judge_app = Router::new()
            .route(
                "/v1/systemone",
                post(
                    |axum::extract::State(sink): axum::extract::State<
                        Arc<std::sync::Mutex<Option<Value>>>,
                    >,
                     Json(body): Json<Value>| async move {
                        *sink.lock().unwrap() = Some(body);
                        Json(judge_response.clone())
                    },
                )
                .with_state(sink),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, judge_app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let judge_cfg = {
            let mut cfg = crate::judge::JudgeConfig::resolve_with(
                None,
                None,
                None,
                &env_lookup(&[("QWEN_API_KEY", "fixture")]),
                &dc_none(),
            )
            .unwrap();
            cfg.base = format!("http://{addr}/v1");
            cfg
        };
        let evaluator = Evaluator {
            base_url: String::new(),
            model: String::new(),
            api_key: String::new(),
            inference: None,
            observer: None,
            judge: Some(judge_cfg),
        };

        let dir = std::env::temp_dir().join(format!("media_txtjudge_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let svg = dir.join("out.svg");
        std::fs::write(&svg, "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();

        let mut eval = sample_eval(0.6, &[]);
        eval.judge = Some("decision".into());
        let score = evaluator
            .score_output(&svg, "a circle", &eval, false)
            .await
            .expect("judged score");

        let captured = judged_state.lock().unwrap().take().expect("judge called");
        assert!(
            captured["state"]["output"]
                .as_str()
                .unwrap_or_default()
                .contains("<svg"),
            "text output must ride in state.output"
        );
        assert!((score.weighted - (3.0 * (9.0 / 9.0) + 1.0 * (8.0 / 9.0)) / 4.0).abs() < 1e-9);
        assert!(score.judge.as_ref().map(|d| d.observer_model.is_none()).unwrap_or(false));

        let _ = tx.send(());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
