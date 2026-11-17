//! TypeSafe System One wire adapter for the decision-judge backends.
//!
//! Rust port of `genai/lib/genai_providers/system_one.ex` and
//! `system_one_rounded.ex`: request encoding, response decoding, strict
//! distribution validation and the inferred Alibaba rounded-hundredths policy.
//! Score indices are validated before remapping onto the caller's numeric
//! rubric; noul probabilities use an explicit boolean threshold; no confidence
//! is invented. Values that cross this boundary are bounded shape/numeric
//! fields only — credentials and raw payloads never enter error strings.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Value};

use super::DecisionBackend;

/// Wire limit shared by both backends (TypeSafe contract).
pub const MAX_CHOICES: usize = 255;
/// Wire limit for ordered score levels.
pub const MAX_SCORE_LEVELS: usize = 10;

const EPSILON: f64 = 1.0e-10;
const TOLERANCE: f64 = 1.0e-6;
const HALF_STEP: f64 = 0.005;

// ---------------------------------------------------------------------------
// Question model (caller side)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreLevel {
    pub label: String,
    /// Caller's numeric rubric value for this level (any scale).
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuestionKind {
    Choice(Vec<ChoiceOption>),
    Score(Vec<ScoreLevel>),
    Noul,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub id: String,
    pub kind: QuestionKind,
    pub instructions: Option<String>,
}

impl Question {
    pub fn new(id: impl Into<String>, kind: QuestionKind) -> Self {
        Self {
            id: id.into(),
            kind,
            instructions: None,
        }
    }

    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// The documented fallback instruction, per wire type.
    pub fn effective_instructions(&self) -> String {
        if let Some(text) = &self.instructions {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        match self.kind {
            QuestionKind::Choice(_) => "Which option best describes the state?".to_string(),
            QuestionKind::Score(_) => "Which rubric level best describes the state?".to_string(),
            QuestionKind::Noul => "Is the statement true?".to_string(),
        }
    }

    fn wire_type(&self) -> &'static str {
        match self.kind {
            QuestionKind::Choice(_) => "choice",
            QuestionKind::Score(_) => "score",
            QuestionKind::Noul => "noul",
        }
    }

