//! media-tool library surface.
//!
//! Everything the `generate-media-prompt` CLI does is implemented here so that other
//! front-ends (a GUI, a test harness, another binary) can drive the same logic without
//! a terminal. The binary in `src/main.rs` is a thin CLI shell over this crate:
//! clap parsing, terminal rendering and process exit codes live there, nothing else.
//!
//! Terminal concerns (ratatui, crossterm, dialoguer, indicatif) are *not* used by the
//! orchestration entry points in [`orchestrator`]; progress is surfaced through
//! caller-supplied callbacks and returned values instead.

pub mod attachments;
pub mod dag;
pub mod eval;
pub mod fim;
pub mod orchestrator;
pub mod output;
pub mod pipeline;
pub mod prep;
pub mod provider_config;
pub mod providers;
pub mod refine;
pub mod renderers;
pub mod schema;
pub mod structural;
pub mod test_lab;
pub mod ui;
pub mod validate;

pub use orchestrator::{
    build_batches, collect_prompt_files, dependency_groups, expand_inputs, load_envrc,
    load_prompts, normalize_prompt_files, parse_quality_override, run_generation, InputIssue,
    ResolvedInputs,
};
pub use pipeline::PipelineConfig;
pub use schema::{parse_prompt_file, ParsedPrompt, Quality};
