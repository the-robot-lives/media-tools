//! UniFFI binding generator, pinned to this crate's `uniffi` version.
//!
//! Invoked by `make ffi`; see `macos/MediaWorkbench/README.md` for the full command.

fn main() {
    uniffi::uniffi_bindgen_main()
}
