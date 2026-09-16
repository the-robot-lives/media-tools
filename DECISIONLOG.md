# Decision Log — media-tool & Media Workbench

Architectural decisions, why they were made, and what would reverse them.
Newest first. One entry per decision that constrains future work.

Format: **Decision** · *Date* · Status · Context → Choice → Consequence → Reversal trigger.

---

## 2026-09-16 — Provenance is part of the cascade resolver's return type

**Status:** decided, in implementation (`feat/pref-cascade`)

**Context.** The preference cascade (`*` < `type:` < `type:[attr]`/`tag:` < `id:` <
prompt-file field < CLI flag) resolves a service/model/quality for each prompt. The
obvious signature returns just the resolved values.

**Choice.** The resolver returns resolved values *and* provenance — which rule, at which
index, won each field — as a single non-optional result.

**Consequence.** The `explain` subcommand and the GUI's "why this provider?" affordance
are both readable off the same structure. Neither needs a second resolution pass, and
they cannot disagree with what generation actually does.

**Reversal trigger.** None expected. Note that retrofitting provenance into a resolver
that discarded it is a rewrite, not an addition — which is precisely why it is in the
signature from the start.

---

## 2026-09-16 — Bridge slice 1a exposes only `dry_run`, not real generation

**Status:** decided, in implementation (`feat/uniffi-bridge`)

