# Media Workbench — Desktop App Design

Status: **draft for review** · Author: Loom · Date: 2026-09-16
**Goal: a macOS application for the end-to-end media-tool cycle.** Linux and
Windows are planned (§3.2) but explicitly not the target of this milestone —
every sequencing decision below optimises for reaching a working macOS app,
built on the existing `generate-media-prompt` Rust crate.

---

## Context

`generate-media-prompt` today is a capable but headless pipeline. Everything a
supervised workflow needs already exists in the crate — DAG resolution, 16
providers, a first-class `eval` engine, an interactive `--refine` loop — but it
is reachable only through a terminal, one prompt file at a time. There is no way
to see a prompt, its generated output, its grade, and its revision history in one
place; no way to compare two candidates side by side; and no way to hand the tool
to someone who does not live in a shell.

The ask is a workbench that oversees the whole arc — **author → prepare → review
→ select provider → generate → grade → revise → view** — plus two supporting
pieces of plumbing: per-provider API key management, and a way to express
preferences like "use Anthropic for SVG, Suno for music" without hand-editing
every prompt file.

This document specifies that application, the config contract it shares with the
CLI, and the release packaging that ships both as one installable artifact.

---

## 1. Ground truth — what already exists

Verified against the repo at `0722c42` (branch `develop`). **This section is
load-bearing: the design reuses these rather than rebuilding them.**

| Capability | Where | Notes |
|---|---|---|
| Prompt schema | `src/schema.rs` (`PromptPayload`) | YAML, schema 0.1–0.4 |
| Pipeline + DAG | `src/pipeline.rs`, `src/dag.rs` | `depends_on`, `as`/`collapse` inlining |
| Attachments | `src/attachments.rs` | |
| Provider registry | `src/providers/mod.rs` | 16 modules, `ProviderConstraints` |
| Config resolution | `src/provider_config.rs:8-13` | 4-step precedence chain |
| Eval / grading | `src/eval.rs` | `pass_threshold`, weighted `criteria`, `reject_if`, `mode: llm\|structural\|hybrid`, `visual` |
| Refine loop | `src/refine.rs` | `interactive_refine_loop`, LLM rewrites `prompt.text` in place |
| Renderers | `src/renderers/` | mermaid, graphviz, plantuml, puppeteer — **each already has `is_available()`** |
| Structural probes | `src/structural.rs`, `src/eval.rs` | ffprobe/ffmpeg |
| Test lab | `src/test_lab/` | axum server, `make lab`, static HTML via `include_str!` |
| Terminal UI | `ratatui` 0.29 + `crossterm` 0.28 already in `Cargo.toml` | |

**The `type` field is an open `String`, not an enum** (`src/schema.rs:66-67`,
default `"image"`). Nothing validates it. It is *mapped* — not checked — into a
closed internal `AssetType` (`src/schema.rs:317-328`): `Image, Audio, Video,
Component, ReactPage, Html, StyleGuide, Diagram, Document, Unknown`.
`AssetType::from_type_str` (`src/schema.rs:343-358`) recognises 12 strings —
`image, audio, voice, music, sfx, video, component, react-page, html,
style-guide, diagram, document` — which collapse to 9 concrete variants.
Anything else becomes `Unknown`.

**`output.formats[].format` is also an open `String`** (`src/schema.rs:149-150`)
and is used **verbatim as the file extension**
(`output.join(format!("{}.{}", stem, fmt.format))`, `src/output.rs:16`).
`output.text_format` (`schema.rs:142`) and `output.diagram_type`
(`schema.rs:140`) are likewise open `Option<String>`.

**The chat write path is format-agnostic:** `sanitize_chat_output(content,
path)` then `fs::write` (`providers/openai_chat.rs:159-163`,
`anthropic.rs:126-130`, `gemini_chat.rs:127-131`). The sanitiser strips markdown
fences for everything and has extension-specific repair only for `svg`, `mmd`,
`puml` (`providers/mod.rs:359-417`, `_ => {}`).

**FIM guidance is already file-driven**, not code-gated: `solution/<text_format>.md`
is looked up on disk (`src/fim.rs:125-128`), with existing solution docs for
graphviz, drawio, abc, plantuml, mermaid, latex, typst, lilypond, wavedrom,
katex, html, markdown and svg. This is the precedent the type registry in §4.4
follows.

