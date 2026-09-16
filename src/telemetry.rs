//! Structured progress telemetry for the media-tool library.
//!
//! Library code never writes to a terminal. It publishes [`tracing`] spans and events; a
//! front-end installs a subscriber and decides what to do with them. The CLI installs a
//! subscriber that renders the presentation events through [`crate::ui`] (byte-identical to
//! the old direct `ui::*` calls); a GUI installs one that forwards the progress events over
//! its own transport.
//!
//! # Two targets, two audiences
//!
//! | Target | Emitted by | Consumed by | Shape |
//! |---|---|---|---|
//! | [`TARGET_UI`] (`media_tool::ui`) | every status line the CLI used to print | the CLI terminal renderer | `ui_kind` + already-formatted `message` (plus discrete fields where the old `ui::*` call had them) |
//! | [`TARGET_PROGRESS`] (`media_tool::progress`) | generation lifecycle, eval, refine, provider, renderer | a GUI / test harness | named `event` + typed fields, **no formatted prose** |
//!
//! A consumer that wants to *render progress* subscribes to [`TARGET_PROGRESS`] only and never
//! parses text. [`TARGET_UI`] exists so the CLI's output can stay exactly as it was; it is a
//! presentation channel, not an API.
//!
//! # Progress taxonomy (`target = media_tool::progress`)
//!
//! Every event carries a static `event` field naming it. Anything with a duration is a span;
//! anything that is a point in time is an event.
//!
//! ## Spans
//!
//! - `run` — one call to [`crate::pipeline::run_generation`].
//!   Fields: `total_outputs: u64`, `total_prompts: u64`, `variant_count: u64`, `dry_run: bool`.
//! - `output` — one output artifact within a run.
//!   Fields: `prompt_id: str`, `output_path: str`, `index: u64`, `total: u64`.
//! - `attempt` — one provider attempt at an output.
//!   Fields: `prompt_id: str`, `output_path: str`, `attempt: u64`, `total_attempts: u64`,
//!   `service: str`, `model: str`.
//!
//! ## Events
//!
//! | `event` | Fields |
//! |---|---|
//! | `run.started` | `total_outputs`, `total_prompts`, `variant_count`, `dry_run` |
//! | `run.completed` | `succeeded`, `failed`, `dry_run` |
//! | `plan.item` | `prompt_id`, `asset_type`, `service`, `model`, `output_path` |
//! | `preferences.applied` | `prompt_id`, `service`, `model`, `quality`, `snippet_count` (`u64`) — string fields are `""` when that field was not set by a config rule |
//! | `output.started` | `prompt_id`, `index`, `total`, `output_path` |
//! | `output.completed` | `prompt_id`, `output_path`, `ok`, `service`, `score` (`f64`, `-1.0` when unscored) |
//! | `attempt.started` | `prompt_id`, `attempt`, `total_attempts`, `service`, `model`, `output_path` |
//! | `attempt.completed` | `prompt_id`, `attempt`, `service`, `output_path`, `ok`, `score` |
//! | `attempt.failed` | `prompt_id`, `attempt`, `service`, `output_path`, `reason` |
//! | `eval.started` | `output_path`, `mode`, `model` |
//! | `eval.criterion` | `criterion`, `weight`, `score` (0..1), `threshold`, `passed` |
//! | `eval.completed` | `output_path`, `weighted`, `threshold`, `passed`, `reject_hits` (`u64`) |
//! | `refine.started` | `prompt_id`, `attempt`, `service`, `reason` |
//! | `refine.applied` | `prompt_id`, `attempt`, `service`, `chars_before`, `chars_after` |
//! | `provider.request` | `service`, `model`, `url`, `attempt` |
//! | `provider.response` | `service`, `status` (`u16`, `0` = transport error), `ok`, `attempt` |
//! | `provider.retry` | `service`, `attempt`, `delay_ms`, `reason` |
//! | `renderer.invoked` | `tool`, `input`, `output` |
//! | `renderer.unavailable` | `tool`, `available` (always `false`; present so a consumer can key on one field) |
//!
//! `score` is `-1.0` rather than absent when no score exists, so a consumer can read one
//! typed field instead of branching on field presence.
//!
//! # UI taxonomy (`target = media_tool::ui`)
//!
//! One event per status line, `ui_kind` selecting the renderer:
//! `banner`, `step`, `ok`, `warn`, `fail`, `info`, `verbose`, `progress_label`, `plan_item`,
//! `plan_detail`, `blank` (an empty stderr line), `raw` (a pre-formatted stderr line).
//! `message` holds the text for the single-string kinds; `progress_label` adds `index`/`total`/
//! `label`, `plan_item` adds `id`/`asset_type`/`service`, `plan_detail` adds `label`/`value`.

