//! UniFFI bridge — the foreign-language front door onto [`crate::orchestrator`].
//!
//! Deliberately narrow. Phase 1a exposes exactly three calls:
//!
//! | Rust | Swift |
//! |---|---|
//! | [`parse_prompt`] | `parsePrompt(path:)` |
//! | [`list_prompts`] | `listPrompts(dir:recursive:)` |
//! | [`dry_run`] | `dryRun(paths:observer:)` |
//!
//! Real generation and cancellation are **not** exposed yet. Dry-run is enough to prove the
//! progress path end-to-end and costs nothing.
//!
//! # The progress path
//!
//! [`crate::telemetry::progress`] already publishes typed `tracing` events on
//! [`TARGET_PROGRESS`]. This module installs a [`ProgressLayer`] that filters to that target,
//! decodes each event's typed fields into a [`ProgressEvent`], and hands it to a foreign
//! [`ProgressObserver`]. No prose, no JSON: a consumer reads struct fields.
//!
//! `media_tool::ui` events are *not* forwarded — that channel is the CLI's terminal
//! presentation, not an API.
//!
//! # Subscriber scoping
//!
//! The subscriber is **never** installed globally. [`dry_run`] builds a fresh
//! [`Registry`]-plus-[`ProgressLayer`] per call and installs it with
//! [`tracing::subscriber::with_default`], which sets the *thread-local* dispatcher for the
//! duration of the closure only. The generation future is driven by a **current-thread**
//! tokio runtime created inside that closure, so every event is emitted on the same thread
//! that holds the thread-local dispatcher.
//!
//! Consequences, all of them wanted:
//!
//! - Two concurrent `dry_run` calls on two threads see only their own observer.
//! - The observer is dropped when the call returns; nothing outlives it.
//! - A host that has already installed a global subscriber (the CLI does exactly that) is
//!   untouched — the thread-local default shadows the global one for this thread only, and
//!   is removed on the way out.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::Registry;

use crate::orchestrator::{self, ResolvedInputs};
use crate::output::resolve_output_paths;
use crate::pipeline::PipelineConfig;
use crate::schema::{parse_prompt_file, ParsedPrompt};
use crate::telemetry::{progress::NO_SCORE, TARGET_PROGRESS};

// ---------------------------------------------------------------------------
// Types crossing the boundary
// ---------------------------------------------------------------------------

/// Everything a front-end needs to list a prompt without re-parsing the YAML itself.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PromptSummary {
    /// Prompt id (`id:`, or the filename stem when absent).
    pub id: String,
    /// Asset type as the pipeline resolved it: `Image`, `Audio`, `Video`, `Text`, …
    pub asset_type: String,
    /// Explicit `service:`, or `None` when the prompt leaves provider choice to the pipeline.
    pub service: Option<String>,
    /// Explicit `model:`, or `None`.
    pub model: Option<String>,
    /// Absolute-or-as-given path to the `.prompt` file itself.
    pub path: String,
    /// Every artifact this prompt would write, in declaration order.
    pub output_paths: Vec<String>,
    /// `schema:` version string.
    pub schema_version: String,
    /// Resolved quality tier.
    pub quality: String,
}

/// Outcome of a [`dry_run`] call, assembled from the same progress events the observer saw.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RunSummary {
    /// Prompts that made it into the plan.
    pub total_prompts: u64,
    /// Output artifacts the plan covers.
    pub total_outputs: u64,
    /// Always `0` for a dry run; present so the shape does not change when generation lands.
    pub succeeded: u64,
    /// Always `0` for a dry run.
    pub failed: u64,
    /// Always `true` in this slice.
    pub dry_run: bool,
}