**Providers today:** anthropic, dashscope, elevenlabs, gemini, gemini_chat,
grok_video, groq_chat, openai_chat, openai_tts, openrouter, qwen_image, qwen_tts,
suno, veo, wan_video, zai.

**Config chain (existing, unchanged):**
1. `$MEDIA_TOOL_CONFIG` (path or URL)
2. `$MEDIA_TOOL_CONFIG_URL`
3. `./media-tool.yaml`
4. `~/.config/media-tool/media-tool.yaml`

Existing keys: `defaults`, `image_tiers`, `max_prompt_chars`, `refine_model`,
`prompt_guidance`.

> Note on naming: the **binary** is `generate-media-prompt`; `media-tool` is the
> repo and config name. The design keeps the CLI name as-is for compatibility and
> names the GUI **Media Workbench**.

---

## 2. Gaps this design fills

1. **No GUI.** Everything is terminal-only.
2. **No snippet / house-style mechanism.** The closest primitives are
   `prompt.system` and DAG `depends_on` + `collapse`. There is no named,
   reusable fragment library. This is the single largest authoring gap.
3. **No per-provider key storage.** Keys are ad hoc env vars.
4. **No preference layer.** Choosing a provider per media type means editing
   every prompt file or passing flags every time.
5. **No packaging.** `make install` copies one binary to `~/.local/bin`. There
   is no `release`/`package`/`dist` target, **no Rust build in CI at all** (the
   only workflow builds the marketing site), and no cross-compilation setup.
   media-tool is also absent from the `Portfolio/Utilities` harness `SUBDIRS`.
6. **No open type registry.** The `type` string is open but the machinery
   behind it is a fixed enum plus a handful of whitelists, so an unrecognised
   type degrades badly rather than working generically. See §4.4 — this is a
   whitelist problem, not an architecture problem.
7. **No binary output path.** Every chat provider writes UTF-8 via `fs::write`,
   so text formats are unlimited but binary ones (`.mid`, `.epub`, `.docx`) are
   unreachable without a converter step.

---

## 3. Architecture

**Decision: native UI per platform, shared Rust core, GUI links the crate.**

```
┌─────────────────────────────────────────────────────────────┐
│  macOS: SwiftUI          Linux: GTK4        Windows: WinUI 3 │  ← thin, pixels only
│  + tobor-kit Swift pods                                      │
└───────────────┬─────────────────┬──────────────┬─────────────┘
                │                 │              │
                └──────────── UniFFI bindings ─────────────────┘
                   (Linux/GTK4 needs none — calls the crate directly)
                                  │
┌─────────────────────────────────▼───────────────────────────┐
│  generate-media-prompt  (Rust, existing crate → + lib target)│
│  schema · pipeline · dag · providers · eval · refine ·       │
│  renderers · config · NEW: snippets, preferences, session    │
└─────────────────────────────────────────────────────────────┘
                                  │
                   ┌──────────────┴──────────────┐
                   │  generate-media-prompt CLI  │  ← same crate, same config
                   └─────────────────────────────┘
```

**The governing rule: the Rust core does everything except draw pixels.** Prompt
parsing, cascade resolution, provider dispatch, grading, refinement, session
history and renderer availability probing all live in the core and are exposed
over the FFI boundary. Each native UI is a view layer over a state machine it
does not own. This is what makes platforms two and three affordable rather than
a 3× rewrite — and it is the only reason the native-per-OS choice is tractable.

### 3.1 Work required in the Rust crate

| Change | Why |
|---|---|
| Add `[lib]` target alongside the existing `[[bin]]` | Today it is bin-only |
| UniFFI annotations (`#[uniffi::export]`, `Record`, `Enum`, `Error`) | generates Swift bindings; same annotations later emit C# for WinUI |
| Swift 6 / macOS 14 target floor | set by tobor-kit's `Package.swift` |
| Extract `main.rs` orchestration into library functions | CLI becomes a thin caller, same as the GUI |
| Long-running ops behind a handle + poll/callback API | Generation takes minutes; UI must stay live |
| `snippets` module | §4.2 |
| `preferences` cascade resolver | §4.3 |
| `session` module (run history, candidates, grades) | GUI needs durable history the CLI never kept |

`reqwest` is already `rustls-tls` with `default-features = false` — no OpenSSL —
so cross-compilation is unusually clean. `[profile.release]` already sets
`strip = true`, `lto = true`.

