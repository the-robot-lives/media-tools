# Media Workbench v2: requests, judges, house style, connected accounts

Status: **APPROVED** — comps signed off by the user 2026-11-11 (Loom). Extends `docs/design/media-workbench.md` (v1, 2026-09-16) and does not replace it. The v1 decisions all stand: native SwiftUI, a shared Rust core over UniFFI, the Run as the central object, the §4.3 cascade, Darkroom on judging surfaces only, and the §5.1 state matrix discipline.

Sessions:
- `media-tool-osx-app-ux` (9b08f6c6)
- `media-tools-decision-judge` (012af663), which provides the judge engine
- `b2b-service-consumers` (7f4e67a5), which covers the account model

## 0. What v2 adds

| # | Capability | Why v1 doesn't cover it |
|---|---|---|
| A | **Request intake**: "I need a hero image for the pricing page", with no YAML | v1 assumes the user starts from a `.media.prompt`. Personas P-002, P-006 and P-008 never will. |
| B | **Prep + suitability**: an LLM drafts the prompt and the decision model checks it against the target type and provider before money is spent | v1 Author-assist is diff-only and has no pre-flight gate |
| C | **Decision-model grading**: Alibaba `decision-model-preview` by default, Jev optional, with probabilities and confidence | v1 Grade assumes an LLM's prose score. The judge now returns *distributions*, which deserve a real visual. |
| D | **House and project style layers**: copy voice, palettes, typography, element-construction rules and rubrics, composed in layers (mirrors codemoshiach's layered composites) | v1 snippets are flat text prepended to `prompt.system`. They are not structured, not layered, and not gradable. |
| E | **Connected accounts**: therobot.is (memory of what worked, plus style memories) and evalonomi.com (eval runs, traces, metrics) via per-user OAuth | Nothing in v1 leaves the machine |

## 1. Object model (additions to the v1 Run)

```
Workspace (a folder; one project)
├── Style stack        house → project → collection → prompt   (§3)
│   └── Layer          palette · type · voice · construction · rubric · snippets
├── Requests[]         intent, target type, audience, constraints, status
│   └── → Prompt       drafted .media.prompt (lineage: request id)
└── Runs[]             (v1) + suitability verdict + judge answers + style provenance
Account links          therobot.is (OAuth, PKCE) · evalonomi (OAuth, see §5 gap)
```

- A **Request** is the new entry point. It always produces an ordinary `.media.prompt` on disk, so the CLI stays the source of truth and nothing becomes GUI-only.
- Every judged attempt stores `{backend, model, request_id, per-criterion probabilities, layer provenance}`. That makes a grade explainable back to the style rule that generated the criterion.

## 2. Information architecture

The sidebar adds two sections to v1's Library / Runs / Settings: **Requests** and **Styles**. Accounts live in Settings.

```
┌ Sidebar ──────────┐
│ ◎ Requests      3 │  intake queue (new, drafting, ready, generating)
│ ▤ Library         │  v1
│ ▶ Runs       ●2   │  v1 + Instrument-Deck run rail (live progress)
│ ◈ Styles          │  NEW: house / project layers, rubrics, palettes
│ ─────────────     │
│ ⚙ Settings        │  Keys · Preferences · Judges · Accounts
└───────────────────┘
```

URL-as-state (deep links `mediaworkbench://`): `request/<id>`, `prompt/<path>`, `run/<id>/attempt/<n>`, `style/<layer>/<facet>`.

## 3. Screens

v1 screens 1-8 stand. New and changed screens:

**R1. Request intake** (sheet, `⌘N`). Conventional form, *not a chat window* (same rationale as v1 Author-assist).
- Fields: intent (one sentence), target type (picker grouped by modality, with capability-disabled types greyed out using the v1 §5.3 hint), destination/usage (e.g. "pricing page hero, 16:10, dark bg"), style stack (defaults to the project), optional reference attachments (drag in), budget/quality.
- Primary action: **Draft prompt**.

**R2. Request → Prepare** (document view). Left: the drafted `.media.prompt` as a diff against empty or against the existing file, with accept / partial / reject (v1 §5.2.3). Right: the **Suitability card**:
- The decision-model verdict is `ready | needs-revision | wrong-asset-type | provider-mismatch`. It shows the probability bar for all four outcomes, not just the winner.
- Specificity and constraint-fit scores, each as a 10-level distribution sparkline.
- Actions: **Revise with feedback** (the verdict feeds back into prep), **Change type/provider** (pre-filled when the verdict says mismatch), and **Generate anyway** (logged as an override, like grade overrides in v1).
- Provenance: which style layers were composed into the prompt (chips; click opens Styles at that layer).

**R3. Grade, v2** (Darkroom drawer, extends v1 §5.4).
- The v1 annotated plate stays. Each criterion row gains a **distribution strip**: 10 cells shaded by probability, with an expected-score tick. A confident 70 and a split 70 look different, and that difference is the judge's real signal.
- A provenance chip per criterion names its source: `house:palette`, `project:voice`, `prompt`.
- A judge chip shows the backend/model. If two judges ran (Phase 4 agreement mode), a disagreement marker appears.
- `reject_if` rows show P(triggered).

**S1. Styles** (conventional macOS; *not* Darkroom). Three-pane: layer stack | facet list | facet editor.
- **Layer stack** (left): `House` (global, ~/.config) → `Project` (workspace) → `Collection` (folder/tag) → `Prompt` (inline). Drag to reorder within a tier. Each layer has an enable toggle, an "inherits from" label and an unsaved dot.
- **Facets** (middle): Palette · Typography · Copy voice · Element construction · Imagery rules · Rubric · Snippets.
- **Editor** (right), one per facet:
  - *Palette*: swatches with role (canvas, ink, accent, …), contrast-pair checker (WCAG ratios live), "must use / must avoid" flags. These compile to rubric criteria plus prompt text.
  - *Copy voice*: tone sliders as labelled scales, do/don't example pairs, banned words.
  - *Element construction*: rules for buttons, cards, iconography stroke, corner radius, grid, shown as a checklist with examples.
  - *Rubric*: criteria table (weight, required, levels text, fail signals) with **override semantics per row**: `inherit | override | disable | append`.
- **Effective composite** (toggle, `⌘E`): the merged result for a sample prompt. Every row is tagged with its winning layer and has an overridden-by trail. This is the v1 `explain` popover, scaled up to a whole style.

**S2. Settings → Judges**. Backend picker (Alibaba default, Jev), model, key status with key spec (`env` / `keychain` / `dc`), base URL (Alibaba requires a workspace URL; empty is shown as a blocking state with guidance), and a **Test** button that runs a 1-question probe and shows latency plus the answer. Agreement mode (run both, flag disagreement) is off by default.

**S3. Settings → Accounts**.
- One row per service: status, account/org, scopes, last sync, Connect / Disconnect.
- Per-workspace routing: send runs to `my account` / `don't sync`.
- The app is a **per-user OAuth client** (the "linked" path from SERVICE-CONSUMERS §6). It is not a consumer key, because the desktop app is one person's tool.

### Spec fixes (2026-11-11)

- The Grade drawer is **380pt** wide (v1's was 320pt), with the distribution strip on its own line under each criterion.
- The suitability shortcut is **⌘⇧K**, not ⌘K — ⌘K is reserved for a future command palette. §6 updated to match.
- tobor-kit `LLMInferenceSettingsView` covers **Keys only**; the Judges and Accounts screens are plain SwiftUI `Form`.
- brand-voice and other copy criteria are gated by rubric `applies_to` and are only scored when the asset actually contains copy.

## 4. State matrix (new screens; v1 rows still apply)

| Screen | empty | loading | error | degraded | long-running |
|---|---|---|---|---|---|
| Requests | "Describe what you need" + 3 example intents | draft in flight (streaming diff) | prep LLM failed (provider text verbatim) | no prep profile → offer manual YAML | drafting ≤60s, cancellable `⌘.` |
| Suitability | not yet checked + Check | judging | judge 4xx/5xx verbatim | no judge configured → "unchecked" (never implies ready) | — |
| Grade v2 | ungraded + grade now | judging | judge failed, LLM fallback offered | image judged via observe→decide (badge "described") | — |
| Styles | no house style → starter template | composing preview | invalid YAML at layer X | layer references missing snippet/palette (named, non-fatal) | — |
| Judges | no backend → setup | test running | 401/404 verbatim | key present, base URL missing (Alibaba) | — |
| Accounts | not connected + why connect | OAuth in browser (waiting) | token expired, reconnect | connected, sync paused / offline queue n | first sync |

## 5. Accounts: what each connection is for

| Service | Use in the app | Server readiness |
|---|---|---|
| **therobot.is** | (a) recall: "what worked before for this project and type" when drafting; (b) archive: each promoted attempt writes a memory (prompt, grade, chosen provider, style layers); (c) optional: style layers synced as org memories | **Ready.** OAuth 2.1 with PKCE and public-client dynamic registration exists. The app uses a loopback redirect (`http://127.0.0.1:<port>/callback`) per RFC 8252. Scopes `memory:read memory:write`. |
| **evalonomi.com** | export each graded run as OTLP spans/logs (run → attempts → judge calls), plus scores. The evalonomi dashboards become the cross-project quality history. | **Decision 2026-11-11: build an OAuth 2.1 AS in evalonomi** — port therobot.is `oauth.ex` (DCR public clients, PKCE, loopback redirect). Org API tokens remain the interim path until it ships. |

Shared rules:
- Tokens go in the macOS Keychain (v1 §4.1 keychain spec).
- Sync is an offline-tolerant queue, so a run never blocks on network.
- Disconnecting never deletes remote data; the UI says so.
- The Run carries `synced_to[]`.

## 5b. Style layers: same contract as codemoshiach, with evalonomi as source of truth

There is no new house-style system here. codemoshiach's layered rubric (branch `epic.layered-eval`, spec `the-robot-reviews/docs/contracts/RUBRIC-SPEC.md` plus JSON Schemas `rubric-layer-ops.v1`, `rubric-composite.v1`, `taxonomy.v1`) is domain-neutral. **evalonomi owns storage, compose and the layer editor**, and codemoshiach is a consumer of it. media-tool becomes a second consumer.

- **Model (reused verbatim):**
  - A rubric `Doc` is a tree of `Node`s (`section | paragraph | clause | anchor_set | check`). An `anchor_set` is exactly 10 increasing `{id, label, score}`, which maps 1:1 onto the decision model's 10 score levels.
  - A `Layer` is `{scope_kind, scope_key, version, ops}`, where ops are `replace | amend | insert_* | append_child | delete | suppress`.
  - The `Composite` carries per-node **provenance** and a checksum.
- **Media scopes:** media-tool needs a **taxonomy v2** proposal, filed with evalonomi and not forked. It keeps the same precedence mechanics but uses media axes instead of code-shaped ones: `default < org (house) < org_brand < org_asset_type < project < collection < prompt`, plus applies-to axes `{modality, asset_type, surface/usage, audience}`.
- **Facets become rubric sections.** Palette, typography, copy voice, element construction and imagery rules are each a `section`, with `clause`s that render into the prep prompt and `anchor_set`/`check` nodes that the judge scores. So **one artifact both instructs the generator and grades the output**, and the provenance chip in Grade v2 is simply `Composite.provenance[node_id]`.
- **Repo-authored layer:** `.media/rubric.yaml`, the equivalent of `.trr/rubric.yaml`. It has `pins` (brand, asset types) and `ops` for the project layer, and it is PUT to evalonomi gated on checksum.
- **Offline and degraded behaviour:** same 5-rung ladder as codemoshiach.
  1. Fresh cache
  2. ETag revalidate
  3. Stale-while-revalidate
  4. Bundled fallback composite
  5. Local-only YAML layers composed by a **Rust port of the compose engine**
  
  Compose stays available offline in both the CLI and the app, and it is cross-checked against evalonomi by checksum.
- **The Styles screen (S1)** is therefore a native editor over evalonomi layers. The "effective composite" view is evalonomi's `preview`/`render`, or the local port when offline. It does not invent its own resolution rules.
- **Consequence for accounts:** the evalonomi connection is not optional plumbing. It is where house style lives, which makes Q1 (evalonomi OAuth) a blocker for S1 sync, though not for local layers.
- **Coordination:** contract changes need one PR per repo against the shared spec. The taxonomy v2 proposal goes to the `codemoshiach-layered-eval-plan` workstream before any implementation.

## 6. Keyboard (additions)

| key | action |
|---|---|
| `⌘N` | new request |
| `⌘↩` | draft / generate (context) |
| `⌘⇧K` | check suitability (⌘K is reserved for a future command palette) |
| `⌘E` | effective composite (Styles) |
| `⌘1`…`⌘5` | sidebar sections |

v1's Results keys are unchanged.

## 7. Mockup plan (nano banana via `.media.prompt`): GATE, approve before generating

**Model:** **Nano Banana 2** `gemini-3.1-flash-image` — the original nano banana (`gemini-2.5-flash-image`) is blocked by CLI policy, so the comps were generated with Nano Banana 2. The v1 directions used `gemini-3-pro-image`; nano banana is cheaper and fine for screen-level comps.

**Direction is locked (Darkroom weighting).** These are *screen comps*, not direction exploration.

The criteria include `depicts_the_product`, added because of the v1 lesson that beauty bias passed the wrong product.

| # | File (`macos/screens/`) | Surface | Treatment |
|---|---|---|---|
| 1 | `mw-screen-request-intake` | R1 sheet over Library | conventional |
| 2 | `mw-screen-prepare-suitability` | R2 diff + suitability card | conventional document |
| 3 | `mw-screen-results-grade-v2` | Results + Grade drawer with distribution strips | Darkroom |
| 4 | `mw-screen-styles` | S1 three-pane, palette facet | conventional |
| 5 | `mw-screen-styles-composite` | effective composite with provenance | conventional |
| 6 | `mw-screen-runs-rail` | Runs with the live queue rail | conventional + Instrument-Deck rail |
| 7 | `mw-screen-settings-accounts` | Accounts + Judges tabs | conventional (tobor-kit forms) |

Each prompt is graded by the new decision judge, which dogfoods Phase 1-2. They are then vision-checked by a human (me plus you), per the memory rule "always vision-check".

### Screen comps (generated and approved 2026-11-11)

Prompts and comps ship in `macos/screens/` — one `*.media.prompt` + `*.png` pair per screen. Conventional screens use **macOS light appearance**, and Darkroom tokens appear only in #3.

| File | Purpose |
|---|---|
| `mw-screen-request-intake.media.prompt` | R1 New Request sheet over Library: intent, type with a capability-disabled entry, usage, style chips, one "Draft prompt" action |
| `mw-screen-prepare-suitability.media.prompt` | R2 drafted-YAML diff with accept/partial/reject, plus a Suitability card showing the four-outcome probability bar, 10-cell strips, provenance chips and "Generate anyway" |
| `mw-screen-results-grade-v2.media.prompt` | R3 Darkroom results: hero, scored rail, Grade drawer with per-criterion distribution strips (one split), provenance chips and `reject_if` P values |
| `mw-screen-styles.media.prompt` | S1 three-pane Styles: House → Project → Collection → Prompt stack, facet list, palette editor with WCAG contrast pairs |
| `mw-screen-styles-composite.media.prompt` | S1 `⌘E` effective composite: rules tagged by winning layer, override trail, suppressed row, evalonomi checksum |
| `mw-screen-runs-rail.media.prompt` | Runs table plus a live queue rail with per-job progress and cancel. The rail borrows Instrument Deck's idea, not its vocabulary. |
| `mw-screen-settings-accounts.media.prompt` | S2 + S3 as two Settings windows side by side: therobot.is connected, evalonomi.com connect, and a Judges test result for Alibaba `decision-model-preview` |

### Comp review notes (2026-11-11)

The mockups are **reference, not spec** — the v1 divergence licence applies. Known comp warts, to fix in implementation rather than by regenerating:

- intake renders as a popover rather than a sheet;
- suitability truncates the wrong-type/mismatch segment labels, and red on the wrong-type segment is semantic misuse;
- the composite popover trail is inconsistent with its row;
- the Runs icon duplicates Requests;
- Judges shows a placeholder with a green check.

Lesson: the agent vision check returned a uniform 9/9/9 and missed real defects (leaked labels, gauge-not-distribution). Comps need strict per-element checks — a dogfood case for the decision judge.

GUI user stories US-101..US-127 (epic "Media Workbench (GUI)") are in `project-management/user-stories/`, appended to `index.yaml`.

## 8. Phasing (folds into v1 §8)

| Phase | Adds |
|---|---|
| 0e | style layer schema + resolver + `explain --style` (CLI first, as v1 0b2) |
| judge P2-P3 | decision grading + suitability (in flight, media-tools-decision-judge) |
| 2b | Requests + Prepare/Suitability screens |
| 3b | Grade v2 distribution strips + provenance |
| 3c | Styles screens |
| 3d | Accounts: therobot.is first; evalonomi after its OAuth decision |

## 9. Open questions (ledger media-tool-osx-app-ux)
- Q1 evalonomi auth for the desktop app — **RESOLVED 2026-11-11: build an OAuth 2.1 AS in evalonomi**, porting therobot.is `oauth.ex` (DCR public clients, PKCE, loopback redirect). See §5.
- Q2 mockup prompt set approval (gate) — **RESOLVED 2026-11-11: approved**; all 7 comps generated and shipped to `macos/screens/`.
- Q3 style-layer storage: YAML in repo vs synced memories — **RESOLVED 2026-11-11: evalonomi owns the store**, with the `.media/rubric.yaml` repo layer and an offline Rust compose port (§5b).

Context: service-consumer tenancy is merged on both servers (therobot.is #198, evalonomi #74) — see monorepo `docs/proposals/SERVICE-CONSUMERS.md`.