/// One typed progress event.
///
/// Variants cover the run lifecycle. Eval, refine, provider and renderer events stay
/// [`ProgressEvent::Other`] until the generation slice needs them — a front-end can already
/// key on the name without this enum pretending to model what it has not been tested against.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum ProgressEvent {
    RunStarted {
        total_outputs: u64,
        total_prompts: u64,
        variant_count: u64,
        dry_run: bool,
    },
    PlanItem {
        prompt_id: String,
        asset_type: String,
        service: String,
        model: String,
        output_path: String,
    },
    OutputStarted {
        prompt_id: String,
        index: u64,
        total: u64,
        output_path: String,
    },
    OutputCompleted {
        prompt_id: String,
        output_path: String,
        ok: bool,
        service: String,
        /// `None` where the library emitted the `NO_SCORE` sentinel — the `-1.0` never leaks.
        score: Option<f64>,
    },
    AttemptStarted {
        prompt_id: String,
        attempt: u64,
        total_attempts: u64,
        service: String,
        model: String,
        output_path: String,
    },
    AttemptCompleted {
        prompt_id: String,
        attempt: u64,
        service: String,
        output_path: String,
        ok: bool,
        /// `None` where the library emitted the `NO_SCORE` sentinel.
        score: Option<f64>,
    },
    AttemptFailed {
        prompt_id: String,
        attempt: u64,
        service: String,
        output_path: String,
        reason: String,
    },
    RunCompleted {
        succeeded: u64,
        failed: u64,
        dry_run: bool,
    },
    /// A progress event this slice does not model yet. Carries its taxonomy name only.
    Other { event: String },
}

/// Implemented by the foreign side; called once per progress event, on the calling thread.
#[uniffi::export(with_foreign)]
pub trait ProgressObserver: Send + Sync {
    fn on_event(&self, event: ProgressEvent);
}

/// Everything that can go wrong at the boundary.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum BridgeError {
    /// A prompt file could not be read or parsed.
    #[error("{message}")]
    Parse { message: String },
    /// A path was not a file, not a directory, or otherwise unusable.
    #[error("{message}")]
    Input { message: String },
    /// The pipeline itself failed.
    #[error("{message}")]
    Run { message: String },
}

// ---------------------------------------------------------------------------
// Exported functions
// ---------------------------------------------------------------------------

/// Parse one `.prompt` file into its summary. Parsing is [`crate::schema`]'s, not ours.
#[uniffi::export]
pub fn parse_prompt(path: String) -> Result<PromptSummary, BridgeError> {
    let path = PathBuf::from(path);
    let parsed = parse_prompt_file(&path).map_err(|e| BridgeError::Parse {
        message: format!("{}: {}", path.display(), e),
    })?;
    Ok(summarize(&parsed))
}

/// List the prompts under `dir`.
///
/// `recursive` controls depth only: discovery is [`orchestrator::collect_prompt_files`] either
/// way, and a non-recursive listing keeps just the entries that are direct children of `dir`.
#[uniffi::export]
pub fn list_prompts(dir: String, recursive: bool) -> Result<Vec<PromptSummary>, BridgeError> {
    let root = PathBuf::from(&dir);
    if !root.is_dir() {
        return Err(BridgeError::Input {
            message: format!("{}: not a directory", root.display()),
        });
    }

    let ResolvedInputs { mut files, issues } =
        orchestrator::expand_inputs(std::slice::from_ref(&root), true);
    if let Some(issue) = issues.into_iter().next() {
        return Err(BridgeError::Input {
            message: format!("{:?}", issue),
        });
    }

    if !recursive {
        files.retain(|f| f.parent() == Some(root.as_path()));
    }

    files
        .iter()
        .map(|path| {
            parse_prompt_file(path)
                .map(|parsed| summarize(&parsed))
                .map_err(|e| BridgeError::Parse {
                    message: format!("{}: {}", path.display(), e),
                })
        })
        .collect()
}

