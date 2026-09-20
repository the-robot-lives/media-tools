# Changelog — media-tool

Milestone-based changelog (this monorepo does not version components independently).
Tags follow `utilities-agent-media-tool/<milestone>`; pre-import milestones (m1–m3)
are tagged on the original standalone-repo lineage preserved by the subtree squash.

## [Unreleased]
### Fixed
- A prompt declaring `format: png` no longer ships JPEG bytes in a `.png` file. The gemini
  image provider returns JPEG whatever the prompt declares; the tool wrote those bytes to the
  declared path, reported generation success, and post-processing then failed with "cannot
  decode" — the only honest signal arrived a step too late. Image bytes are now identified by
  magic number and reconciled with the requested extension (`src/imagefmt.rs`): transcoded
  when possible, written under their true extension with a warning when not. Version
  0.2.5 -> 0.2.6. (2026-09-21)
- Post-processing decodes by content instead of by file extension, so a mislabelled file from
  any source is handled rather than reported as corrupt. (2026-09-21)

### Added
- `provider_options.base_url` overrides the gemini API root, which is how the new
  stubbed-provider test drives the real provider. (2026-09-21)

### Fixed
- The tool no longer panics while truncating text for display. Every preview of a string the
  tool does not control (eval notes, provider error bodies, raw LLM replies) used
  `&s[..s.len().min(n)]`, which indexes bytes and panics when the index lands inside a
  multibyte UTF-8 character: an eval note containing an ellipsis or CJK text took the whole
  run down while trying to log itself. New `src/text.rs` cuts at the last character boundary
  instead, and all 33 call sites now use it. Version 0.2.4 -> 0.2.5. (2026-09-18)

### Added
- `docs/providers.md`: a note that qwen content-filter refusals can be lexical. A brief saying
  "AV cart" was refused as adult content; "projector trolley" cleared it unchanged otherwise.
  (2026-09-18)

### Fixed
- Image-to-image qwen-image renders (reference images, which must use
  `multimodal-generation` because `text2image` accepts none) now retry a connection the
  server drops, three attempts by default via `MEDIA_QWEN_RETRIES`. Measured live: a single
  attempt was cut at 60.9s, and the same call completed at 120.9s once the second attempt
  went through. Version 0.2.3 -> 0.2.4. (2026-09-18)

### Changed
- **Correction to the 0.2.3 notes: the ~61s cut is not HTTP/2-specific.** curl pinned to
  `--http1.1` shows the same close as `Empty reply from server` at 61.1s (after a 200 at
  55.8s), and this crate cannot negotiate HTTP/2 at all, since the `h2` crate is not in its
  dependency graph, yet is cut at the same mark. The server closes a synchronous
  `multimodal-generation` connection at ~61s while one render takes 45-75s depending on load,
  so it is a coin toss no client setting can win. (2026-09-18)
- DashScope clients are pinned to HTTP/1.1 (`http1_only`). This is belt-and-braces on a build
  with no HTTP/2 support, so that enabling an unrelated feature later cannot silently put long
  renders back on h2. (2026-09-18)
- Default synchronous ceiling raised 300s -> 360s, above the longest render observed. (2026-09-18)

### Fixed
- qwen-image renders that take longer than about a minute no longer fail. The
  `multimodal-generation` endpoint is synchronous only on our accounts and has a hard ~61s
  connection ceiling that no client setting avoids: measured live, curl over HTTP/2 failed
  three for three at 61.5-61.7s and this crate (HTTP/1.1) failed at ~62s, while on an idle
  service the same prompt returned 200 at 46.8 / 53.7 / 57.0s. Render latency swings with
  load, so the sync route was a coin toss against the ceiling. qwen-image now defaults to
  the async-native `text2image/image-synthesis` route: submit with
  `X-DashScope-Async: enable`, poll `/api/v1/tasks/<id>`, download. The same heavy prompt
  completes in 9.6-14.9s. Version 0.2.2 -> 0.2.3. (2026-09-18)

### Changed
- **The default route changes the model id.** `text2image` rejects `qwen-image-3.0` with 400
  InvalidParameter, so the route maps the multimodal default onto `qwen-image`.
  `qwen-image-plus` is also available via `model:`. Pin `provider_options: {route: multimodal}`
  to stay on `qwen-image-3.0`, accepting the ~60s ceiling. (2026-09-18)
- Prompts carrying input images stay on `multimodal-generation` automatically; it is the only
  route that accepts them. (2026-09-18)
- `MEDIA_DEBUG=1` logs the HTTP client configuration (timeouts, keepalive, HTTP/2 off) at
  request time. (2026-09-18)

### Fixed
- qwen-image runs no longer die at ~61s with a bare "Network error calling Qwen Image:
  error sending request". The provider built a bare `reqwest::Client::new()` and set only a
  per-request timeout, leaving connection-level defaults (idle-pool retirement, no TCP
  keepalive) to cut a silent wait long before the nominal 180s. New
  `src/providers/http.rs` builds clients with an explicit connect timeout, idle-pool
  retirement disabled and TCP keepalive on. Version 0.2.0 -> 0.2.2. (2026-09-18)

### Changed
- qwen-image supports DashScope **async task mode** (`X-DashScope-Async: enable` plus task
  polling), as an opt-in via `provider_options: {async: true}` or `MEDIA_QWEN_ASYNC=1`. It
  is not the default: the multimodal-generation endpoint answers HTTP 403 `AccessDenied`,
  "current user api does not support asynchronous calls", on the accounts we use, so
  defaulting it on would make every render fail. When async is enabled and rejected, the
  provider warns and retries synchronously; a genuine bad-key 403 still fails loudly. A
  deployment that answers inline is still handled. Tunables:
  `MEDIA_QWEN_TIMEOUT_SECS` (sync ceiling, default 300), `MEDIA_QWEN_POLL_SECS` (default 5),
  `MEDIA_QWEN_POLL_ATTEMPTS` (default 120). (2026-09-18)