**Binding layer: UniFFI** (mozilla/uniffi-rs, v0.32.x, production-hardened in
Firefox). Chosen over a hand-rolled C ABI, which an earlier draft of this
document specified, on four grounds:

| requirement | UniFFI |
|---|---|
| async | native: `#[uniffi::export(async_runtime = "tokio")]`, and the core already runs tokio `full` — a direct match, not an adapter. Swift call site is plain `await`. |
| progress mid-operation | callback interfaces let Rust invoke Swift methods during a run — this is the progress channel |
| cancellation | 0.32 wires Swift `Task` cancellation through to `rust_future_cancel`; `foreign_future_dropped_callback` fires on drop. Real bidirectional cancellation, not a polling flag. |
| rich types | `Result<T, E>` → Swift `throws`; structs/enums via `derive(uniffi::Record / Enum)` |

The decisive argument against hand-rolling is not callback plumbing — that is
tractable — but **ABI drift**: hand-marshalled structs and enums have no
compiler-checked contract between Rust and Swift, and become a steady source of
mismatch bugs as the schema grows. UniFFI makes the contract checked.

**Platform dividend:** Linux needs **no binding layer at all** — GTK4 via
`gtk4-rs` is Rust-native and calls the crate in-process. Windows/WinUI can use
UniFFI's C# backend generated from the same annotations. So the binding work is
paid once, for macOS, and the other two platforms inherit or bypass it.