/// Run the existing dry-run path over `paths`, streaming progress to `observer`.
///
/// No provider is contacted and nothing is written. See the module docs for how the
/// subscriber is scoped.
#[uniffi::export]
pub fn dry_run(
    paths: Vec<String>,
    observer: Arc<dyn ProgressObserver>,
) -> Result<RunSummary, BridgeError> {
    let inputs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();

    let ResolvedInputs { files, issues } = orchestrator::expand_inputs(&inputs, false);
    if let Some(issue) = issues.into_iter().next() {
        return Err(BridgeError::Input {
            message: format!("{:?}", issue),
        });
    }
    let files = orchestrator::normalize_prompt_files(files);

    let prompts =
        orchestrator::load_prompts(&files, &mut |_, _| {}).map_err(|e| BridgeError::Parse {
            message: e.to_string(),
        })?;

    let config = PipelineConfig {
        variant_count: 1,
        dry_run: true,
        force: false,
        model_override: None,
        verbose: false,
        refine: false,
        quality_override: None,
        service_override: None,
        no_eval: true,
        no_prep: true,
        fim_enabled: false,
        eval_url: None,
        eval_model: None,
    };

    // Counters the layer fills in as the run's own events go past — the summary is read out
    // of the same pipe the observer is, so a broken pipe cannot return a plausible summary.
    let tally = Arc::new(Mutex::new(Tally::default()));
    let layer = ProgressLayer {
        observer,
        tally: Arc::clone(&tally),
    };
    let subscriber = Registry::default().with(layer);

    let result = tracing::subscriber::with_default(subscriber, || {
        // Current-thread runtime: the whole future runs on *this* thread, so the
        // thread-local dispatcher installed above is the one in force for every event.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| BridgeError::Run {
                message: format!("runtime: {}", e),
            })?;
        runtime
            .block_on(orchestrator::run_generation(prompts, &config))
            .map_err(|e| BridgeError::Run {
                message: e.to_string(),
            })
    });
    result?;

    let tally = tally.lock().expect("tally poisoned").clone();
    Ok(RunSummary {
        total_prompts: tally.total_prompts,
        total_outputs: tally.total_outputs,
        succeeded: tally.succeeded,
        failed: tally.failed,
        dry_run: true,
    })
}

// ---------------------------------------------------------------------------
// tracing -> callback bridge
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
struct Tally {
    total_prompts: u64,
    total_outputs: u64,
    succeeded: u64,
    failed: u64,
}

/// Typed field bag for one `media_tool::progress` event.
#[derive(Default)]
struct Fields {
    name: String,
    strings: HashMap<&'static str, String>,
    numbers: HashMap<&'static str, u64>,
    floats: HashMap<&'static str, f64>,
    bools: HashMap<&'static str, bool>,
}

impl Fields {
    fn s(&self, key: &str) -> String {
        self.strings.get(key).cloned().unwrap_or_default()
    }
    fn n(&self, key: &str) -> u64 {
        self.numbers.get(key).copied().unwrap_or_default()
    }
    fn b(&self, key: &str) -> bool {
        self.bools.get(key).copied().unwrap_or_default()
    }
    /// `-1.0` is the library's "no score" sentinel; it must never reach the foreign side.
    fn score(&self) -> Option<f64> {
        self.floats.get("score").copied().filter(|v| *v != NO_SCORE)
    }
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "event" {
            self.name = value.to_string();
        } else {
            self.strings.insert(field.name(), value.to_string());
        }
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.numbers.insert(field.name(), value);
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.numbers.insert(field.name(), value.max(0) as u64);
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.floats.insert(field.name(), value);
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.bools.insert(field.name(), value);
    }
    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