**Context.** UniFFI's callback-interface async completion is its known sharp edge
(mozilla/uniffi-rs#2633). Progress callbacks crossing the FFI boundary are the riskiest
part of the bridge.

**Choice.** Slice 1a exports exactly `parse_prompt`, `list_prompts`, and
`dry_run(paths, observer)`. No real generation, no cancellation.

**Consequence.** The callback/async model gets exercised on work that is free and
completes in milliseconds. A hung future is immediately visible as a hang.

**Reversal trigger.** None — 1b (generation + cancellation) builds on this deliberately.
The ordering exists because on a minutes-long paid generation, a hung future and slow
work look identical.

---

## 2026-09-16 — Byte-identical CLI output is the primary regression check

**Status:** standing practice

**Context.** Installing a `tracing` subscriber caused color-eyre to attempt SpanTrace
capture; with no `tracing-error` ErrorLayer it appended
`Warning: SpanTrace capture is Unsupported` to every error report. Every test passed.

**Choice.** Any change touching output paths must be verified by diffing CLI output
byte-for-byte against a build of `develop` on representative prompts from `demos/`.

**Consequence.** Caught a regression no test would have. Fixed with
`HookBuilder::default().capture_span_trace_by_default(false)`.

**Corollary.** Nondeterministic output actively defeats this check — which is why the
`Options: {...}` HashMap ordering is being fixed (`fix/pipeline-determinism`) as
correctness work rather than cosmetics.

---

## 2026-09-16 — Two-target tracing: `media_tool::ui` vs `media_tool::progress`

**Status:** shipped (PR #16, `58689ec`)

**Context.** The CLI's output had to stay byte-identical while the GUI needed structured,
typed lifecycle events. One event stream could not serve both without one of them
degrading.

**Choice.** Two tracing targets. `media_tool::ui` carries `ui_kind` + a formatted message
for the terminal renderer. `media_tool::progress` carries named events with typed fields
and no prose, for the GUI and test harness.

**Consequence.** 307 call sites converted. `eval.criterion{criterion, weight, score,
threshold, passed}` reaches consumers as five typed fields.

**Reversal trigger.** If the bridge ever flattens `progress` events into a JSON string
for convenience, the conversion bought nothing and Swift is back to parsing text. This is
an explicit merge gate on the bridge PR.

---

## 2026-09-16 — Open type registry over a closed modality enum

**Status:** decided; partially implemented

**Context.** Request was for "hundreds of allowed media types", many being text LLMs
emitting a specific output format. The instinct is a bigger enum.

**Choice.** `type` stays an open `String`; `format` stays an open `String` used verbatim
as the file extension. `AssetType` remains a small *closed* routing enum (text / media /
render / compose) that decides which pipeline handles a type — it is not the type list.

**Consequence.** New types need no code change. SVG already worked this way; MIDI needs a
renderer, not a schema change. Seven whitelist choke points were identified as the places
where openness leaks; 1, 2 and 4 are fixed (3 was dead code), 5–7 remain.

**Reversal trigger.** None. A closed type enum was the wrong shape for the requirement.

---

## 2026-09-16 — Darkroom direction adopted with an explicit divergence licence

**Status:** decided (`macos/design-direction/CHOSEN.md`, PR #13)

**Context.** Five generated UX directions; none was ideal as drawn. Darkroom was closest
directionally but its near-total absence of chrome would make an e2e review tool unusable.

**Choice.** Adopt Darkroom as directional, with `CHOSEN.md` recording verbatim: "The
mockup is a reference, not a specification… Do not treat the near-total absence of chrome
as a mandate."

**Consequence.** §5.4 diverges deliberately — a slim 28pt context bar, a left-aligned
candidate rail, a grade drawer that overlays rather than resizes.

---

## 2026-09-16 — UniFFI over a hand-rolled C ABI

**Status:** decided; supersedes the original §3

**Context.** §3 originally specced a hand-rolled C ABI for the Rust↔Swift boundary.

**Choice.** UniFFI 0.32.x, proc-macro only (no UDL file to drift).

**Consequence.** Native async via `#[uniffi::export(async_runtime = "tokio")]`, callback
interfaces for progress, typed error enums rather than stringly-typed failure.

**The decisive argument was ABI drift**, not callback plumbing — a hand-rolled boundary has
no compiler-checked contract between the Rust and Swift sides, so a signature change
compiles cleanly on both sides and fails at runtime.

---

## 2026-09-16 — Keychain access lives in the host app's store, not upstream in tobor-kit

**Status:** decided; an earlier contrary call was reversed

**Context.** I initially said contributing a `.keychain` case upstream to tobor-kit was
preferred, then reversed it.

**Choice.** Keychain handling stays in `MediaWorkbenchKit`'s config store.

**Consequence.** Lit's `LLMInferenceConfigData` carries the same two-case spec; a
Swift-only third case breaks mirror parity, which is the kit's entire value proposition.
Verification afterward found the kit *explicitly documents* delegating this to hosts
(`LLMInferenceConfig.swift:11,127`).

**Note.** The reversal was correct but should not have been needed — the mirror-parity
constraint was knowable before the first call.

---

## 2026-09-16 — Modality filtering added upstream to tobor-kit, all three mirrors

**Status:** shipped (tobor-kit PRs #52, #53)

**Context.** Media Workbench needs per-provider API key configuration filtered by
modality. The kit's `LLMInferenceSettingsView` had no such filter, and its Hologram mirror
had no host-catalog seam at all.

**Choice.** Add `LLMModality` (`.text/.image/.audio/.speech/.video`) to Lit, Hologram and
Swift mirrors; add `modality:` filter and `sectionBy:` to the settings view; give Hologram
`prop :providers, :list, default: []`.

**Consequence.** Contrast with the keychain decision: this one *is* mirror-symmetric, so
upstream is right. The test is parity, not convenience.

---

## Earlier — native-per-OS UI over a shared Rust UI core

**Status:** decided (design PR #8)

**Choice.** SwiftUI on macOS, native toolkits on Linux/Windows, over a shared
Rust-rendered UI.

**Consequence.** A `make release` bundling CLI + app + prompts per platform. More surface
to maintain; a genuinely native review tool on each platform, which matters for a tool
whose job is looking closely at images.

---

## Process decisions

### Parallel agents are fenced by file path, not by trust

Concurrent agents each receive an explicit list of paths owned by other agents, and are
instructed to **stop and report** rather than edit a fenced file. Rationale: a merge
conflict is recoverable; a silent semantic conflict between two agents editing the same
function is not.

### Agents verify against code, not against my summary of it

Briefs state known facts *and* point at the file to confirm them. Rationale: I twice
relayed a design-doc proposal as though it were shipped code (`base_url` overridability;
the claim that SVG was not a supported type). Making the agent check is more reliable than
my resolving to be more careful.

### Worktrees are cut from `origin/develop`, never bare `develop`

`git worktree add .claude/worktrees/<name> -b <branch> develop` resolves the **local**
`develop` ref. Local `develop` drifts behind the remote constantly — PRs merge on origin and
nothing fast-forwards the local checkout — so every worktree cut this way silently inherits a
stale base.

Briefs must say `origin/develop`, after an explicit `git fetch origin`.

**Why this is here:** on 2026-09-16 two agents were launched against a local `develop` that was
5 commits stale (ad33efa vs 58689ec). Their trees had no `src/telemetry.rs` and no
`src/term_layer.rs` — introduced by PR #16 — after both had been briefed to read exactly those
files. It surfaced only because a third, unrelated agent mentioned the stale ref as an
incidental footnote in its own report.

**The rebase is not the fix.** A clean rebase means no textual conflict; it does not mean the
code is correct. Work authored against a pre-refactor tree can call the replaced code path and
still rebase without conflict. That has already happened once in this repo: a conflict-free
rebase left 4 unconverted `ui::` call sites, making suno's rate-limit retries invisible to any
GUI, with no failing test. After any such rebase, every site the agent authored must be grepped
against the post-refactor API — and any byte-identical output diff must be retaken against the
current base, since a diff against the stale base proves nothing.

### Squash-merge artifacts are not orphaned work

`git branch --no-merged` lists branches that were squash-merged. The check for genuinely
unmerged work is `git diff origin/develop origin/<branch>` — an empty diff means it landed.
A survey of "orphan WIP" using branch merge-status was one-in-three wrong.

---

*Maintained by Loom. Append new decisions at the top of the dated section.*