**Escape hatch:** if UniFFI's callback-interface completion model proves too
rigid for high-frequency streaming progress (the known sharp edge —
mozilla/uniffi-rs#2633), fall back to a hand-rolled C ABI for the progress
channel specifically. Prototype the progress path first to find out early.

### 3.2 Platform sequencing

macOS ships first (it is the platform actually asked for, and tobor-kit already
has Swift mirrors). Linux and Windows follow once the FFI surface has stopped
moving. **Do not start platform two until the macOS app has shipped and the UniFFI
interface has been stable for a release** — otherwise three UIs chase a moving
boundary.

---

## 4. Config contract

One file, shared by CLI and GUI: `~/.config/media-tool/media-tool.yaml`.
Existing resolution chain and existing keys are **unchanged**; everything below is
additive and optional, so every current config keeps working untouched.

### 4.1 `keys` — per-provider credentials

Shape mirrors the tobor-kit Hologram `HostConfig :llm_providers` map
(`%{provider => %{base_url, api_key}}`), so the two stay conceptually aligned.

```yaml
keys:
  anthropic:
    api_key: {env: ANTHROPIC_API_KEY}     # resolved at call time
  openai:
    api_key: {env: OPENAI_API_KEY}
  elevenlabs:
    api_key: {keychain: "media-tool/elevenlabs"}
  suno:
    api_key: {literal: "sk-..."}          # discouraged; see below
  zai:
    api_key: {env: ZAI_API_KEY}
    base_url: "https://api.z.ai/api/coding/paas/v4"
```

**Key specs, not key values.** A key is one of `{env: NAME}`,
`{keychain: REF}`, or `{literal: ...}` — the same three-way discrimination
tobor-kit's `LLMInferenceConfig` already uses, where the value is resolved at
call time and never logged. Consequences:

- `{env:}` is the default the GUI writes, and the config file stays safe to
  commit or sync.
- `{keychain:}` maps to macOS Keychain / libsecret / Windows Credential Manager.
  This is what the GUI's key form writes when the user types a key directly.
  **Integration, resolved — no upstream change.** tobor-kit's `LLMKeySpec`
  (`LLMInferenceConfig.swift`) has only `literal` and `.environment` cases and
  cannot round-trip a keychain spec. Adding a `.keychain` case upstream was the
  first instinct and is **rejected**: the Lit `LLMInferenceConfigData` carries
  the same two-case spec, and the kit's worth is that its Lit, Hologram and
  Swift mirrors agree. A Swift-only third case buys a local convenience at the
  cost of the property that makes the kit valuable.

  Instead the translation lives in our `LLMInferenceConfigStoring`
  implementation, and the view never learns about it:

  | direction | behaviour |
  |---|---|
  | **load** | read `{keychain: ref}` from YAML → fetch the secret from Keychain → hand the view a `.literal` holding the value, **in memory only** |
  | **save** | view returns `.literal` → write the value to Keychain under `ref` → persist `{keychain: ref}` to YAML |

  So `.literal` becomes an **in-process transport**, never a storage format. The
  secret never reaches the YAML file, the kit stays unmodified, mirror parity
  survives, and the view's contract is honoured exactly as written. The one rule
  this imposes: the store must never write a `.literal` through to disk, which
  is worth an explicit test.
- `{literal:}` is supported for CI and headless use but the GUI **never writes
  it** and warns when it reads one.
- Redaction is enforced in the core, not the UI, so the CLI benefits too.

### 4.2 `snippets` — the house-style library

The gap from §2.2. Named, reusable prompt fragments, composable into any prompt.

```yaml
snippets:
  house-style: |
    Clean, confident, understated. No stock-photo gloss, no lens flare,
    no gradient mesh. Prefer real materials and honest light.
  brand-voice: |
    Dry, precise, never breathless. Understate the claim.
  no-gemini-images: |
    Do not use Gemini or Imagen for image generation.
```

Resolution: snippets are composed in declared order and prepended to
`prompt.system` — the field that already exists for exactly this purpose — so no
schema break, and the CLI honours them with no flag. Prompt files may also name
snippets directly via a new optional `snippets: [name, ...]` key.

Snippets may live inline as above, or be `!include`-ed from
`~/.config/media-tool/snippets/*.md` so they can be version-controlled and shared
across a team.

### 4.3 `preferences` — the CSS-style cascade

The request was explicit: express preferences the way CSS does. So the model is
CSS's: **selectors with specificity, later-and-more-specific wins, each matching
layer patches rather than replaces.**

```yaml
preferences:
  "*":                          # universal          specificity 0
    service: openai
    quality: med
    snippets: [house-style]

  "type:image":                 # media type         specificity 1
    service: grok
    snippets: [house-style, no-gemini-images]

  "type:music":
    service: suno
    model: V6

  "type:svg":
    service: anthropic
    model: claude-opus-5

  "type:image[quality=high]":   # type + attribute   specificity 2
    service: gemini
    model: imagen-4-ultra

  "tag:brand":                  # tag                specificity 2
    snippets: [house-style, brand-voice]

  "id:hero-shot":               # id                 specificity 3
    service: grok
```

**Resolution order (lowest → highest):**

| # | Layer | Example |
|---|---|---|
| 0 | `"*"` universal | every prompt |
| 1 | `type:<t>` | `type:image` |
| 2 | `type:<t>[attr=v]`, `tag:<t>` | `type:image[quality=high]` |
| 3 | `id:<id>` | `id:hero-shot` |
| 4 | **Explicit field in the `.media.prompt` file** | `service: grok` — the inline style, always wins |
| 5 | **CLI flag / GUI override for this run** | `--service grok` — `!important` |

Ties at equal specificity resolve by declaration order, later wins — again as in
CSS. Patching is per-field: a layer setting only `model` leaves an inherited
`service` intact.

**Why this shape.** It is not novelty for its own sake — tobor-kit's
`noizu-model-config` component already implements an `inherit` model where an
empty value means "inherit" with nearest-ancestor-wins resolution owned by the
host. The cascade above is the config-file expression of a resolution rule the
component library already speaks, which is what lets the settings UI in §5.2 be
assembled from stock parts rather than invented.

**Debuggability is a requirement, not a nicety.** A cascade you cannot inspect
is a cascade that wastes paid API calls on a provider you did not expect. Both
surfaces must be able to explain a resolution:

```
$ generate-media-prompt explain media/hero.media.prompt
service: grok
  "*"              service: openai     (overridden)
  "type:image"     service: grok       ← winner
  file             service: —
model:   imagen-4-ultra
  "type:image[quality=high]"           ← winner
snippets: [house-style, no-gemini-images]
```

The GUI shows the same trace as a provenance popover next to each resolved field.

### 4.4 `types` — the open type registry

**Design goal: hundreds of output types, most of them text formats written by a
chat LLM.** The good news from the stock-take is that this is far closer than it
looks, because `type` and `format` are already open strings and the chat write
path is already format-agnostic.

#### What already works today — no code change

This produces a LaTeX file right now, at `0722c42`:

```yaml
schema: "0.4"
id: paper-draft
type: document              # any chat type
service: anthropic          # pinning a chat provider is the load-bearing bit
output:
  formats: [{format: tex}]  # used verbatim as the extension
  text_format: latex        # drives prep + FIM guidance
```

The same shape already yields `csv`, `ics`, `gcode`, `sql`, `dockerfile`, `rss`,
`abc`, `musicxml`, `json-ld`, `typst`, `lilypond` — **the long tail is reachable
today**, it is merely undiscoverable and requires pinning `service:` by hand.

**SVG is fully supported, with a shipped demo** (`demos/svg/sample-icon.media.prompt`):
`type: image` + `service: gemini-chat` + `format: svg` + `text_format: svg`,
with a dedicated repair arm in `sanitize_chat_output` (`providers/mod.rs:377-395`)
and `validate_svg` xmllint + LLM auto-repair (`validate.rs:29-113`). A second
route renders SVG from `type: diagram` via `post_processing` (`output_format`
defaults to `"svg"`, `pipeline.rs:409-412`).

#### Modalities — the closed set (code)

| modality | engine today | examples |
|---|---|---|
| `text` | chat provider → `sanitize_chat_output` → `fs::write` | svg, latex, csv, ics, gcode, sql, rss, abc, musicxml |
| `media` | media API returns binary | image, video, music, voice, sfx |
| `render` | `text`, then `post_processing.render` via `renderers::get_renderer` | mermaid→png, plantuml→svg, graphviz→svg |
| `compose` | DAG (`depends_on`, `as`/`collapse`) | style-guide, react-page |

Types are open; modalities are not. Four engines cover an unbounded type space.

#### Registry entry (YAML — no rebuild per type)

Follows the precedent already set by FIM's on-disk `solution/<text_format>.md`
lookup (`src/fim.rs:125-128`).

```yaml
types:
  gcode:
    modality: text
    extension: gcode
    text_format: gcode
    default_service: anthropic
    system: "Emit only valid RS-274 G-code. Absolute coords. No commentary."
    validate: []

  midi:
    modality: render        # text first, then convert
    via: abc                # LLM writes ABC notation
    renderer: abc2midi      # NEW renderer — see below
    extension: mid
```

Builtin types ship in the package as `types.d/*.yaml`; user types live in
`~/.config/media-tool/types.d/` and merge over them, so house and community
types are added without touching the binary. A new type is ~8 lines of YAML and
zero Rust.

#### MIDI — corrected assessment

MIDI is **not** reachable today (zero `midi`/`.mid` references in `src/`,
`media-tool.yaml` or `docs/`; no provider emits binary MIDI; no renderer
converts to it). But it does **not** need a "MIDI provider". An LLM writes ABC
notation or MusicXML as *text* — which works today — and a converter turns it
into `.mid`. That is exactly the pattern `mermaid` already uses (`mmdc`:
text → png).

So MIDI costs **one new renderer** (`abc2midi`, or `lilypond` which also gives
`.pdf` and `.ly`), registered alongside the existing four in
`renderers/mod.rs:22-30` with the same `is_available()` probe. That single
renderer unlocks a family: abc→midi, musicxml→midi, lilypond→midi/pdf. FIM
solution docs for `abc`, `lilypond`, `musicxml`, `vexflow` already exist in
`skill/content-media-engine/references/fim/solution/`.

#### The seven choke points

Making the long tail work *ergonomically* — without pinning `service:`, with
correct prep and eval — is a whitelist problem, not a rewrite. Each is small:

| # | File:line | Problem | Fix |
|---|---|---|---|
| 1 | `schema.rs:356` | unknown type → `AssetType::Unknown` dead end | map to a generic text type, or let `is_chat_type()` consider `text_format` |
| 2 | `providers/mod.rs:254-257` | **`Unknown` dispatches to a hardcoded Gemini _image_ model** | must be a chat ladder |
| 3 | `pipeline.rs:1058-1071` | `_ => asset_type == Image` rejects unknown types as unsupported | accept text types |
| 4 | `providers/mod.rs:242-252` | chat tier is one hardcoded `groq-chat` candidate for all qualities | add `chat_tiers` to `ProviderConfig` mirroring the existing `image_tiers` |
| 5 | `pipeline.rs:626-645` | `effective_text_format` extension whitelist | registry-driven |
| 6 | `prep.rs:55-96` | prep channel routing whitelist | registry-driven |
| 7 | `eval.rs:360-412` | unknown extensions are "un-scorable", structural only | registry declares an eval hint |

Choke point **2** is the worst: an unrecognised type silently tries to generate
an *image*. Fixing 1–4 alone unlocks the generic long tail; 5–7 make it good.

**Config extensibility caveat:** `provider_config.rs:24-46` exposes `image_tiers`
only — the image ladder is swappable without a rebuild, but chat/audio/video
ladders and the `service → impl` maps are compiled. New *providers* need a
rebuild; new *types* (after the above) do not.

---

## 5. Application design

### 5.0 The central object: a **Run**

Everything the app shows hangs off one durable record the CLI never kept:

```
Run
├── prompt          the .media.prompt at time of run (snapshot, not a pointer)
├── resolved        cascade output + provenance trace  (§4.3)
├── composed        final text after snippets applied  (§4.2)
├── provider/model  what was actually called
├── attempts[]      each: artifact, grade, duration, cost, error
└── lineage         parent run if this was a refine
```

Runs are append-only and live in `~/.local/share/media-tool/runs/`. This makes
the two things the CLI cannot do today possible: **compare candidates** and
**see why a prompt got better or worse**.

### 5.1 State matrix — written before layout

Per screen, the states that must be designed. The gap that sinks most tools of
this kind is that only the happy path gets drawn.

| Screen | empty | loading | error | degraded | long-running |
|---|---|---|---|---|---|
| Library | "no prompts yet" + create / open folder | dir scan skeleton | unreadable dir | — | — |
| Editor | new-prompt template | — | YAML parse error w/ line | schema < 0.4 notice | — |
| Compose review | — | resolving cascade | unknown snippet name | snippet missing → named, non-fatal | — |
| Generate | — | **per-attempt progress + elapsed + cancel** | provider 4xx/5xx, quota, timeout | renderer unavailable → type disabled w/ install hint | **yes — minutes; must survive window close** |
| Results | "no attempts" | artifact decode | artifact unreadable | no preview for type → metadata + reveal-in-finder | — |
| Grade | "no eval block" + offer to add | scoring in flight | grader LLM failed | `mode: structural` only (no vision) | — |
| Settings | no keys → first-run guidance | — | key test failed (`test-complete`) | key present but untested | — |

Two hard rules:

- **A generation in flight must survive the window closing.** Work runs in the
  core, not the view; the UI reattaches to a handle. Paid API calls must never
  be lost to a UI event.
- **Every provider error shows the provider's own message**, not a generic
  "something went wrong". Debugging a 400 from Suno requires Suno's words.

### 5.2 Screens

**Shell:** sidebar + main. Sidebar = Library / Runs / Settings. No tab bar, no
command palette in v1 — this is a tool used deliberately, not at speed.

1. **Library** — prompt files, grouped by directory, with per-prompt last-run
   status dot (never run / passed / failed / stale). Stale = prompt edited since
   its newest run.

2. **Editor** — YAML on the left, live-resolved preview on the right. The
   preview is the *composed* prompt (snippets + cascade applied), which is the
   thing actually sent and the thing people most often get wrong.

3. **Author-assist** ("prompting to create prompts") — a panel, **not a chat
   window**. Input: an intent sentence + target type. Output: a proposed
   `.media.prompt` shown as a **diff against the current file**, with
   accept / accept-partial / reject. Rationale: the user is editing a document,
   not conversing; a diff is reviewable and a chat transcript is not. Uses the
   `refine_model` already configured in `media-tool.yaml`.

4. **Generate** — resolved provider/model with a provenance popover (§4.3),
   attempt count, estimated cost if known, and a prominent **Cancel**.

5. **Results** — candidate grid. Side-by-side compare for images/video,
   waveform + transport for audio, source view for svg/html/diagram. Each
   candidate shows its grade badge.

6. **Grade** — the `eval` block made legible: per-criterion weighted score,
   which `required_pass` criteria failed, which `reject_if` triggered, and the
   grader's stated reasoning. Scores are **explanations, not verdicts** — the
   user can override a grade and record why.

7. **Revise** — the refine loop with lineage. Shows parent → child prompts as a
   diff, each with its grade, so it is visible whether a revision helped. This
   is the loop `src/refine.rs` already runs, given memory and a UI.

8. **Settings** — three tabs:
   - **Keys**: one `LLMInferenceSettingsView` per provider. The kit component is
     single-provider by design, so the multi-provider matrix is composition, not
     a kit change. Verified Swift contract:

     ```swift
     LLMInferenceSettingsView(
       config: Binding<LLMInferenceConfig>,
       catalog: [LLMProvider] = LLMProvider.catalog,   // injectable ✓
       transport: any HTTPTransporting = URLSessionTransport(),
       environment: [String: String] = ProcessInfo.processInfo.environment)
     ```

     `catalog:` is a plain default argument, so we pass a **media-provider
     catalog** (our 16) in place of the built-in 9-entry chat catalog — no
     upstream change needed. The view's only binding is `config`; **the host
     owns persistence** and the view never saves, so the app loads on appear and
     writes on change.

     Consumption: SwiftPM by local path — `.package(path: "../../Libs/tobor-kit")`,
     product `ToborKitUI` (package name is `ToborKit` though the directory is
     `tobor-kit`). **Do not pin the `v0.1.0` tag**: it predates most of the Swift
     work. Swift tools 6.0, `.swiftLanguageMode(.v6)`, **macOS 14.0 minimum** —
     which sets the app's deployment target.
   - **Preferences**: the §4.3 cascade, as an ordered rule list with a live
     "resolve a sample prompt" pane. Editing rules without seeing their effect
     is how people misconfigure cascades.
   - **Snippets**: the house-style library, plain text editing.

### 5.3 Capabilities panel (detect + degrade)

`is_available()` already exists on every renderer. Extend the same probe to the
other shelled-out binaries the recon found — `ffprobe`/`ffmpeg`
(`src/structural.rs`, `src/eval.rs`), `xmllint` (`src/validate.rs`),
`rsvg-convert` and ImageMagick `convert` (`src/eval.rs`) — and surface all of it
in one place:

```
Renderers          Probes
✓ mermaid   mmdc   ✓ ffprobe     media duration/streams
✗ graphviz  dot    ✓ ffmpeg      structural eval
  → brew install graphviz
✓ plantuml  java   ✗ rsvg-convert  svg rasterize for visual eval
✗ puppeteer node     → brew install librsvg
  → npm i -g puppeteer
```

Unavailable tooling **disables the affected media types with an install hint**
rather than failing at generation time after the user has written a prompt.

---

## 6. Packaging — `make release`

Today: no `release`/`package`/`dist` target, **no Rust build in CI at all**, no
cross-compilation config, and media-tool is not in the `Portfolio/Utilities`
harness. All of this is greenfield.

### 6.1 Package contents (identical on all three platforms)

```
Media Workbench
├── Media Workbench.app / mediaworkbench.exe / usr/bin/media-workbench
├── generate-media-prompt              → PATH
├── snippets/house-style.md            starter library
├── media-tool.yaml.sample             commented default config
└── docs/  eval-criteria-catalog.md, providers.md, howto/
```

First launch seeds `~/.config/media-tool/media-tool.yaml` from the sample **only
if absent** — never overwrite a user's config.

### 6.2 Targets

| Platform | Arch | Artifact |
|---|---|---|
| macOS | universal (`lipo` x86_64 + aarch64) | `.app` → codesign → notarize → `.dmg` |
| Linux | x86_64, aarch64 | `.deb` + `AppImage` |
| Windows | x86_64 | `.msi` (WiX) |

### 6.3 Make targets

```make
release            # all platforms the host can produce, into dist/
release-macos      # universal .app + .dmg (requires macOS host for notarization)
release-linux      # .deb + AppImage, via cross or container
release-windows    # .msi
release-cli        # CLI-only tarballs per triple (no GUI)
dist-clean
```

`release` composes the per-platform targets and writes `dist/` plus a
`SHA256SUMS`. Since `reqwest` is already `rustls-tls` with no OpenSSL, cross
builds need no C toolchain gymnastics for the network path.

### 6.4 CI

A **new** workflow (the existing `ci.yml` is site-only and must not be disturbed):
`cargo test` + `cargo clippy` on PRs, and a tag-triggered build matrix producing
a GitHub Release with all artifacts attached. macOS signing/notarization
credentials come from repo secrets; **there is no signing story today and one is
required before distributing a `.app`** — Gatekeeper will otherwise refuse it.

### 6.5 Harness registration

Add `source/media-tool` to `SUBDIRS` in `Portfolio/Utilities/Makefile` so
`make install-utilities` covers it, matching every other utility package.

---

## 7. Open questions

> Questions 1-3 were delegated to author discretion and are **resolved** below.
> The rationale is recorded because the reasoning, not the answer, is what a
> reviewer needs in order to disagree usefully.

1. **RESOLVED — converters: `abc2midi` and `pandoc` in phase 0c; lilypond
   deferred.** The expensive part is the `via:` intermediate contract, not any
   individual converter; the second converter is nearly free once the mechanism
   exists. So the two chosen each pay for a different reason. `abc2midi` is a
   tiny dependency and exercises the **hard** path — a binary artifact produced
   from a text intermediate — which is precisely the contract worth proving
   first, and it delivers MIDI. `pandoc` has the widest fan-out per binary of
   any converter available: docx, epub, odt, rst, org, asciidoc, mediawiki and
   more, from one dependency and one probe. **Lilypond is deferred**: ~200 MB,
   finicky, and redundant for MIDI now that `abc2midi` covers it — it earns a
   slot only when engraved `.pdf` scores are actually wanted. `latexmk` follows
   the same rule and waits for a real LaTeX→PDF need.

2. **RESOLVED — the registry is tiered, not sized.** "~40 types" was the wrong
   shape of answer: it implies forty equally-supported things, when the real
   per-type cost is the **system prompt**, and forty hand-written ones would
   mostly be mediocre. Three tiers instead, with honest labels:

   | tier | count | contents | support |
   |---|---|---|---|
   | **curated** | ~15 | hand-written system prompt, validator, eval criteria | first-class; these are the types we use daily |
   | **declared** | ~60 | extension + mime + modality only; generic text handling | works, but unpolished — labelled as such in the UI and `--list-types` |
   | **user** | unbounded | `~/.config/media-tool/types.d/` | whatever the user writes |

   Curated v1: `svg, mermaid, plantuml, graphviz, latex, typst, html, markdown,
   csv, json, yaml, sql, dockerfile, abc, musicxml`. Everything else starts
   declared and gets promoted when someone cares enough to write its prompt.
   This keeps the type count large and honest at the same time, and makes
   promotion a visible, low-ceremony act rather than a code change.

3. **RESOLVED — text-intermediate is the contract; binary-direct is not
   required.** Every reachable binary format goes text → converter, and the
   registry's `via:` field names the intermediate. This is accepted rather than
   waiting for providers that emit binary directly, for three reasons: it is
   the pattern already shipping (`mermaid` via `mmdc`); the intermediate is
   **inspectable and diffable**, so a bad `.mid` can be debugged by reading its
   ABC rather than hex-dumping a binary; and it means a revision loop can
   operate on text the LLM can actually re-edit. Providers that emit binary
   directly remain welcome later as a `media`-modality addition — the registry
   accommodates both without a schema change.
4. **Linux toolkit** — GTK4 vs Qt. GTK4 is the lighter dependency; Qt has better
   Windows story if the two ever converge. Not urgent until platform two.
5. **Cost tracking.** Attempts record duration; should they record spend? Every
   provider reports it differently and some not at all.
6. **Run store growth.** Generated media is large. Retention policy — cap by
   size, by age, or prune-on-demand?
7. **Apple Developer ID** — does one exist for signing, or is that a
   prerequisite to acquire?

---

## 8. Phasing

| Phase | Deliverable | Gate |
|---|---|---|
| **0a** | Type-system unblock: choke points 1-4 (§4.4) — `Unknown` → chat not image, config-driven `chat_tiers` | an unregistered text type generates without pinning `service:` |
| **0b** | `[lib]` target, config additions (`keys`, `snippets`, `preferences`, `types`), cascade resolver + `explain` subcommand | CLI honours cascade; `explain` correct; tests green |
| **0c** | Type registry + `types.d/`, choke points 5-7, first converter renderer (`abc2midi` or `lilypond`) | MIDI generates end-to-end from an ABC intermediate |
| **1** | UniFFI annotations + generated Swift bindings, `session`/Run store | a Swift test harness round-trips a run, streams progress, and cancels mid-generation |
| **2** | macOS app: Library, Editor, Compose review, Settings | Keys work end-to-end; cascade visible |
| **3** | macOS app: Generate, Results, Grade, Revise | Full loop on one real prompt, survives window close |
| **4** | `make release-macos` + signing + notarization + CI | Installable `.dmg` on a clean machine |
| **5** | Linux, then Windows | C ABI unchanged for one release first |

Phase 0 ships value with **no GUI at all**. The cascade, snippets, `explain`
and the open type registry make the CLI materially better on their own, and they
de-risk everything above by settling the config contract before any UI depends
on it. Phase 0a in particular is the highest-value work in this document: four
small edits turn an unrecognised type from "silently tries to generate an image"
into "works", which is the difference between a dozen types and an open-ended
long tail.

---

Co-Authored-By: Loom <loom@therobotlives.com>
