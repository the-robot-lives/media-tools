# Media Workbench (macOS)

First vertical slices of the media-tool desktop app: a SwiftUI shell, a working
**Settings → Keys** screen that reads and writes the real
`~/.config/media-tool/media-tool.yaml`, and a **narrow UniFFI bridge** onto the
Rust library (`MediaToolBridge`). Library and Runs are still placeholders.

## Build & run

The Swift package links a Rust staticlib, so the Rust side must be built first.
From the **repo root**:

```bash
make ffi        # cargo build --release + regenerate the Swift bindings
```

Then, from this directory:

```bash
swift build
swift test
swift run MediaWorkbench
```

`Package.swift` consumes tobor-kit by **local path**, not by tag (the published
`v0.1.0` predates the Swift surface entirely). The default relative path assumes
a canonical monorepo checkout; from a git worktree — which sits at a different
depth — set the override:

```bash
TOBOR_KIT_PATH=/abs/path/to/Portfolio/Libs/tobor-kit swift build
```

## Layout

| Path | Role |
| --- | --- |
| `Sources/MediaWorkbench/` | `@main` App lifecycle entry point, nothing else |
| `Sources/MediaWorkbenchKit/Catalog/` | media-tool's 16-provider catalog + modality grouping |
| `Sources/MediaWorkbenchKit/Config/` | `media-tool.yaml` read/write, `LLMInferenceConfigStoring` adapter |
| `Sources/MediaWorkbenchKit/Views/` | Root split view, Keys settings, placeholders |
| `Sources/MediaToolBridge/` | **Generated** UniFFI Swift bindings — do not hand-edit |
| `Sources/media_toolFFI/` | **Generated** C header + `module.modulemap` for the staticlib |
| `Tests/MediaWorkbenchKitTests/` | YAML round-trip, catalog, store, key-default tests |
| `Tests/MediaToolBridgeTests/` | Progress-path proof: `listPrompts`/`parsePrompt`/`dryRun` against a temp fixture |

Logic lives in the `MediaWorkbenchKit` library so it is testable; the executable
target is a thin `@main` shell.

## The Rust bridge (`MediaToolBridge`)

Phase 1a exposes three calls and nothing else. Real generation and cancellation
are the next slice.

```swift
func parsePrompt(path: String) throws -> PromptSummary
func listPrompts(dir: String, recursive: Bool) throws -> [PromptSummary]
func dryRun(paths: [String], observer: ProgressObserver) throws -> RunSummary
```

`dryRun` streams `ProgressEvent` — a Swift enum with associated values, never a
string or a JSON blob — to a `ProgressObserver` you implement:

```swift
final class Recorder: ProgressObserver, @unchecked Sendable {
    func onEvent(event: ProgressEvent) {
        if case let .planItem(promptId, assetType, service, model, outputPath) = event {
            …
        }
    }
}
```

Rust scopes its `tracing` subscriber to the single `dryRun` call (thread-local
dispatcher + current-thread runtime), so nothing is installed globally and an
observer stops hearing anything the moment its call returns. See
`src/ffi.rs` module docs.

### Regenerating the bindings

`Sources/MediaToolBridge/media_tool.swift`, `Sources/media_toolFFI/media_toolFFI.h`
and `Sources/media_toolFFI/module.modulemap` are **generated and committed**.
After any change to `src/ffi.rs`, from the repo root:

```bash
make ffi
```

which is:

```bash
cargo build --release
cargo run --release --bin uniffi-bindgen -- generate \
    --library target/release/libmedia_tool.dylib \
    --language swift --no-format \
    --out-dir target/uniffi-swift
# then copy the three files into place (see the Makefile `ffi` target)
```

`Package.swift` finds `libmedia_tool.a` relative to its own location
(`../../target/release`); `MEDIA_TOOL_LIB_DIR` overrides that for a debug build
or a CI artifact, the same way `TOBOR_KIT_PATH` overrides the tobor-kit path.

## Config contract

`media-tool.yaml` is user-owned. This app adds and maintains exactly one
top-level section — `keys:` — and must not disturb `defaults`, `image_tiers`,
`max_prompt_chars`, `refine_model`, `prompt_guidance`, or anything a future
media-tool release adds.

Writes therefore parse to a generic Yams `Node` tree, mutate only the `keys`
subtree, and re-emit. Unknown fields *inside* a provider block survive too.
`YamlRoundTripTests` pins this behaviour.

Known limitation: YAML comments are not part of the node tree, so a rewrite
drops them. Values, structure, and key order all survive.

```yaml
keys:
  openai_chat:
    env: OPENAI_API_KEY     # or `key:` for a literal — discouraged
    base_url: https://api.openai.com/v1
    model: gpt-4o
    api_shape: openai
```

New provider entries default to `env:`, never a literal — no API key at rest in
a plaintext config file. Provider ids use underscores so they are clean YAML
keys; `MediaProvider.serviceID` gives back the hyphenated form the CLI takes.

Env var names are copied from `src/providers/mod.rs::api_key_env`, which is
authoritative; `CatalogTests` fails if they drift. Note `zai` genuinely reads
`XAI_API_KEY` there, not `ZAI_API_KEY`.