use tracing::{event, Level};

/// Target for terminal-presentation events (the CLI's channel).
pub const TARGET_UI: &str = "media_tool::ui";

/// Target for structured generation-progress events (a front-end's channel).
pub const TARGET_PROGRESS: &str = "media_tool::progress";

// ---------------------------------------------------------------------------
// Presentation channel
// ---------------------------------------------------------------------------

/// Which terminal renderer a [`TARGET_UI`] event asks for.
pub mod ui_kind {
    pub const BANNER: &str = "banner";
    pub const STEP: &str = "step";
    pub const OK: &str = "ok";
    pub const WARN: &str = "warn";
    pub const FAIL: &str = "fail";
    pub const INFO: &str = "info";
    pub const VERBOSE: &str = "verbose";
    pub const PROGRESS_LABEL: &str = "progress_label";
    pub const PLAN_ITEM: &str = "plan_item";
    pub const PLAN_DETAIL: &str = "plan_detail";
    pub const BLANK: &str = "blank";
    pub const RAW: &str = "raw";
}

fn emit_ui(kind: &'static str, message: &str) {
    event!(target: TARGET_UI, Level::INFO, ui_kind = kind, message = message);
}

/// Boxed banner line.
pub fn banner(msg: &str) {
    emit_ui(ui_kind::BANNER, msg);
}

/// A new phase of work is starting.
pub fn step(msg: &str) {
    emit_ui(ui_kind::STEP, msg);
}

/// Something succeeded.
pub fn ok(msg: &str) {
    emit_ui(ui_kind::OK, msg);
}

/// Something is off but work continues.
pub fn warn_msg(msg: &str) {
    emit_ui(ui_kind::WARN, msg);
}

/// Something failed.
pub fn fail_msg(msg: &str) {
    emit_ui(ui_kind::FAIL, msg);
}

/// Incidental detail worth showing by default.
pub fn info(msg: &str) {
    emit_ui(ui_kind::INFO, msg);
}

/// Detail shown only under `--verbose` (callers gate on their own verbose flag).
pub fn verbose(msg: &str) {
    emit_ui(ui_kind::VERBOSE, msg);
}

/// An empty stderr line (spacing).
pub fn blank() {
    emit_ui(ui_kind::BLANK, "");
}

/// A pre-formatted stderr line, printed verbatim.
pub fn raw(msg: &str) {
    emit_ui(ui_kind::RAW, msg);
}

/// `[index/total] label` progress header.
pub fn progress_label(index: usize, total: usize, label: &str) {
    event!(
        target: TARGET_UI,
        Level::INFO,
        ui_kind = ui_kind::PROGRESS_LABEL,
        index = index as u64,
        total = total as u64,
        label = label,
    );
}

/// Plan header for one prompt.
pub fn plan_item(id: &str, asset_type: &str, service: &str) {
    event!(
        target: TARGET_UI,
        Level::INFO,
        ui_kind = ui_kind::PLAN_ITEM,
        id = id,
        asset_type = asset_type,
        service = service,
    );
}

/// `label: value` detail line under a plan item.
pub fn plan_detail(label: &str, value: &str) {
    event!(
        target: TARGET_UI,
        Level::INFO,
        ui_kind = ui_kind::PLAN_DETAIL,
        label = label,
        value = value,
    );
}