- `provider_options.base_url` overrides DashScope plan/region routing (used by the new
  stub-server tests). (2026-09-18)

### Added
- `post_processing` actions `crop` and `resize` are implemented (`src/postprocess.rs`):
  gravity-anchored aspect crop, and `cover`/`contain`/`fill` resize, applied in place to
  PNG/JPEG/WebP/GIF outputs. Version bumped 0.1.0 → 0.2.0. (2026-09-18)

### Changed
- An unimplemented `post_processing` action now warns and fails the run (non-zero exit)
  instead of printing an informational "not yet implemented" line and exiting 0. The new
  `--allow-unimplemented-post` flag restores the lenient behaviour. (2026-09-18)

- Added `docs/PROJ-FAQ.md` + summary (motivation/fit/comparison/capability/caveats/trust Q&A) (2026-07-17)
- Added `docs/PROJ-HOWTO.md` + summary and `docs/howto/` (first-hour, FIM rich-format usage, common-error troubleshooting) (2026-07-17)
- Added `docs/PROJ-ARCH.md` + summary; refreshed `docs/PROJ-LAYOUT.md` and `docs/layout/src.md` (2026-07-16)

## [m6-fim-solution-library] — 2026-07-09 — tag: `utilities-agent-media-tool/m6-fim-solution-library`
Milestone summary: built out the FIM ("fill-in-the-middle") solution library — a
catalog of ~190 LLM-emittable media formats (diagram DSLs, canvas/WebGL/3D engines,
math/scientific rendering, music notation, audio synthesis, circuit/EDA tools)
with per-solution reference guides and eval scenario suites, wired into the CLI.

### Added
- `src/fim.rs` FIM subsystem plus `main.rs`/`pipeline.rs`/`prep.rs` integration
- `skill/content-media-engine/references/fim/` — categorized solution index, overview, inventory, per-solution guides
- Solution walkthrough docs (getting-started, limerick, sonnet, marketing-copy)
- 4-scenario eval `.media.prompt` suites (canonical / stress / communication / integration) per solution — several hundred prompt files
- Committed eval-run artifacts (rendered diag/svg/png/pdf outputs + metadata) for diagram solutions

### Changed
- Per-solution reference docs substantially expanded (~37k insertions across the milestone)
- README / HOW-TO updated for FIM usage

## [m5-quality-eval-gating] — 2026-06-14 — tag: `utilities-agent-media-tool/m5-quality-eval-gating`
Milestone summary: schema v0.4 — authors declare intent (type, quality tier) and
acceptance criteria; the tool owns provider choice, grades outputs against the
prompt's `eval` block via a hosted LM Studio model, and falls back across
candidate providers until one passes.

### Added
- `src/prep.rs` (prompt preparation) and `src/validate.rs` (prompt validation)
- `docs/quality-selection-and-eval.md` design doc
- `bin/media-eval-port-forward` helper for the eval-model endpoint

### Changed
- `pipeline.rs` overhauled for quality-based provider selection with eval-gated fallback
- `eval.rs` expanded (~5x) to grade outputs against declared criteria
- `schema.rs` upgraded to `.media.prompt` schema v0.4; Suno and provider registry reworked
- README, HOW-TO, demos, and skill doc updated to schema v0.4

## [m4-monorepo-import] — 2026-06-13 — tag: `utilities-agent-media-tool/m4-monorepo-import`
Milestone summary: standalone media-tool repo imported into the Noizu infra
monorepo as a squashed git subtree at `utilities/agent/media-tool/`.

### Changed
- Project root moved from standalone repo to `utilities/agent/media-tool/`; prior history preserved behind the subtree squash commit

## [m3-project-management-scaffold] — 2026-06-01 — tag: `utilities-agent-media-tool/m3-project-management-scaffold`
Milestone summary: product-planning corpus and layout docs added on top of the
working tool.

### Added
- `project-management/` — 8 user personas and 100 user stories (US-001…US-100)
- `docs/PROJ-LAYOUT.md` + summary and `docs/layout/src.md`

### Changed
- Makefile tweaks

## [m2-rust-rewrite] — 2026-05-27 — tag: `utilities-agent-media-tool/m2-rust-rewrite`
Milestone summary: the Python prototype was rewritten as a Rust CLI with a full
generation pipeline — 12 providers, local renderers, refinement loop, eval hooks —
plus demos across every supported media type.

### Added
- Rust crate: `pipeline.rs`, `schema.rs`, `refine.rs`, `eval.rs`, `dag.rs`, `attachments.rs`, `output.rs`, `ui.rs`
- Providers: Anthropic, OpenAI (chat + TTS), Gemini (image + chat), ElevenLabs, Qwen TTS, Suno, Veo, Grok video, Z.ai
- Renderers: mermaid, plantuml, graphviz, puppeteer
- `HOW-TO.md`; demo `.media.prompt` suites for image, svg, diagram, html, game, music, voice, video
- MIT-style `LICENSE`

### Changed
- Demo prompts/outputs iterated; stale generated artifacts pruned

## [m1-python-prototype] — 2026-05-26 — tag: `utilities-agent-media-tool/m1-python-prototype`
Milestone summary: initial proof of concept — a Python prompt engine and CLI that
turn declarative `.media.prompt` files into generated media, with a researched
provider catalog.

### Added
- `lib/media-prompt-engine.py` (~1.2k lines) and `bin/generate-media-prompt` CLI
- `docs/providers.md` provider catalog (heavily expanded same day)
- README, Makefile, first test `.media.prompt` fixtures