/// Decode one field bag into the typed enum. `None` for anything that is not a progress event.
fn to_progress_event(f: &Fields) -> ProgressEvent {
    match f.name.as_str() {
        "run.started" => ProgressEvent::RunStarted {
            total_outputs: f.n("total_outputs"),
            total_prompts: f.n("total_prompts"),
            variant_count: f.n("variant_count"),
            dry_run: f.b("dry_run"),
        },
        "plan.item" => ProgressEvent::PlanItem {
            prompt_id: f.s("prompt_id"),
            asset_type: f.s("asset_type"),
            service: f.s("service"),
            model: f.s("model"),
            output_path: f.s("output_path"),
        },
        "output.started" => ProgressEvent::OutputStarted {
            prompt_id: f.s("prompt_id"),
            index: f.n("index"),
            total: f.n("total"),
            output_path: f.s("output_path"),
        },
        "output.completed" => ProgressEvent::OutputCompleted {
            prompt_id: f.s("prompt_id"),
            output_path: f.s("output_path"),
            ok: f.b("ok"),
            service: f.s("service"),
            score: f.score(),
        },
        "attempt.started" => ProgressEvent::AttemptStarted {
            prompt_id: f.s("prompt_id"),
            attempt: f.n("attempt"),
            total_attempts: f.n("total_attempts"),
            service: f.s("service"),
            model: f.s("model"),
            output_path: f.s("output_path"),
        },
        "attempt.completed" => ProgressEvent::AttemptCompleted {
            prompt_id: f.s("prompt_id"),
            attempt: f.n("attempt"),
            service: f.s("service"),
            output_path: f.s("output_path"),
            ok: f.b("ok"),
            score: f.score(),
        },
        "attempt.failed" => ProgressEvent::AttemptFailed {
            prompt_id: f.s("prompt_id"),
            attempt: f.n("attempt"),
            service: f.s("service"),
            output_path: f.s("output_path"),
            reason: f.s("reason"),
        },
        "run.completed" => ProgressEvent::RunCompleted {
            succeeded: f.n("succeeded"),
            failed: f.n("failed"),
            dry_run: f.b("dry_run"),
        },
        other => ProgressEvent::Other {
            event: other.to_string(),
        },
    }
}

/// Forwards `media_tool::progress` events to a foreign observer. Ignores every other target,
/// which is what keeps `media_tool::ui` — the CLI's presentation channel — out of the bridge.
struct ProgressLayer {
    observer: Arc<dyn ProgressObserver>,
    tally: Arc<Mutex<Tally>>,
}

impl<S: Subscriber> Layer<S> for ProgressLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != TARGET_PROGRESS {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);

        let decoded = to_progress_event(&fields);
        match &decoded {
            ProgressEvent::RunStarted {
                total_outputs,
                total_prompts,
                ..
            } => {
                let mut tally = self.tally.lock().expect("tally poisoned");
                tally.total_outputs = *total_outputs;
                tally.total_prompts = *total_prompts;
            }
            ProgressEvent::RunCompleted {
                succeeded, failed, ..
            } => {
                let mut tally = self.tally.lock().expect("tally poisoned");
                tally.succeeded = *succeeded;
                tally.failed = *failed;
            }
            _ => {}
        }

        self.observer.on_event(decoded);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn summarize(parsed: &ParsedPrompt) -> PromptSummary {
    let meta = &parsed.meta;
    PromptSummary {
        id: meta.id.clone(),
        asset_type: format!("{:?}", meta.asset_type),
        service: meta.service.clone(),
        model: meta.model.clone(),
        path: meta.path.display().to_string(),
        output_paths: resolve_output_paths(parsed)
            .into_iter()
            .map(|(path, _)| path.display().to_string())
            .collect(),
        schema_version: meta.schema_version.clone(),
        quality: meta.quality.as_str().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(name: &str) -> Fields {
        Fields {
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// The `-1.0` sentinel is an internal encoding; the foreign side sees absence.
    #[test]
    fn no_score_sentinel_becomes_none() {
        let mut f = fields("output.completed");
        f.floats.insert("score", NO_SCORE);
        match to_progress_event(&f) {
            ProgressEvent::OutputCompleted { score, .. } => assert_eq!(score, None),
            other => panic!("expected OutputCompleted, got {other:?}"),
        }
    }

    #[test]
    fn a_real_score_survives() {
        let mut f = fields("attempt.completed");
        f.floats.insert("score", 0.82);
        f.bools.insert("ok", true);
        match to_progress_event(&f) {
            ProgressEvent::AttemptCompleted { score, ok, .. } => {
                assert_eq!(score, Some(0.82));
                assert!(ok);
            }
            other => panic!("expected AttemptCompleted, got {other:?}"),
        }
    }

    /// Events this slice does not model keep their name rather than being dropped.
    #[test]
    fn unmodelled_events_keep_their_name() {
        match to_progress_event(&fields("eval.criterion")) {
            ProgressEvent::Other { event } => assert_eq!(event, "eval.criterion"),
            other => panic!("expected Other, got {other:?}"),
        }
    }
}
