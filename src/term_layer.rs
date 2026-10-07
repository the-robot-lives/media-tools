//! The CLI's `tracing` subscriber: renders `media_tool::telemetry` UI events to the terminal.
//!
//! This is the binary's half of the split introduced with [`media_tool::telemetry`]. Library
//! code emits presentation events on the `media_tool::ui` target; this layer turns each one
//! back into the exact `media_tool::ui::*` call the library used to make itself, so the CLI's
//! stderr output is unchanged. Events on the `media_tool::progress` target are ignored here —
//! they exist for front-ends that want structured progress.

use media_tool::telemetry::{ui_kind, TARGET_UI};
use media_tool::ui;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

/// Collects the fields of one `media_tool::ui` event.
#[derive(Default)]
struct UiVisitor {
    kind: Option<&'static str>,
    message: String,
    label: String,
    value: String,
    id: String,
    asset_type: String,
    service: String,
    index: usize,
    total: usize,
}

/// Map a recorded `ui_kind` string back onto the `'static` constant, so the dispatch below
/// can match on it without allocating.
fn kind_from(value: &str) -> Option<&'static str> {
    const KINDS: [&str; 12] = [
        ui_kind::BANNER,
        ui_kind::STEP,
        ui_kind::OK,
        ui_kind::WARN,
        ui_kind::FAIL,
        ui_kind::INFO,
        ui_kind::VERBOSE,
        ui_kind::PROGRESS_LABEL,
        ui_kind::PLAN_ITEM,
        ui_kind::PLAN_DETAIL,
        ui_kind::BLANK,
        ui_kind::RAW,
    ];
    KINDS.into_iter().find(|k| *k == value)
}

impl Visit for UiVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "ui_kind" => self.kind = kind_from(value),
            "message" => self.message.push_str(value),
            "label" => self.label.push_str(value),
            "value" => self.value.push_str(value),
            "id" => self.id.push_str(value),
            "asset_type" => self.asset_type.push_str(value),
            "service" => self.service.push_str(value),
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "index" => self.index = value as usize,
            "total" => self.total = value as usize,
            _ => {}
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

/// Renders `media_tool::ui` events through [`media_tool::ui`].
pub struct TermLayer;

impl<S: Subscriber> Layer<S> for TermLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != TARGET_UI {
            return;
        }

        let mut visitor = UiVisitor::default();
        event.record(&mut visitor);

        let Some(kind) = visitor.kind else { return };
        match kind {
            ui_kind::BANNER => ui::banner(&visitor.message),
            ui_kind::STEP => ui::step(&visitor.message),
            ui_kind::OK => ui::ok(&visitor.message),
            ui_kind::WARN => ui::warn_msg(&visitor.message),
            ui_kind::FAIL => ui::fail_msg(&visitor.message),
            ui_kind::INFO => ui::info(&visitor.message),
            ui_kind::VERBOSE => ui::verbose(&visitor.message),
            ui_kind::BLANK => ui::blank(),
            ui_kind::RAW => ui::raw(&visitor.message),
            ui_kind::PROGRESS_LABEL => {
                ui::progress_label(visitor.index, visitor.total, &visitor.label)
            }
            ui_kind::PLAN_ITEM => ui::plan_item(&visitor.id, &visitor.asset_type, &visitor.service),
            ui_kind::PLAN_DETAIL => ui::plan_detail(&visitor.label, &visitor.value),
            _ => {}
        }
    }
}
