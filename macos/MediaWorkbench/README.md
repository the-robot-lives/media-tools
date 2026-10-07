# Media Workbench (macOS)

First vertical slice of the media-tool desktop app: a SwiftUI shell plus a
working **Settings → Keys** screen that reads and writes the real
`~/.config/media-tool/media-tool.yaml`. No Rust FFI exists yet — Library and
Runs are honest placeholders naming what blocks them.

The point of this slice is to prove the [tobor-kit](../../../../Libs/tobor-kit)
integration end-to-end before any FFI work starts.

## Build & run

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
| `Tests/MediaWorkbenchKitTests/` | YAML round-trip, catalog, store, key-default tests |

Logic lives in the `MediaWorkbenchKit` library so it is testable; the executable
target is a thin `@main` shell.

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
