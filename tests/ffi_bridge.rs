//! The UniFFI surface, exercised through the Rust side of the boundary.
//!
//! `swift test` proves the same three calls survive the generated bindings; this proves the
//! Rust half without a Swift toolchain in the loop. No provider is contacted: dry-run only.

use std::sync::{Arc, Mutex};

use media_tool::ffi::{dry_run, list_prompts, parse_prompt, ProgressEvent, ProgressObserver};

const PROMPT: &str = r#"schema: "0.4"
id: ffi-bridge-001
type: image
service: gemini
model: imagen-4

prompt:
  text: |
    A single flat-shaded teal hexagon on a white background.

output:
  formats:
    - format: png
"#;

const NESTED_PROMPT: &str = r#"schema: "0.4"
id: ffi-bridge-nested-001
type: image
service: gemini

prompt:
  text: |
    A nested fixture.

output:
  formats:
    - format: png
"#;

/// Stands in for the Swift `ProgressObserver`.
#[derive(Default)]
struct Recorder(Mutex<Vec<ProgressEvent>>);

impl ProgressObserver for Recorder {
    fn on_event(&self, event: ProgressEvent) {
        self.0.lock().expect("recorder poisoned").push(event);
    }
}

fn fixture_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("media-tool-ffi-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("nested")).expect("temp dir");
    std::fs::write(dir.join("top.media.prompt"), PROMPT).expect("write prompt");
    std::fs::write(dir.join("nested/deep.media.prompt"), NESTED_PROMPT).expect("write nested");
    dir
}

#[test]
fn parse_prompt_reports_typed_fields() {
    let dir = fixture_dir("parse");
    let path = dir.join("top.media.prompt");

    let summary = parse_prompt(path.display().to_string()).expect("parse");
    assert_eq!(summary.id, "ffi-bridge-001");
    assert_eq!(summary.asset_type, "Image");
    assert_eq!(summary.service.as_deref(), Some("gemini"));
    assert_eq!(summary.model.as_deref(), Some("imagen-4"));
    assert_eq!(summary.schema_version, "0.4");
    assert_eq!(summary.output_paths.len(), 1);
    assert!(summary.output_paths[0].ends_with(".png"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parse_prompt_surfaces_a_missing_file_as_an_error() {
    let err = parse_prompt("/nonexistent/nope.media.prompt".to_string())
        .expect_err("missing file must fail");
    assert!(!err.to_string().is_empty());
}

#[test]
fn list_prompts_honours_recursion() {
    let dir = fixture_dir("list");

    let shallow = list_prompts(dir.display().to_string(), false).expect("shallow list");
    assert_eq!(shallow.len(), 1, "non-recursive listing is top level only");
    assert_eq!(shallow[0].id, "ffi-bridge-001");

    let deep = list_prompts(dir.display().to_string(), true).expect("recursive list");
    assert_eq!(deep.len(), 2);
    let mut ids: Vec<&str> = deep.iter().map(|p| p.id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, vec!["ffi-bridge-001", "ffi-bridge-nested-001"]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dry_run_streams_typed_progress_to_the_observer() {
    let dir = fixture_dir("dryrun");
    let path = dir.join("top.media.prompt");

    let recorder = Arc::new(Recorder::default());
    let summary = dry_run(
        vec![path.display().to_string()],
        recorder.clone() as Arc<dyn ProgressObserver>,
    )
    .expect("dry run");

    assert_eq!(summary.total_prompts, 1);
    assert_eq!(summary.total_outputs, 1);
    assert!(summary.dry_run);

    let events = recorder.0.lock().expect("recorder poisoned").clone();

    let started = events
        .iter()
        .find(|e| matches!(e, ProgressEvent::RunStarted { .. }))
        .expect("run.started");
    match started {
        ProgressEvent::RunStarted {
            total_outputs,
            total_prompts,
            dry_run,
            ..
        } => {
            assert_eq!(*total_outputs, 1);
            assert_eq!(*total_prompts, 1);
            assert!(*dry_run);
        }
        _ => unreachable!(),
    }

    let plan = events
        .iter()
        .find(|e| matches!(e, ProgressEvent::PlanItem { .. }))
        .expect("plan.item");
    match plan {
        ProgressEvent::PlanItem {
            prompt_id,
            asset_type,
            service,
            output_path,
            ..
        } => {
            assert_eq!(prompt_id, "ffi-bridge-001");
            assert_eq!(asset_type, "Image");
            assert_eq!(service, "gemini");
            assert!(output_path.ends_with(".png"));
        }
        _ => unreachable!(),
    }

    assert!(
        events
            .iter()
            .any(|e| matches!(e, ProgressEvent::RunCompleted { dry_run: true, .. })),
        "run.completed must arrive"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The subscriber must not survive the call that installed it.
#[test]
fn the_subscriber_does_not_leak_past_the_call() {
    let dir = fixture_dir("scope");
    let path = dir.join("top.media.prompt");

    let first = Arc::new(Recorder::default());
    dry_run(
        vec![path.display().to_string()],
        first.clone() as Arc<dyn ProgressObserver>,
    )
    .expect("first dry run");
    let after_first = first.0.lock().expect("poisoned").len();
    assert!(after_first > 0);

    // A second run must not reach the first observer.
    let second = Arc::new(Recorder::default());
    dry_run(
        vec![path.display().to_string()],
        second.clone() as Arc<dyn ProgressObserver>,
    )
    .expect("second dry run");

    assert_eq!(
        first.0.lock().expect("poisoned").len(),
        after_first,
        "the first observer must be deaf once its call returned"
    );
    assert!(!second.0.lock().expect("poisoned").is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}