// ---------------------------------------------------------------------------
// Progress channel
// ---------------------------------------------------------------------------

/// Structured generation-progress events — the front-end API.
///
/// See the module docs for the full taxonomy.
pub mod progress {
    use tracing::{event, span, Level, Span};

    /// Sentinel used where a `score` field exists but no score was produced.
    pub const NO_SCORE: f64 = -1.0;

    /// Span covering one [`crate::pipeline::run_generation`] call.
    pub fn run_span(
        total_outputs: usize,
        total_prompts: usize,
        variant_count: usize,
        dry_run: bool,
    ) -> Span {
        span!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            "run",
            total_outputs = total_outputs as u64,
            total_prompts = total_prompts as u64,
            variant_count = variant_count as u64,
            dry_run = dry_run,
        )
    }

    /// Span covering one output artifact within a run.
    pub fn output_span(prompt_id: &str, output_path: &str, index: usize, total: usize) -> Span {
        span!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            "output",
            prompt_id = prompt_id,
            output_path = output_path,
            index = index as u64,
            total = total as u64,
        )
    }

    /// Span covering one provider attempt at an output.
    pub fn attempt_span(
        prompt_id: &str,
        output_path: &str,
        attempt: usize,
        total_attempts: usize,
        service: &str,
        model: &str,
    ) -> Span {
        span!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            "attempt",
            prompt_id = prompt_id,
            output_path = output_path,
            attempt = attempt as u64,
            total_attempts = total_attempts as u64,
            service = service,
            model = model,
        )
    }

    /// A generation run has begun.
    pub fn run_started(
        total_outputs: usize,
        total_prompts: usize,
        variant_count: usize,
        dry_run: bool,
    ) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "run.started",
            total_outputs = total_outputs as u64,
            total_prompts = total_prompts as u64,
            variant_count = variant_count as u64,
            dry_run = dry_run,
        );
    }

    /// A generation run has finished.
    pub fn run_completed(succeeded: usize, failed: usize, dry_run: bool) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "run.completed",
            succeeded = succeeded as u64,
            failed = failed as u64,
            dry_run = dry_run,
        );
    }

    /// The preference cascade (design §4.3) supplied settings a prompt file did
    /// not. Emitted once per prompt, only when something was actually applied.
    ///
    /// `service` / `model` / `quality` are `""` when that field came from the
    /// prompt file or a CLI flag rather than a config rule — a consumer reads
    /// one typed field instead of branching on presence, as elsewhere here.
    pub fn preferences_applied(
        prompt_id: &str,
        service: &str,
        model: &str,
        quality: &str,
        snippet_count: usize,
    ) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "preferences.applied",
            prompt_id = prompt_id,
            service = service,
            model = model,
            quality = quality,
            snippet_count = snippet_count as u64,
        );
    }

    /// One planned output, emitted while the plan is being shown.
    pub fn plan_item(
        prompt_id: &str,
        asset_type: &str,
        service: &str,
        model: &str,
        output_path: &str,
    ) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "plan.item",
            prompt_id = prompt_id,
            asset_type = asset_type,
            service = service,
            model = model,
            output_path = output_path,
        );
    }

    /// Work on one output artifact has begun.
    pub fn output_started(prompt_id: &str, index: usize, total: usize, output_path: &str) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "output.started",
            prompt_id = prompt_id,
            index = index as u64,
            total = total as u64,
            output_path = output_path,
        );
    }

    /// Work on one output artifact has finished.
    pub fn output_completed(
        prompt_id: &str,
        output_path: &str,
        ok: bool,
        service: &str,
        score: f64,
    ) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "output.completed",
            prompt_id = prompt_id,
            output_path = output_path,
            ok = ok,
            service = service,
            score = score,
        );
    }

    /// A provider attempt is about to be made.
    pub fn attempt_started(
        prompt_id: &str,
        attempt: usize,
        total_attempts: usize,
        service: &str,
        model: &str,
        output_path: &str,
    ) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "attempt.started",
            prompt_id = prompt_id,
            attempt = attempt as u64,
            total_attempts = total_attempts as u64,
            service = service,
            model = model,
            output_path = output_path,
        );
    }

    /// A provider attempt produced an artifact (whether or not it passed eval).
    pub fn attempt_completed(
        prompt_id: &str,
        attempt: usize,
        service: &str,
        output_path: &str,
        ok: bool,
        score: f64,
    ) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "attempt.completed",
            prompt_id = prompt_id,
            attempt = attempt as u64,
            service = service,
            output_path = output_path,
            ok = ok,
            score = score,
        );
    }

    /// A provider attempt did not produce a usable artifact.
    pub fn attempt_failed(
        prompt_id: &str,
        attempt: usize,
        service: &str,
        output_path: &str,
        reason: &str,
    ) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "attempt.failed",
            prompt_id = prompt_id,
            attempt = attempt as u64,
            service = service,
            output_path = output_path,
            reason = reason,
        );
    }

    /// Scoring of one artifact has begun.
    pub fn eval_started(output_path: &str, mode: &str, model: &str) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "eval.started",
            output_path = output_path,
            mode = mode,
            model = model,
        );
    }

    /// One eval criterion was scored.
    pub fn eval_criterion(criterion: &str, weight: f64, score: f64, threshold: f64, passed: bool) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "eval.criterion",
            criterion = criterion,
            weight = weight,
            score = score,
            threshold = threshold,
            passed = passed,
        );
    }

    /// Scoring of one artifact has finished.
    pub fn eval_completed(
        output_path: &str,
        weighted: f64,
        threshold: f64,
        passed: bool,
        reject_hits: usize,
    ) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "eval.completed",
            output_path = output_path,
            weighted = weighted,
            threshold = threshold,
            passed = passed,
            reject_hits = reject_hits as u64,
        );
    }

    /// A prompt refinement round is starting.
    pub fn refine_started(prompt_id: &str, attempt: usize, service: &str, reason: &str) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "refine.started",
            prompt_id = prompt_id,
            attempt = attempt as u64,
            service = service,
            reason = reason,
        );
    }

    /// A refined prompt was accepted and will be used for the next attempt.
    pub fn refine_applied(
        prompt_id: &str,
        attempt: usize,
        service: &str,
        chars_before: usize,
        chars_after: usize,
    ) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "refine.applied",
            prompt_id = prompt_id,
            attempt = attempt as u64,
            service = service,
            chars_before = chars_before as u64,
            chars_after = chars_after as u64,
        );
    }

    /// An HTTP request is being sent to a provider.
    pub fn provider_request(service: &str, model: &str, url: &str, attempt: usize) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "provider.request",
            service = service,
            model = model,
            url = url,
            attempt = attempt as u64,
        );
    }

    /// A provider responded. `status` is `0` for a transport-level failure.
    pub fn provider_response(service: &str, status: u16, ok: bool, attempt: usize) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "provider.response",
            service = service,
            status = status,
            ok = ok,
            attempt = attempt as u64,
        );
    }

    /// A provider call is being retried (or polled again) after a delay.
    pub fn provider_retry(service: &str, attempt: usize, delay_ms: u64, reason: &str) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "provider.retry",
            service = service,
            attempt = attempt as u64,
            delay_ms = delay_ms,
            reason = reason,
        );
    }

    /// A post-processing renderer is being run.
    pub fn renderer_invoked(tool: &str, input: &str, output: &str) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "renderer.invoked",
            tool = tool,
            input = input,
            output = output,
        );
    }

    /// A post-processing renderer is not installed (or is not a known tool).
    pub fn renderer_unavailable(tool: &str) {
        event!(
            target: super::TARGET_PROGRESS,
            Level::INFO,
            event = "renderer.unavailable",
            tool = tool,
            available = false,
        );
    }
}
