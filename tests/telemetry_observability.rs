//! A front-end must be able to observe a generation run with no terminal attached.
//!
//! This is the regression guard for the whole point of the telemetry layer: link `media_tool`,
//! install a `tracing` subscriber, drive a run, and read structured progress out of it —
//! without a TTY, without `ui::*`, and without parsing any formatted text.

use std::sync::{Arc, Mutex};

use media_tool::pipeline::{self, PipelineConfig};
use media_tool::schema::parse_prompt_file;
use media_tool::telemetry::TARGET_PROGRESS;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::Registry;

/// One observed progress event: its name plus the string/number fields it carried.
#[derive(Debug, Clone, Default)]
struct Observed {
    name: String,
    strings: Vec<(String, String)>,
    numbers: Vec<(String, u64)>,
    bools: Vec<(String, bool)>,
}

impl Observed {
    fn string(&self, key: &str) -> Option<&str> {
        self.strings
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn number(&self, key: &str) -> Option<u64> {
        self.numbers.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
    }

    fn boolean(&self, key: &str) -> Option<bool> {
        self.bools.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
    }
}

impl Visit for Observed {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "event" {
            self.name = value.to_string();
        } else {
            self.strings
                .push((field.name().to_string(), value.to_string()));
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.numbers.push((field.name().to_string(), value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.bools.push((field.name().to_string(), value));
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

/// Collects every `media_tool::progress` event. Stands in for the GUI's subscriber.
struct CollectLayer(Arc<Mutex<Vec<Observed>>>);

impl<S: Subscriber> Layer<S> for CollectLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != TARGET_PROGRESS {
            return;
        }
        let mut observed = Observed::default();
        event.record(&mut observed);
        self.0.lock().unwrap().push(observed);
    }
}

const PROMPT: &str = r#"schema: "0.4"
id: telemetry-observability-001
type: image
service: gemini

prompt:
  text: |
    A single flat-shaded teal hexagon on a white background.

output:
  formats:
    - format: png
"#;

#[tokio::test]
async fn a_subscriber_observes_a_run_without_a_terminal() {
    let dir = std::env::temp_dir().join(format!("media-tool-telemetry-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let prompt_path = dir.join("observability.media.prompt");
    std::fs::write(&prompt_path, PROMPT).expect("write prompt");

    let events = Arc::new(Mutex::new(Vec::new()));
    let subscriber = Registry::default().with(CollectLayer(Arc::clone(&events)));
    let _guard = tracing::subscriber::set_default(subscriber);

    let prompt = parse_prompt_file(&prompt_path).expect("parse prompt");
    let config = PipelineConfig {
        variant_count: 1,
        // Dry run: the plan is walked and reported, but no provider is called.
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

    pipeline::run_generation(vec![prompt], &config)
        .await
        .expect("dry run");

    let observed = events.lock().unwrap().clone();
    let names: Vec<&str> = observed.iter().map(|e| e.name.as_str()).collect();

    // The run bracket is observable...
    assert!(
        names.contains(&"run.started"),
        "expected run.started, saw {names:?}"
    );
    assert!(
        names.contains(&"run.completed"),
        "expected run.completed, saw {names:?}"
    );
    assert!(
        names.contains(&"plan.item"),
        "expected plan.item, saw {names:?}"
    );

    // ...and carries typed fields, not prose a consumer would have to parse.
    let started = observed
        .iter()
        .find(|e| e.name == "run.started")
        .expect("run.started");
    assert_eq!(started.number("total_outputs"), Some(1));
    assert_eq!(started.number("total_prompts"), Some(1));
    assert_eq!(started.boolean("dry_run"), Some(true));

    let plan = observed
        .iter()
        .find(|e| e.name == "plan.item")
        .expect("plan.item");
    assert_eq!(
        plan.string("prompt_id"),
        Some("telemetry-observability-001")
    );
    assert_eq!(plan.string("asset_type"), Some("Image"));
    assert!(
        plan.string("output_path")
            .is_some_and(|p| p.ends_with(".png")),
        "plan.item should name the output file, got {:?}",
        plan.string("output_path")
    );

    let completed = observed
        .iter()
        .find(|e| e.name == "run.completed")
        .expect("run.completed");
    assert_eq!(completed.boolean("dry_run"), Some(true));

    let _ = std::fs::remove_dir_all(&dir);
}
