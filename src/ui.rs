//! Terminal formatting for the CLI.
//!
//! Only the binary calls these: `src/term_layer.rs` renders `media_tool::telemetry` UI
//! events through them, and `main.rs` uses them directly for CLI-only status lines.
//! Library code emits telemetry events instead — see [`crate::telemetry`].

const RED: &str = "\x1b[0;31m";
const YEL: &str = "\x1b[1;33m";
const GRN: &str = "\x1b[0;32m";
const BLU: &str = "\x1b[0;34m";
const CYN: &str = "\x1b[0;36m";
const NC: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";

// <REMOVED UUID HERE> banner :: auto-generated pointer for public function banner
pub fn banner(msg: &str) {
    eprintln!(
        "\n{BLU}{BOLD}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}{NC}"
    );
    eprintln!("{BLU}{BOLD}  {msg}{NC}");
    eprintln!(
        "{BLU}{BOLD}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}{NC}"
    );
}

// <REMOVED UUID HERE> step :: auto-generated pointer for public function step
pub fn step(msg: &str) {
    eprintln!("\n{BLU}\u{25b6} {msg}{NC}");
}

// <REMOVED UUID HERE> ok :: auto-generated pointer for public function ok
pub fn ok(msg: &str) {
    eprintln!("  {GRN}\u{2705} {msg}{NC}");
}

// <REMOVED UUID HERE> warn_msg :: auto-generated pointer for public function warn_msg
pub fn warn_msg(msg: &str) {
    eprintln!("  {YEL}\u{26a0}\u{fe0f}  {msg}{NC}");
}

// <REMOVED UUID HERE> fail_msg :: auto-generated pointer for public function fail_msg
pub fn fail_msg(msg: &str) {
    eprintln!("  {RED}\u{274c} {msg}{NC}");
}

// <REMOVED UUID HERE> info :: auto-generated pointer for public function info
pub fn info(msg: &str) {
    eprintln!("  {CYN}\u{2139}\u{fe0f}  {msg}{NC}");
}

// <REMOVED UUID HERE> verbose :: auto-generated pointer for public function verbose
pub fn verbose(msg: &str) {
    eprintln!("  {CYN}   {msg}{NC}");
}

// <REMOVED UUID HERE> progress_label :: auto-generated pointer for public function progress_label
pub fn progress_label(index: usize, total: usize, label: &str) {
    eprintln!("\n  {CYN}[{index}/{total}]{NC} {label}");
}

// <REMOVED UUID HERE> plan_item :: auto-generated pointer for public function plan_item
pub fn plan_item(id: &str, asset_type: &str, service: &str) {
    eprintln!("\n  {BOLD}{id}{NC} ({asset_type}, {service})");
}

// <REMOVED UUID HERE> plan_detail :: auto-generated pointer for public function plan_detail
pub fn plan_detail(label: &str, value: &str) {
    eprintln!("    {:<7}: {}", label, value);
}

/// An empty stderr line (spacing) — the old bare `eprintln!()` calls.
pub fn blank() {
    eprintln!();
}

/// A pre-formatted stderr line, printed verbatim — for output that never went through one of
/// the styled helpers above.
pub fn raw(msg: &str) {
    eprintln!("{msg}");
}
