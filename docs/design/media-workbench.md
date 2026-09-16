# Media Workbench — Desktop App Design

Status: **draft for review** · Author: Loom · Date: 2026-09-16
Scope: a cross-platform desktop application for end-to-end oversight of media
generation, built on the existing `generate-media-prompt` Rust CLI.

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

**Media types today:** `image`, `audio`, `voice`, `music`, `sfx`, `video`,
`component`, `react-page`, `html`, `style-guide`, `diagram`, `document`.

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
6. **No SVG or MIDI type.** Both were named in the request; neither exists.
   See §7.

---

## 3. Architecture

**Decision: native UI per platform, shared Rust core, GUI links the crate.**

```
┌─────────────────────────────────────────────────────────────┐
│  macOS: SwiftUI          Linux: GTK4        Windows: WinUI 3 │  ← thin, pixels only
│  + tobor-kit Swift pods                                      │
└───────────────┬─────────────────┬──────────────┬─────────────┘
                │                 │              │
                └────────── C ABI (media-tool-ffi) ────────────┘
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
| New crate `media-tool-ffi` → `staticlib` + `cdylib` | C ABI for Swift/GTK/WinUI |
| Extract `main.rs` orchestration into library functions | CLI becomes a thin caller, same as the GUI |
| Long-running ops behind a handle + poll/callback API | Generation takes minutes; UI must stay live |
| `snippets` module | §4.2 |
| `preferences` cascade resolver | §4.3 |
| `session` module (run history, candidates, grades) | GUI needs durable history the CLI never kept |

`reqwest` is already `rustls-tls` with `default-features = false` — no OpenSSL —
so cross-compilation is unusually clean. `[profile.release]` already sets
`strip = true`, `lto = true`.

### 3.2 Platform sequencing

macOS ships first (it is the platform actually asked for, and tobor-kit already
has Swift mirrors). Linux and Windows follow once the FFI surface has stopped
moving. **Do not start platform two until the macOS app has shipped and the C ABI
has been stable for a release** — otherwise three UIs chase a moving boundary.

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
   - **Keys**: one `tobor-llm-inference` instance per provider. The kit
     component is single-provider by design, so the multi-provider matrix is
     composition, not a kit change. The component takes a `catalog` prop, so we
     pass a **media-provider catalog** (our 16) instead of the default 9-entry
     chat catalog — no upstream modification required. Its `test-complete`
     event drives the per-provider status badge.
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

1. **SVG and MIDI are not media types today.** Both were named in the request.
   `svg` is close to existing `image`/`diagram` handling and is cheap to add as
   a first-class type. **`midi` has no provider and no renderer at all** — it
   needs a generator decided before it can be more than a cascade key. The
   cascade in §4.3 accepts `type:svg` and `type:midi` as selectors regardless,
   so config can be written ahead of support, but generation will fail until
   the types exist. *Which of these is actually wanted in v1?*
2. **Linux toolkit** — GTK4 vs Qt. GTK4 is the lighter dependency; Qt has better
   Windows story if the two ever converge. Not urgent until platform two.
3. **Cost tracking.** Attempts record duration; should they record spend? Every
   provider reports it differently and some not at all.
4. **Run store growth.** Generated media is large. Retention policy — cap by
   size, by age, or prune-on-demand?
5. **Apple Developer ID** — does one exist for signing, or is that a
   prerequisite to acquire?

---

## 8. Phasing

| Phase | Deliverable | Gate |
|---|---|---|
| **0** | `[lib]` target, config additions (`keys`, `snippets`, `preferences`), cascade resolver + `explain` subcommand | CLI honours cascade; `explain` correct; tests green |
| **1** | `media-tool-ffi` C ABI, `session`/Run store | FFI stable, round-trips from a Swift test harness |
| **2** | macOS app: Library, Editor, Compose review, Settings | Keys work end-to-end; cascade visible |
| **3** | macOS app: Generate, Results, Grade, Revise | Full loop on one real prompt, survives window close |
| **4** | `make release-macos` + signing + notarization + CI | Installable `.dmg` on a clean machine |
| **5** | Linux, then Windows | C ABI unchanged for one release first |

Phase 0 ships value with **no GUI at all** — the cascade, snippets and `explain`
make the CLI materially better on their own, and they de-risk everything above
by settling the config contract before any UI depends on it.

---

Co-Authored-By: Loom <loom@therobotlives.com>