    fn levels(&self) -> usize {
        match &self.kind {
            QuestionKind::Choice(options) => options.len(),
            QuestionKind::Score(levels) => levels.len(),
            QuestionKind::Noul => 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    MissingModel,
    InvalidState,
    TooManyChoices { question: String },
    TooManyScoreLevels { question: String },
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::MissingModel => write!(f, "judge request has no model id"),
            EncodeError::InvalidState => write!(
                f,
                "judge state must be a string, object or array (text-only modality)"
            ),
            EncodeError::TooManyChoices { question } => write!(
                f,
                "judge question '{question}' exceeds the {MAX_CHOICES}-choice wire limit"
            ),
            EncodeError::TooManyScoreLevels { question } => write!(
                f,
                "judge question '{question}' exceeds the {MAX_SCORE_LEVELS}-level score limit"
            ),
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct WireQuestion {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criteria: Option<Value>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct WireRequest {
    pub model: String,
    pub state: Value,
    pub questions: BTreeMap<String, WireQuestion>,
}

/// Encode typed questions with stable ids; choice criteria are an id->label
/// map, score criteria an ordered label array, noul carries no criteria.
pub fn encode(model: &str, state: &Value, questions: &[Question]) -> Result<WireRequest, EncodeError> {
    let model = model.trim();
    if model.is_empty() {
        return Err(EncodeError::MissingModel);
    }
    if !matches!(state, Value::String(_) | Value::Object(_) | Value::Array(_)) {
        return Err(EncodeError::InvalidState);
    }
    let mut wire_questions = BTreeMap::new();
    for question in questions {
        let entries = question.levels();
        match &question.kind {
            QuestionKind::Choice(_) if entries > MAX_CHOICES => {
                return Err(EncodeError::TooManyChoices {
                    question: question.id.clone(),
                });
            }
            QuestionKind::Score(_) if entries > MAX_SCORE_LEVELS => {
                return Err(EncodeError::TooManyScoreLevels {
                    question: question.id.clone(),
                });
            }
            _ => {}
        }
        let criteria = match &question.kind {
            QuestionKind::Choice(options) => Some(json!(options
                .iter()
                .map(|option| (option.id.clone(), option.label.clone()))
                .collect::<BTreeMap<String, String>>())),
            QuestionKind::Score(levels) => Some(json!(levels
                .iter()
                .map(|level| level.label.clone())
                .collect::<Vec<String>>())),
            QuestionKind::Noul => None,
        };
        wire_questions.insert(
            question.id.clone(),
            WireQuestion {
                kind: question.wire_type(),
                instructions: question.effective_instructions(),
                criteria,
            },
        );
    }
    Ok(WireRequest {
        model: model.to_string(),
        state: state.clone(),
        questions: wire_questions,
    })
}

// ---------------------------------------------------------------------------
// Policy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributionPolicy {
    Strict,
    AlibabaRoundedHundredths,
}

/// Decode-level policy selection: automatic (backend+model gated) or forced
/// (used by tests; the CLI never forces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyOverride {
    Auto,
    Strict,
    RoundedHundredths,
}

/// Which models report their distributions on the inferred hundredths wire
/// precision: Alibaba's `decision-model-preview` and the hosted `jev-*`
/// classifiers (live capture 2026-11-11 showed real Jev score scalars drift
/// from Σp·i, which the strict exact-equality check rejects). Both get the
/// rounded-hundredths policy by default; `PolicyOverride::Strict` restores
/// the exact check inside the gated path.
pub fn policy_for(backend: DecisionBackend, model: &str) -> DistributionPolicy {
    let rounded_model = match backend {
        DecisionBackend::Alibaba => model == "decision-model-preview",
        DecisionBackend::Jev => model.starts_with("jev-"),
    };
    if rounded_model {
        DistributionPolicy::AlibabaRoundedHundredths
    } else {
        DistributionPolicy::Strict
    }
}

fn resolve_policy(
    backend: DecisionBackend,
    response_model: &str,
    request_model: &str,
    override_policy: PolicyOverride,
) -> DistributionPolicy {
    // The gate needs BOTH sides to be the rounded-family model: the request
    // model we sent and the response model the service reports (Jev resolves
    // its aliases, so "jev-latest" answers as e.g. "jev-1.14.0" — prefix
    // matching tolerates that alias resolution).
    let auto_condition = match backend {
        DecisionBackend::Alibaba => {
            response_model == "decision-model-preview"
                && request_model == "decision-model-preview"
        }
        DecisionBackend::Jev => {
            response_model.starts_with("jev-") && request_model.starts_with("jev-")
        }
    };
    if !auto_condition {
        return DistributionPolicy::Strict;
    }
    match override_policy {
        PolicyOverride::Auto => policy_for(backend, response_model),
        PolicyOverride::Strict => DistributionPolicy::Strict,
        PolicyOverride::RoundedHundredths => DistributionPolicy::AlibabaRoundedHundredths,
    }
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AnswerKind {
    Choice {
        index: usize,
        choice_id: String,
    },
    Score {
        /// Σ p·level.score on the caller's rubric.
        expected_score: f64,
    },
    Noul {
        value: bool,
        positive_probability: f64,
    },
}

/// Provenance emitted when the rounded policy renormalized the distribution.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundedProvenance {
    pub wire_distribution_sum: f64,
    pub wire_weighted_index: f64,
    pub canonical_weighted_index: f64,
    pub attainable_index_min: f64,
    pub attainable_index_max: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub question_id: String,
    pub kind: AnswerKind,
    /// Ordered probabilities aligned with the question's options/levels.
    pub distribution: Vec<f64>,
    pub selected_probability: Option<f64>,
    pub provider_confidence: Option<f64>,
    pub wire_expected_index: Option<f64>,
    pub policy: DistributionPolicy,
    pub rounded: Option<RoundedProvenance>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeResult {
    pub model: String,
    pub request_id: Option<String>,
    pub latency_ms: Option<f64>,
    pub usage: BTreeMap<String, u64>,
    pub answers: Vec<Answer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Answers missing, not an object, id-set mismatch, or unusable model name.
    InvalidResponse,
    /// Probabilities malformed: wrong keys, out-of-range values, bad mass, or
    /// (score type) a wire score contradicting the distribution.
    InvalidDistribution { question: String },
    /// Rounded policy rejected the distribution or the native score.
    InvalidRounded { question: String },
    /// Wrong wire type, unknown choice id, failed argmax, bad legend/score, or
    /// invalid noul probability.
    InvalidAnswer { question: String, reason: &'static str },
    /// Provider confidence present but not a probability.
    InvalidConfidence { question: String },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::InvalidResponse => {
                write!(f, "judge response answers are missing or mismatched")
            }
            DecodeError::InvalidDistribution { question } => {
                write!(f, "judge question '{question}' returned an invalid distribution")
            }
            DecodeError::InvalidRounded { question } => write!(
                f,
                "judge question '{question}' failed the rounded-hundredths policy"
            ),
            DecodeError::InvalidAnswer { question, reason } => {
                write!(f, "judge question '{question}' answer invalid ({reason})")
            }
            DecodeError::InvalidConfidence { question } => {
                write!(f, "judge question '{question}' confidence is not a probability")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

/// Decode and validate a System One response against the request's questions
/// (documented 0.5 boolean threshold).
pub fn decode(
    raw: &Value,
    request_model: &str,
    backend: DecisionBackend,
    questions: &[Question],
    override_policy: PolicyOverride,
) -> Result<JudgeResult, DecodeError> {
    decode_with_threshold(
        raw,
        request_model,
        backend,
        questions,
        override_policy,
        0.5,
    )
}

/// As [`decode`] with an explicit boolean threshold (noul is strictly greater).
pub fn decode_with_threshold(
    raw: &Value,
    request_model: &str,
    backend: DecisionBackend,
    questions: &[Question],
    override_policy: PolicyOverride,
    boolean_threshold: f64,
) -> Result<JudgeResult, DecodeError> {
    let Some(answers) = raw.get("answers").and_then(Value::as_object) else {
        return Err(DecodeError::InvalidResponse);
    };
    let model_name = raw
        .get("model")
        .and_then(Value::as_str)
        .filter(|m| !m.trim().is_empty())
        .unwrap_or(request_model)
        .to_string();
    if model_name.trim().is_empty() {
        return Err(DecodeError::InvalidResponse);
    }
    let policy = resolve_policy(backend, &model_name, request_model, override_policy);

    // Answer keys must equal the question ids exactly (no missing, no extra).
    let mut expected: Vec<&str> = questions.iter().map(|q| q.id.as_str()).collect();
    expected.sort_unstable();
    let mut received: Vec<&str> = answers.keys().map(String::as_str).collect();
    received.sort_unstable();
    if expected != received {
        return Err(DecodeError::InvalidResponse);
    }
    if !(0.0..=1.0).contains(&boolean_threshold) {
        return Err(DecodeError::InvalidResponse);
    }

    let mut out = Vec::with_capacity(questions.len());
    for question in questions {
        let wire = answers
            .get(&question.id)
            .expect("id-set equality checked above");
        out.push(decode_answer(
            wire,
            question,
            policy,
            boolean_threshold,
        )?);
    }

    Ok(JudgeResult {
        model: model_name,
        request_id: raw
            .get("request_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        latency_ms: raw
            .get("latency_ms")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite()),
        usage: safe_usage(raw.get("usage")),
        answers: out,
    })
}

fn safe_usage(usage: Option<&Value>) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    if let Some(map) = usage.and_then(Value::as_object) {
        for key in ["input_tokens", "output_tokens", "total_tokens"] {
            if let Some(count) = map.get(key).and_then(Value::as_u64) {
                out.insert(key.to_string(), count);
            }
        }
    }
    out
}

fn decode_answer(
    wire: &Value,
    question: &Question,
    policy: DistributionPolicy,
    boolean_threshold: f64,
) -> Result<Answer, DecodeError> {
    match (&question.kind, wire.get("type").and_then(Value::as_str)) {
        (QuestionKind::Choice(options), Some("choice")) => {
            decode_choice(wire, question, options, policy)
        }
        (QuestionKind::Noul, Some("noul")) => decode_noul(wire, question, boolean_threshold),
        (QuestionKind::Score(levels), Some("score")) => {
            decode_score(wire, question, levels, policy)
        }
        _ => Err(DecodeError::InvalidAnswer {
            question: question.id.clone(),
            reason: "type",
        }),
    }
}

fn decode_choice(
    wire: &Value,
    question: &Question,
    options: &[ChoiceOption],
    policy: DistributionPolicy,
) -> Result<Answer, DecodeError> {
    let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
    let (probabilities, rounded) = distribution(
        wire.get("probabilities"),
        &ids,
        policy,
        ScoreCheck::Choice,
    )
    .map_err(|err| attach(err, &question.id))?;

    let chosen = wire.get("choice").and_then(Value::as_str);
    let index = ids
        .iter()
        .position(|id| Some(*id) == chosen)
        .ok_or(DecodeError::InvalidAnswer {
            question: question.id.clone(),
            reason: "unknown_choice",
        })?;
    let max = probabilities
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    if !(probabilities[index] + TOLERANCE >= max) {
        return Err(DecodeError::InvalidAnswer {
            question: question.id.clone(),
            reason: "choice_probability",
        });
    }
    let confidence = validate_confidence(wire.get("confidence")).map_err(|_| {
        DecodeError::InvalidConfidence {
            question: question.id.clone(),
        }
    })?;
    Ok(Answer {
        question_id: question.id.clone(),
        kind: AnswerKind::Choice {
            index,
            choice_id: options[index].id.clone(),
        },
        selected_probability: Some(probabilities[index]),
        distribution: probabilities,
        provider_confidence: confidence,
        wire_expected_index: None,
        policy,
        rounded,
    })
}

fn decode_noul(
    wire: &Value,
    question: &Question,
    boolean_threshold: f64,
) -> Result<Answer, DecodeError> {
    let positive = wire.get("noul").and_then(Value::as_f64);
    let Some(positive) = positive.filter(|p| p.is_finite() && (0.0..=1.0).contains(p)) else {
        return Err(DecodeError::InvalidAnswer {
            question: question.id.clone(),
            reason: "noul_probability",
        });
    };
    // Deterministic ties: strictly greater than the threshold means true.
    let value = positive > boolean_threshold;
    Ok(Answer {
        question_id: question.id.clone(),
        kind: AnswerKind::Noul {
            value,
            positive_probability: positive,
        },
        distribution: vec![1.0 - positive, positive],
        selected_probability: Some(if value { positive } else { 1.0 - positive }),
        provider_confidence: None,
        wire_expected_index: None,
        policy: DistributionPolicy::Strict,
        rounded: None,
    })
}

fn decode_score(
    wire: &Value,
    question: &Question,
    levels: &[ScoreLevel],
    policy: DistributionPolicy,
) -> Result<Answer, DecodeError> {
    let indices: Vec<String> = (0..levels.len()).map(|i| i.to_string()).collect();
    let index_refs: Vec<&str> = indices.iter().map(String::as_str).collect();
    let native_score = wire.get("score").and_then(Value::as_f64);
    let (probabilities, rounded) = distribution(
        wire.get("probabilities"),
        &index_refs,
        policy,
        ScoreCheck::Score(native_score),
    )
    .map_err(|err| attach(err, &question.id))?;

    // Legend must enumerate exactly "0".."n-1" with the caller's labels.
    let legend_ok = wire
        .get("legend")
        .and_then(Value::as_object)
        .map(|legend| {
            legend.len() == levels.len()
                && indices
                    .iter()
                    .zip(levels)
                    .all(|(index, level)| legend.get(index).and_then(Value::as_str) == Some(level.label.as_str()))
        })
        .unwrap_or(false);
    let score_is_number = native_score
        .map(|s| s.is_finite())
        .unwrap_or(false);
    if !legend_ok || !score_is_number {
        return Err(DecodeError::InvalidAnswer {
            question: question.id.clone(),
            reason: "score_or_legend",
        });
    }

    let native_mean = weighted_index(&probabilities);
    let score = native_score.unwrap_or(f64::NAN);
    if policy == DistributionPolicy::Strict
        && (native_mean - score).abs() > TOLERANCE * 1.0f64.max(native_mean.abs())
    {
        // Native index expectation must validate before rubric remapping.
        return Err(DecodeError::InvalidAnswer {
            question: question.id.clone(),
            reason: "score_or_legend",
        });
    }
    let confidence = validate_confidence(wire.get("confidence")).map_err(|_| {
        DecodeError::InvalidConfidence {
            question: question.id.clone(),
        }
    })?;

    let expected_score = probabilities
        .iter()
        .zip(levels)
        .map(|(p, level)| p * level.score)
        .sum::<f64>();
    Ok(Answer {
        question_id: question.id.clone(),
        kind: AnswerKind::Score { expected_score },
        distribution: probabilities,
        selected_probability: None,
        provider_confidence: confidence,
        wire_expected_index: Some(score),
        policy,
        rounded,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ScoreCheck {
    Choice,
    Score(Option<f64>),
}

fn attach(err: DistributionError, question: &str) -> DecodeError {
    match err {
        DistributionError::Invalid => DecodeError::InvalidDistribution {
            question: question.to_string(),
        },
        DistributionError::Rounded => DecodeError::InvalidRounded {
            question: question.to_string(),
        },
    }
}

enum DistributionError {
    Invalid,
    Rounded,
}

/// Validate the probability map against `keys` (exact set, probabilities in
/// range), then apply the policy: strict wants unit mass (and, for scores, is
/// later checked against the native score); rounded falls back to grid
/// normalization when the displayed mass is off by rounding only.
fn distribution(
    value: Option<&Value>,
    keys: &[&str],
    policy: DistributionPolicy,
    check: ScoreCheck,
) -> Result<(Vec<f64>, Option<RoundedProvenance>), DistributionError> {
    let Some(map) = value.and_then(Value::as_object) else {
        return Err(DistributionError::Invalid);
    };
    let mut expected: Vec<&str> = keys.to_vec();
    expected.sort_unstable();
    let mut received: Vec<&str> = map.keys().map(String::as_str).collect();
    received.sort_unstable();
    if expected != received {
        return Err(DistributionError::Invalid);
    }
    let mut values = Vec::with_capacity(keys.len());
    for key in keys {
        let p = map
            .get(*key)
            .and_then(Value::as_f64)
            .filter(|p| p.is_finite() && (0.0..=1.0).contains(p))
            .ok_or(DistributionError::Invalid)?;
        values.push(p);
    }
    let sum = values.iter().sum::<f64>();
    match policy {
        DistributionPolicy::Strict => {
            if (sum - 1.0).abs() <= TOLERANCE {
                Ok((values, None))
            } else {
                Err(DistributionError::Invalid)
            }
        }
        DistributionPolicy::AlibabaRoundedHundredths => {
            let strict_ok = (sum - 1.0).abs() <= TOLERANCE
                && match check {
                    ScoreCheck::Choice => true,
                    ScoreCheck::Score(native) => strict_score(&values, native),
                };
            if strict_ok {
                return Ok((values, None));
            }
            match rounded::normalize(&values, check) {
                Ok((canonical, provenance)) => Ok((canonical, Some(provenance))),
                Err(()) => Err(DistributionError::Rounded),
            }
        }
    }
}

/// True when the wire score already equals the distribution's weighted index
/// within tolerance (rounded fast path — no normalization needed).
fn strict_score(values: &[f64], native_score: Option<f64>) -> bool {
    let Some(score) = native_score.filter(|s| s.is_finite()) else {
        return false;
    };
    let mean = weighted_index(values);
    (mean - score).abs() <= TOLERANCE * 1.0f64.max(mean.abs())
}

fn weighted_index(values: &[f64]) -> f64 {
    values
        .iter()
        .enumerate()
        .map(|(index, p)| p * index as f64)
        .sum()
}

fn validate_confidence(value: Option<&Value>) -> Result<Option<f64>, ()> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let p = v.as_f64().ok_or(())?;
            if p.is_finite() && (0.0..=1.0).contains(&p) {
                Ok(Some(p))
            } else {
                Err(())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Rounded hundredths policy (port of system_one_rounded.ex)
// ---------------------------------------------------------------------------

pub(crate) mod rounded {
    use super::{EPSILON, HALF_STEP, ScoreCheck};
    use super::RoundedProvenance;

    /// An inferred Alibaba wire policy, not recovery of a hidden distribution:
    /// values must sit on the hundredths grid, the missing mass must be within
    /// what display rounding could have eaten, and (for scores) the native
    /// score must be attainable from bins around the displayed values.
    pub fn normalize(
        values: &[f64],
        check: ScoreCheck,
    ) -> Result<(Vec<f64>, RoundedProvenance), ()> {
        let count = values.len();
        if !(2..=10).contains(&count) {
            return Err(());
        }
        if !values
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err(());
        }
        let sum = values.iter().sum::<f64>();
        if !(sum > 0.0) {
            return Err(());
        }
        if !values.iter().all(|v| on_grid(*v)) {
            return Err(());
        }
        // What display rounding could have eaten: each of `count` on-grid
        // values is within half a step of an unknown unit-sum pre-image, so
        // up to `count * half_step` of mass can be missing. The Elixir
        // reference caps this at 0.01, but real workspace traffic exceeds
        // that (captured decision-model-preview answers sum to 0.98 with 10
        // bins), and the bounds + attainable-pre-image checks below already
        // reject any vector no unit-sum pre-rounding could produce.
        let mass_tolerance = count as f64 * HALF_STEP + EPSILON;
        if (sum - 1.0).abs() > mass_tolerance {
            return Err(());
        }
        let bounds: Vec<(f64, f64)> = values
            .iter()
            .map(|v| (0.0f64.max(v - HALF_STEP), 1.0f64.min(v + HALF_STEP)))
            .collect();
        if !bounds.iter().all(|(lower, upper)| lower <= upper) {
            return Err(());
        }
        let lower_sum = bounds.iter().map(|(lower, _)| lower).sum::<f64>();
        let upper_sum = bounds.iter().map(|(_, upper)| upper).sum::<f64>();
        if lower_sum > 1.0 + EPSILON || upper_sum < 1.0 - EPSILON {
            return Err(());
        }
        let minimum = attainable_mean(&bounds, false);
        let maximum = attainable_mean(&bounds, true);
        if let ScoreCheck::Score(native) = check {
            if !score_valid(native, minimum, maximum, count) {
                return Err(());
            }
        }
        let canonical: Vec<f64> = values.iter().map(|v| v / sum).collect();
        let provenance = RoundedProvenance {
            wire_distribution_sum: sum,
            wire_weighted_index: weighted(values),
            canonical_weighted_index: weighted(&canonical),
            attainable_index_min: minimum,
            attainable_index_max: maximum,
        };
        Ok((canonical, provenance))
    }

    fn on_grid(value: f64) -> bool {
        (value * 100.0 - (value * 100.0).round()).abs() <= EPSILON
    }

    fn score_valid(native: Option<f64>, minimum: f64, maximum: f64, count: usize) -> bool {
        let Some(score) = native else {
            return false;
        };
        score.is_finite()
            && score == score
            && score >= 0.0
            && score <= (count - 1) as f64
            && on_grid(score)
            && score >= minimum - HALF_STEP - EPSILON
            && score <= maximum + HALF_STEP + EPSILON
    }

    /// Largest/smallest weighted index attainable by distributing the missing
    /// unit mass across the bins (ascending order minimizes, descending
    /// maximizes, because later indices weigh more).
    fn attainable_mean(bounds: &[(f64, f64)], descending: bool) -> f64 {
        let base: f64 = bounds.iter().map(|(lower, _)| lower).sum();
        let mut indexed: Vec<(usize, &(f64, f64))> = bounds.iter().enumerate().collect();
        if descending {
            indexed.reverse();
        }
        let mut mass = (1.0 - base).max(0.0);
        let mut mean = 0.0;
        for (index, (lower, upper)) in indexed {
            let extra = mass.min(upper - lower);
            mass = (mass - extra).max(0.0);
            mean += (lower + extra) * index as f64;
        }
        mean
    }

    fn weighted(values: &[f64]) -> f64 {
        values
            .iter()
            .enumerate()
            .map(|(index, p)| p * index as f64)
            .sum()
    }
}

// ---------------------------------------------------------------------------
// Endpoint building
// ---------------------------------------------------------------------------

/// Build `{base}/systemone` without duplicating an existing `/systemone` or
/// `/models` segment. Rejects non-HTTP(S) schemes, missing hosts, userinfo,
/// query strings and fragments so credentials can never leak into a URL.
pub fn build_endpoint(base: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(base).map_err(|_| "judge base URL is not a valid URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("judge base URL must use http or https".to_string());
    }
    if url.host_str().is_none() {
        return Err("judge base URL has no host".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("judge base URL must not embed credentials".to_string());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("judge base URL must not carry a query or fragment".to_string());
    }
    let trimmed = url.path().trim_end_matches('/').to_string();
    let trimmed = trimmed
        .strip_suffix("/systemone")
        .map(str::to_string)
        .or_else(|| trimmed.strip_suffix("/models").map(str::to_string))
        .unwrap_or(trimmed);
    let mut out = url;
    out.set_path(&format!("{trimmed}/systemone"));
    Ok(out.to_string())
}

// ---------------------------------------------------------------------------
// Tests (fixtures ported from genai classifier_providers_test.exs)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn fixture_questions() -> Vec<Question> {
        vec![
            Question::new(
                "route",
                QuestionKind::Choice(vec![
                    ChoiceOption {
                        id: "billing".into(),
                        label: "Payments".into(),
                    },
                    ChoiceOption {
                        id: "technical".into(),
                        label: "Software".into(),
                    },
                ]),
            )
            .with_instructions("Route the ticket"),
            Question::new("refund", QuestionKind::Noul)
                .with_instructions("Does this ask for a refund?"),
            Question::new(
                "severity",
                QuestionKind::Score(vec![
                    ScoreLevel {
                        label: "Low".into(),
                        score: -10.0,
                    },
                    ScoreLevel {
                        label: "Medium".into(),
                        score: 0.0,
                    },
                    ScoreLevel {
                        label: "High".into(),
                        score: 100.0,
                    },
                ]),
            )
            .with_instructions("Grade severity"),
        ]
    }

    fn fixture_response() -> Value {
        json!({
            "model": "jev-pinned",
            "request_id": "fixture-id",
            "usage": {"input_tokens": 23, "api_key": "fixture-secret"},
            "latency_ms": 2.5,
            "answers": {
                "route": {
                    "type": "choice",
                    "choice": "billing",
                    "probabilities": {"technical": 0.2, "billing": 0.8},
                    "confidence": 0.6
                },
                "refund": {"type": "noul", "noul": 0.9},
                "severity": {
                    "type": "score",
                    "score": 1.6,
                    "probabilities": {"2": 0.7, "0": 0.1, "1": 0.2},
                    "legend": {"0": "Low", "1": "Medium", "2": "High"},
                    "confidence": 0.7
                }
            }
        })
    }

    fn strict_decode(raw: &Value) -> Result<JudgeResult, DecodeError> {
        // jev-latest defaults to the rounded policy now; these fixtures pin
        // strict semantics, so force it explicitly.
        decode(raw, "jev-latest", DecisionBackend::Jev, &fixture_questions(), PolicyOverride::Strict)
    }

    fn rounded_decode(raw: &Value, override_policy: PolicyOverride) -> Result<JudgeResult, DecodeError> {
        let mut body = raw.clone();
        body["model"] = json!("decision-model-preview");
        decode(
            &body,
            "decision-model-preview",
            DecisionBackend::Alibaba,
            &fixture_questions(),
            override_policy,
        )
    }

    fn put_in(body: &Value, path: &[&str], value: Value) -> Value {
        let mut out = body.clone();
        let mut cursor = &mut out;
        for (i, key) in path.iter().enumerate() {
            if i == path.len() - 1 {
                cursor[key] = value.clone();
            } else {
                cursor = &mut cursor[*key];
            }
        }
        out
    }

    // ----- encoding -----

    #[test]
    fn encodes_question_ids_choice_map_and_ordinal_labels() {
        let wire = encode("jev-latest", &json!({"ticket": "refund request"}), &fixture_questions())
            .expect("encode");
        assert_eq!(wire.model, "jev-latest");
        let route = &wire.questions["route"];
        assert_eq!(route.kind, "choice");
        assert_eq!(
            route.criteria.as_ref().unwrap(),
            &json!({"billing": "Payments", "technical": "Software"})
        );
        assert_eq!(wire.questions["refund"].kind, "noul");
        assert!(wire.questions["refund"].criteria.is_none());
        let severity = &wire.questions["severity"];
        assert_eq!(severity.kind, "score");
        assert_eq!(
            severity.criteria.as_ref().unwrap(),
            &json!(["Low", "Medium", "High"])
        );
        assert_eq!(
            wire.questions["route"].instructions,
            "Route the ticket"
        );
    }

    #[test]
    fn falls_back_to_documented_default_instructions() {
        let questions = vec![
            Question::new("c", QuestionKind::Choice(vec![ChoiceOption { id: "a".into(), label: "A".into() }])),
            Question::new("n", QuestionKind::Noul),
            Question::new("s", QuestionKind::Score(vec![ScoreLevel { label: "L".into(), score: 0.0 }])),
        ];
        let wire = encode("m", &json!("state"), &questions).unwrap();
        assert_eq!(wire.questions["c"].instructions, "Which option best describes the state?");
        assert_eq!(wire.questions["n"].instructions, "Is the statement true?");
        assert_eq!(wire.questions["s"].instructions, "Which rubric level best describes the state?");
    }

    #[test]
    fn limits_score_levels_without_capping_choices() {
        // 255 choices pass the wire limit...
        let many: Vec<Question> = vec![Question::new(
            "many",
            QuestionKind::Choice((0..MAX_CHOICES).map(|i| ChoiceOption { id: i.to_string(), label: i.to_string() }).collect()),
        )];
        assert!(encode("m", &json!("state"), &many).is_ok());
        // ...256 do not
        let too_many: Vec<Question> = vec![Question::new(
            "many",
            QuestionKind::Choice((0..=MAX_CHOICES).map(|i| ChoiceOption { id: i.to_string(), label: i.to_string() }).collect()),
        )];
        assert_eq!(
            encode("m", &json!("state"), &too_many),
            Err(EncodeError::TooManyChoices { question: "many".into() })
        );
        // 11 score levels are rejected, 10 accepted
        let levels = |n: usize| {
            Question::new(
                "levels",
                QuestionKind::Score((0..n).map(|i| ScoreLevel { label: i.to_string(), score: i as f64 }).collect()),
            )
        };
        assert_eq!(
            encode("m", &json!("state"), &vec![levels(MAX_SCORE_LEVELS + 1)]),
            Err(EncodeError::TooManyScoreLevels { question: "levels".into() })
        );
        assert!(encode("m", &json!("state"), &vec![levels(MAX_SCORE_LEVELS)]).is_ok());
    }

    #[test]
    fn rejects_empty_model_and_scalar_state() {
        assert_eq!(
            encode("  ", &json!("state"), &fixture_questions()),
            Err(EncodeError::MissingModel)
        );
        assert_eq!(
            encode("m", &json!(7), &fixture_questions()),
            Err(EncodeError::InvalidState)
        );
    }

    // ----- strict decoding (Jev fixture) -----

    #[test]
    fn decodes_choice_noul_and_nonlinear_score() {
        let result = strict_decode(&fixture_response()).expect("decode");
        assert_eq!(result.model, "jev-pinned");
        let answers = &result.answers;
        assert_eq!(answers.len(), 3);
        assert_eq!(
            answers[0].kind,
            AnswerKind::Choice {
                index: 0,
                choice_id: "billing".into()
            }
        );
        assert_eq!(answers[0].distribution, vec![0.8, 0.2]);
        assert_eq!(answers[0].selected_probability, Some(0.8));
        assert_eq!(answers[0].provider_confidence, Some(0.6));
        assert_eq!(
            answers[1].kind,
            AnswerKind::Noul {
                value: true,
                positive_probability: 0.9
            }
        );
        assert!((answers[1].distribution[0] - 0.1).abs() < 1e-12);
        assert_eq!(
            answers[2].kind,
            AnswerKind::Score { expected_score: 69.0 }
        );
        assert_eq!(answers[2].wire_expected_index, Some(1.6));
        assert_eq!(answers[2].policy, DistributionPolicy::Strict);
    }

    #[test]
    fn keeps_safe_usage_and_request_metadata_only() {
        let mut body = fixture_response();
        body["api_key"] = json!("fixture-secret");
        let result = strict_decode(&body).expect("decode");
        assert_eq!(result.usage.get("input_tokens"), Some(&23));
        assert!(!result.usage.contains_key("api_key"));
        assert_eq!(result.request_id.as_deref(), Some("fixture-id"));
        assert_eq!(result.latency_ms, Some(2.5));
    }

    #[test]
    fn validates_native_index_before_rubric_remap() {
        let body = put_in(&fixture_response(), &["answers", "severity", "score"], json!(69.0));
        assert_eq!(
            strict_decode(&body),
            Err(DecodeError::InvalidAnswer {
                question: "severity".into(),
                reason: "score_or_legend"
            })
        );
    }

    #[test]
    fn rejects_bad_probabilities_argmax_confidence_and_legend() {
        let bodies = vec![
            put_in(
                &fixture_response(),
                &["answers", "route", "probabilities"],
                json!({"billing": 0.2, "technical": 0.2}),
            ),
            put_in(&fixture_response(), &["answers", "route", "choice"], json!("technical")),
            put_in(&fixture_response(), &["answers", "route", "confidence"], json!(2)),
            put_in(&fixture_response(), &["answers", "severity", "legend", "0"], json!("Wrong rubric")),
            put_in(&fixture_response(), &["answers", "route", "choice"], json!("unknown-id")),
            put_in(&fixture_response(), &["answers", "refund", "noul"], json!(1.5)),
        ];
        for body in bodies {
            assert!(
                strict_decode(&body).is_err(),
                "expected rejection for {body}"
            );
        }
    }

    #[test]
    fn rejects_missing_additional_or_mismatched_answers() {
        let mut empty = fixture_response();
        empty["answers"] = json!({});
        let mut extra = fixture_response();
        extra["answers"]["bogus"] = json!({"type": "choice", "choice": "billing", "probabilities": {}});
        let mut mismatch = fixture_response();
        mismatch["answers"]["refund"] = json!({"type": "choice", "choice": "billing", "probabilities": {"billing": 1.0}});
        for body in [empty, extra, mismatch] {
            assert!(strict_decode(&body).is_err());
        }
        // Non-map answers block
        let mut no_answers = fixture_response();
        no_answers["answers"] = json!([]);
        assert!(strict_decode(&no_answers).is_err());
    }

    #[test]
    fn boolean_threshold_is_strictly_greater_with_false_ties() {
        let body = put_in(&fixture_response(), &["answers", "refund", "noul"], json!(0.5));
        let result = strict_decode(&body).expect("decode");
        assert_eq!(
            result.answers[1].kind,
            AnswerKind::Noul {
                value: false,
                positive_probability: 0.5
            }
        );
        let result = decode_with_threshold(
            &fixture_response(),
            "jev-latest",
            DecisionBackend::Jev,
            &fixture_questions(),
            PolicyOverride::Auto,
            0.95,
        )
        .expect("decode");
        assert_eq!(
            result.answers[1].kind,
            AnswerKind::Noul {
                value: false,
                positive_probability: 0.9
            }
        );
        let result = decode_with_threshold(
            &fixture_response(),
            "jev-latest",
            DecisionBackend::Jev,
            &fixture_questions(),
            PolicyOverride::Auto,
            0.4,
        )
        .expect("decode");
        assert_eq!(
            result.answers[1].kind,
            AnswerKind::Noul {
                value: true,
                positive_probability: 0.9
            }
        );
    }

    // ----- endpoint building -----

    #[test]
    fn builds_supplied_endpoint_exactly_once() {
        for base in [
            "https://fixture.example/compatible-mode/v1",
            "https://fixture.example/compatible-mode/v1/",
            "https://fixture.example/compatible-mode/v1/systemone",
            "https://fixture.example/compatible-mode/v1/models",
        ] {
            assert_eq!(
                build_endpoint(base).expect("endpoint"),
                "https://fixture.example/compatible-mode/v1/systemone",
                "base was {base}"
            );
        }
        assert!(build_endpoint("https://user:password@fixture.example/v1").is_err());
        assert!(build_endpoint("https://fixture.example/v1?token=x").is_err());
        assert!(build_endpoint("ftp://fixture.example/v1").is_err());
        assert!(build_endpoint("not a url").is_err());
        assert!(build_endpoint("").is_err());
        assert_eq!(
            build_endpoint("https://fixture.example").expect("root base"),
            "https://fixture.example/systemone"
        );
    }

    // ----- rounded policy -----

    #[test]
    fn rounded_score_remaps_canonical_estimate_onto_nonlinear_rubric() {
        let body = put_in(
            &fixture_response(),
            &["answers", "severity", "probabilities"],
            json!({"0": 0.1, "1": 0.2, "2": 0.69}),
        );
        let result = rounded_decode(&body, PolicyOverride::Auto).expect("decode");
        let score = &result.answers[2];
        let canonical_sum = score.distribution.iter().sum::<f64>();
        assert!((canonical_sum - 1.0).abs() < 1e-12);
        let AnswerKind::Score { expected_score } = score.kind else {
            panic!("score answer");
        };
        assert!((expected_score - 68.0 / 0.99).abs() < 1e-12);
        assert_eq!(score.provider_confidence, Some(0.7));
        assert_eq!(score.wire_expected_index, Some(1.6));
        let provenance = score.rounded.as_ref().expect("rounded provenance");
        assert!((provenance.wire_distribution_sum - 0.99).abs() < 1e-12);
        assert!((provenance.wire_weighted_index - 1.58).abs() < 1e-12);
        assert!((provenance.canonical_weighted_index - 1.58 / 0.99).abs() < 1e-12);
        assert!(provenance.attainable_index_min - HALF_STEP - EPSILON <= 1.6);
        assert!(provenance.attainable_index_max + HALF_STEP + EPSILON >= 1.6);
    }

    #[test]
    fn rounded_native_score_checked_against_feasible_bins() {
        // Original fixture distribution [0.1, 0.2, 0.7]: attainable maximum is
        // exactly 1.61, so a native score of 1.61 is accepted...
        let body = put_in(&fixture_response(), &["answers", "severity", "score"], json!(1.61));
        assert!(rounded_decode(&body, PolicyOverride::Auto).is_ok());
        // ...1.8 is outside every attainable bin and rejected.
        assert_eq!(
            rounded_decode(&put_in(&fixture_response(), &["answers", "severity", "score"], json!(1.8)), PolicyOverride::Auto),
            Err(DecodeError::InvalidRounded { question: "severity".into() })
        );
        // Strict policy still rejects the displayed unit mass contradiction
        // (mean 1.6 vs wire score 1.61) before any rounded normalization.
        assert_eq!(
            rounded_decode(&body, PolicyOverride::Strict),
            Err(DecodeError::InvalidAnswer { question: "severity".into(), reason: "score_or_legend" })
        );
        // A 0.99-mass, off-grid-tolerant body keeps its score checked too:
        // 2.0 exceeds the [0.1, 0.2, 0.69] attainable band.
        let squashed = put_in(&fixture_response(), &["answers", "severity", "probabilities"], json!({"0": 0.1, "1": 0.2, "2": 0.69}));
        assert!(rounded_decode(&squashed, PolicyOverride::Auto).is_ok());
        assert_eq!(
            rounded_decode(&put_in(&squashed, &["answers", "severity", "score"], json!(2.0)), PolicyOverride::Auto),
            Err(DecodeError::InvalidRounded { question: "severity".into() })
        );
    }

    /// Live capture (2026-11-11, jev-1.14.0): the real Jev backend reports a
    /// native `score` that is NOT its distribution mean (7.58 vs 7.52 — the
    /// scalar drifts up to 0.06). Per user decision 2026-11-11, `jev-*` models
    /// default to the rounded-hundredths policy (grid + attainable band, no
    /// exact-equality requirement) so this traffic decodes; forcing strict
    /// (`judge: policy: strict` / `PolicyOverride::Strict`) keeps the exact
    /// Σp·i contract and rejects the same payload.
    #[test]
    fn live_jev_score_drift_is_strict_rejected_rounded_accepted() {
        let levels: Vec<ScoreLevel> = (0..10)
            .map(|i| ScoreLevel {
                label: format!("level {i}"),
                score: i as f64,
            })
            .collect();
        let questions = vec![Question::new("drift", QuestionKind::Score(levels))];
        // Captured shape: unit mass, on-grid probabilities, score off-mean.
        let probs = json!({
            "0": 0.01, "1": 0.03, "2": 0.03, "3": 0.03, "4": 0.02,
            "5": 0.03, "6": 0.05, "7": 0.1, "8": 0.19, "9": 0.51,
        });
        let body = json!({
            "model": "jev-1.14.0",
            "answers": {
                "drift": {
                    "type": "score",
                    "score": 7.58,
                    "probabilities": probs,
                    "legend": (0..10)
                        .map(|i| (i.to_string(), json!(format!("level {i}"))))
                        .collect::<Map<String, Value>>(),
                    "confidence": 0.5,
                }
            }
        });
        // Default (Auto): jev-* models get the rounded-hundredths policy, so
        // the drifted wire score (7.58 vs mean 7.52) is accepted — grid +
        // attainable band hold; exact equality does not.
        let result = decode(&body, "jev-latest", DecisionBackend::Jev, &questions, PolicyOverride::Auto)
            .expect("default policy accepts the live Jev shape");
        let answer = &result.answers[0];
        match &answer.kind {
            AnswerKind::Score { expected_score } => {
                assert!((expected_score - 7.52).abs() < 1e-9);
            }
            other => panic!("expected score answer, got {other:?}"),
        }
        // The rounded path is recorded whenever normalization runs — even at
        // unit mass, where canonical values equal the displayed ones.
        assert!(answer.rounded.is_some(), "normalize path is recorded");
        // Forced strict (the reference contract) still rejects the drift.
        assert_eq!(
            decode(&body, "jev-latest", DecisionBackend::Jev, &questions, PolicyOverride::Strict),
            Err(DecodeError::InvalidAnswer { question: "drift".into(), reason: "score_or_legend" })
        );
    }

    /// Live capture (2026-11-11, decision-model-preview via the workspace
    /// endpoint): real criterion answers sum as low as 0.98 across ten
    /// on-grid bins — the Elixir reference's `min(count·0.005, 0.01)` mass
    /// cap rejects this traffic. The port widens that redundant pre-filter
    /// to the rounding-eaten bound (`count × 0.005`); the bounds and
    /// attainable-pre-image checks still reject any distribution that no
    /// unit-sum pre-rounding could have produced, and strict stays exact.
    #[test]
    fn live_alibaba_missing_mass_within_rounding_slack_is_accepted() {
        let levels: Vec<ScoreLevel> = (0..10)
            .map(|i| ScoreLevel {
                label: format!("level {i}"),
                score: i as f64,
            })
            .collect();
        let questions = vec![Question::new("validity", QuestionKind::Score(levels))];
        let body = json!({
            "model": "decision-model-preview",
            "answers": {
                "validity": {
                    "type": "score",
                    "score": 4.01,
                    "probabilities": {
                        "0": 0.39, "1": 0.05, "2": 0.03, "3": 0.04, "4": 0.03,
                        "5": 0.04, "6": 0.04, "7": 0.04, "8": 0.05, "9": 0.27,
                    },
                    "legend": (0..10)
                        .map(|i| (i.to_string(), json!(format!("level {i}"))))
                        .collect::<Map<String, Value>>(),
                    "confidence": 0.55,
                }
            }
        });
        let result = decode(
            &body,
            "decision-model-preview",
            DecisionBackend::Alibaba,
            &questions,
            PolicyOverride::Auto,
        )
        .expect("live workspace shape (mass 0.98) decodes under the default policy");
        let answer = &result.answers[0];
        // Canonical = displayed / 0.98; the wire weighted index is 3.90, so
        // the canonical expectation is 3.90 / 0.98 ≈ 3.9796.
        match &answer.kind {
            AnswerKind::Score { expected_score } => {
                assert!((expected_score - 3.90 / 0.98).abs() < 1e-9);
            }
            other => panic!("expected score answer, got {other:?}"),
        }
        let provenance = answer.rounded.as_ref().expect("normalize path is recorded");
        assert!((provenance.wire_distribution_sum - 0.98).abs() < 1e-12);
        // The reference's exact-mass contract still rejects the same payload.
        assert_eq!(
            decode(
                &body,
                "decision-model-preview",
                DecisionBackend::Alibaba,
                &questions,
                PolicyOverride::Strict,
            ),
            Err(DecodeError::InvalidDistribution {
                question: "validity".into()
            })
        );
    }

    #[test]
    fn rounded_choices_use_canonical_selected_probability() {
        let body = put_in(
            &fixture_response(),
            &["answers", "route", "probabilities"],
            json!({"billing": 0.79, "technical": 0.2}),
        );
        let result = rounded_decode(&body, PolicyOverride::Auto).expect("decode");
        let choice = &result.answers[0];
        assert_eq!(
            choice.kind,
            AnswerKind::Choice {
                index: 0,
                choice_id: "billing".into()
            }
        );
        assert_eq!(choice.provider_confidence, Some(0.6));
        assert!((choice.selected_probability.unwrap() - 0.79 / 0.99).abs() < 1e-12);
        // Strict policy rejects the 0.99 mass; under the Jev gate (request
        // jev-latest + response jev-pinned are both jev-*) the forced rounded
        // policy applies and renormalizes the same way.
        assert_eq!(
            rounded_decode(&body, PolicyOverride::Strict),
            Err(DecodeError::InvalidDistribution { question: "route".into() })
        );
        let jev_forced = decode(
            &body,
            "jev-latest",
            DecisionBackend::Jev,
            &fixture_questions(),
            PolicyOverride::RoundedHundredths,
        )
        .expect("forced rounded applies inside the jev-* gate");
        assert!((jev_forced.answers[0].selected_probability.unwrap() - 0.79 / 0.99).abs() < 1e-12);
    }

    #[test]
    fn hundredths_policy_rejects_malformed_and_off_grid_shapes() {
        let reject = |values: Vec<f64>| {
            rounded::normalize(&values, ScoreCheck::Choice).is_err()
        };
        assert!(reject(vec![-0.01, 0.51, 0.5]));
        assert!(reject(vec![1.01, 0.0]));
        assert!(reject(vec![0.0, 0.0]));
        assert!(reject(vec![0.495, 0.505])); // off-grid halves
        assert!(reject(vec![0.48, 0.5])); // mass loss beyond display rounding
        assert!(reject(vec![0.9, 0.9])); // double mass
        assert!(reject(vec![0.99; 11])); // over cardinality cap
        assert!(rounded::normalize(&[0.49, 0.5], ScoreCheck::Choice).is_ok());
        assert!(rounded::normalize(&[0.0, 1.0], ScoreCheck::Score(Some(1.0))).is_ok());
        assert!(rounded::normalize(&[0.0, 1.0], ScoreCheck::Score(Some(f64::NAN))).is_err());
        assert!(rounded::normalize(&[], ScoreCheck::Choice).is_err());
        assert!(rounded::normalize(&[0.5], ScoreCheck::Choice).is_err());
    }

    #[test]
    fn rounded_policy_preserves_key_legend_and_confidence_validation() {
        let bodies = vec![
            put_in(
                &fixture_response(),
                &["answers", "route", "probabilities"],
                json!({"billing": 0.8, "other": 0.2}),
            ),
            put_in(&fixture_response(), &["answers", "severity", "legend", "1"], json!("wrong")),
            put_in(&fixture_response(), &["answers", "route", "confidence"], json!(1.1)),
        ];
        for body in bodies {
            assert!(rounded_decode(&body, PolicyOverride::Auto).is_err(), "body was {body}");
        }
    }

    #[test]
    fn auto_policy_follows_encoded_model_and_strict_stays_usable() {
        let body = fixture_response()
            .as_object()
            .cloned()
            .map(|mut map| {
                map.insert("model".into(), json!("decision-model-preview"));
                put_in(
                    &Value::Object(map),
                    &["answers", "route", "probabilities"],
                    json!({"billing": 0.79, "technical": 0.2}),
                )
            })
            .unwrap();
        // Auto: Alibaba + decision-model-preview applies the rounded policy.
        assert!(decode(&body, "decision-model-preview", DecisionBackend::Alibaba, &fixture_questions(), PolicyOverride::Auto).is_ok());
        // Strict override stays usable under the same auto condition.
        assert_eq!(
            decode(&body, "decision-model-preview", DecisionBackend::Alibaba, &fixture_questions(), PolicyOverride::Strict),
            Err(DecodeError::InvalidDistribution { question: "route".into() })
        );
        // A different requested model keeps strict semantics even with the
        // rounded intent: the gate requires decision-model-preview end to end.
        assert_eq!(
            decode(&body, "other-model", DecisionBackend::Alibaba, &fixture_questions(), PolicyOverride::RoundedHundredths),
            Err(DecodeError::InvalidDistribution { question: "route".into() })
        );
        // Forced rounded policy under a matching gate decodes cleanly.
        assert!(decode(&body, "decision-model-preview", DecisionBackend::Alibaba, &fixture_questions(), PolicyOverride::RoundedHundredths).is_ok());
        // Jev answering as a non-jev model stays strict, auto or forced.
        assert_eq!(
            decode(&body, "decision-model-preview", DecisionBackend::Jev, &fixture_questions(), PolicyOverride::Auto),
            Err(DecodeError::InvalidDistribution { question: "route".into() })
        );
        // Jev + jev-* models on BOTH sides apply the rounded policy by default
        // (user decision 2026-11-11: real Jev score scalars drift from Σp·i).
        let mut jev_body = body.clone();
        jev_body["model"] = json!("jev-1.14.0");
        assert!(decode(&jev_body, "jev-latest", DecisionBackend::Jev, &fixture_questions(), PolicyOverride::Auto).is_ok());
        // ...and forced strict still rejects its 0.99-mass contradiction.
        assert_eq!(
            decode(&jev_body, "jev-latest", DecisionBackend::Jev, &fixture_questions(), PolicyOverride::Strict),
            Err(DecodeError::InvalidDistribution { question: "route".into() })
        );
    }

    #[test]
    fn alibaba_policy_keeps_high_precision_strict_and_large_choice_responses() {
        let body = put_in(
            &fixture_response(),
            &["answers", "severity", "probabilities"],
            json!({"0": 0.123, "1": 0.234, "2": 0.643}),
        );
        let body = put_in(&body, &["answers", "severity", "score"], json!(1.52));
        let result = rounded_decode(&body, PolicyOverride::Auto).expect("decode");
        let score = &result.answers[2];
        // Strict fast path: no rounded normalization happened.
        assert!(score.rounded.is_none());
        assert_eq!(score.policy, DistributionPolicy::AlibabaRoundedHundredths);
        let AnswerKind::Score { expected_score } = score.kind else {
            panic!("score answer");
        };
        assert!((expected_score - (0.123 * -10.0 + 0.234 * 0.0 + 0.643 * 100.0)).abs() < 1e-12);

        // Large cardinality choices still decode under the rounded policy
        // because their mass is exact.
        let choices: Vec<String> = (0..11).map(|i| i.to_string()).collect();
        let questions = vec![Question::new(
            "pick",
            QuestionKind::Choice(
                choices
                    .iter()
                    .map(|id| ChoiceOption {
                        id: id.clone(),
                        label: id.clone(),
                    })
                    .collect(),
            ),
        )];
        let mut probabilities = Map::new();
        for id in &choices {
            probabilities.insert(id.clone(), json!(if id == "0" { 1.0 } else { 0.0 }));
        }
        let wire = json!({
            "model": "decision-model-preview",
            "answers": {
                "pick": {
                    "type": "choice",
                    "choice": "0",
                    "probabilities": Value::Object(probabilities)
                }
            }
        });
        assert!(decode(
            &wire,
            "decision-model-preview",
            DecisionBackend::Alibaba,
            &questions,
            PolicyOverride::Auto
        )
        .is_ok());
    }

    #[test]
    fn display_errors_carry_ids_and_reasons_only() {
        let err = DecodeError::InvalidAnswer {
            question: "severity".into(),
            reason: "score_or_legend",
        };
        let rendered = err.to_string();
        assert!(rendered.contains("severity"));
        assert!(!rendered.contains("fixture"));
        let wire_err = EncodeError::TooManyScoreLevels {
            question: "levels".into(),
        };
        assert!(wire_err.to_string().contains("levels"));
    }
}
