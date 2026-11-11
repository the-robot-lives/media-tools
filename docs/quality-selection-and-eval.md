# Quality-Based Provider Selection & Eval-Gated Generation (schema v0.4)

Status: approved design — implementation in progress (2026-06-13)

## Motivation

Prompt authors should declare *intent* (asset type, quality tier, duration) and
*acceptance criteria* (eval block), not provider plumbing. The tool owns provider
choice, tries candidates in preference order, grades each output against the
prompt's `eval` section using a hosted Qwen 3.6 model (LM Studio, OpenAI-compatible),
and falls back to the next provider until the output passes or candidates are
exhausted (best-scoring output is then kept, with a warning).

## Schema v0.4 changes (`.media.prompt`)

```yaml
schema: "0.4"
id: hero-001
type: image            # image | music | voice | audio | video | component | react-page |
                       # html | style-guide | diagram | document
                       # NEW: music, voice (replace ambiguous audio+service combos;
                       # bare "audio" with no service is treated as voice)
quality: medium        # NEW: low | medium | high (default: medium)
# service: gemini      # NOW OPTIONAL — explicit service pins provider (skips auto-selection)
# model: imagen-4.0-generate-001   # still optional, only meaningful with service

prompt:
  text: "..."

output:
  duration: 8          # NEW: seconds, for video/music/voice (alias: length)
  formats: [{format: png}]
  dimensions: {aspect_ratio: "16:9"}

eval:                  # drives grading + provider fallback
  judge: decision            # decision (System One judge) | llm (legacy chat grader)
                             # default: decision when a judge backend resolves, else llm
  pass_threshold: 0.75       # weighted normalized score in [0,1]; default 0.7
  max_attempts: 3            # max provider candidates to try; default: all available
  required_pass: [readability]   # these criteria must individually reach pass_threshold
  criteria:
    composition: {weight: 2, description: "Subject centered, rule of thirds"}
    readability: {weight: 3, description: "Title text crisp and legible",
                  fail_signals: ["garbled text", "missing title"]}
  reject_if:
    - "watermark or signature visible"
    - "extra limbs or malformed anatomy"
```

Back-compat: v0.3 files keep working — `service` present means pinned provider,
absent means auto-select (v0.3's implicit gemini default is dropped; type drives
selection). `quality` absent → `medium`.

## Provider candidate tables

One function owns the mapping (`providers::candidates_for(kind, quality) -> Vec<Candidate{service, model}>`),
ordered best-first, filtered at runtime by API-key availability (`api_key_env` set).
If empty after filtering → hard error listing the missing env vars.

| Kind | low | medium | high |
|---|---|---|---|
| image | gemini/imagen-4.0-fast-generate-001 | gemini/imagen-4.0-generate-001 | gemini/imagen-4.0-ultra-generate-001 → gemini/imagen-4.0-generate-001 |
| video | grok-video/grok-imagine-video → veo/veo-3.0-fast-generate-001 | veo/veo-3.0-fast-generate-001 → grok-video | veo/veo-3.0-generate-001 → grok-video |
| music / sfx | suno/V5_5 (music) · suno/V5_SOUND (sfx) | same | same |
| voice | qwen-tts/qwen3-tts-flash → openai-tts/gpt-4o-mini-tts | openai-tts/gpt-4o-mini-tts → elevenlabs/eleven_multilingual_v2 | elevenlabs/eleven_multilingual_v2 → openai-tts/gpt-4o-mini-tts |
| chat (component/react-page/html/style-guide/diagram/document) | groq-chat/meta-llama/llama-4-scout-17b-16e-instruct | same (all quality tiers today) | same |

**Chat auto-select (current code):** all chat asset types resolve to a single `groq-chat` candidate regardless of quality tier. Pin `service:` / `model:` (or CLI `--service`/`--model`) for Anthropic, Gemini chat, OpenAI chat, or z.ai — those providers are implemented and work when pinned. Multi-provider quality tiers for chat remain a future enhancement.

CLI `--service`/`--model` override everything; YAML `service:` pins; otherwise table order.

See also: [prompt-quality-audit.md](prompt-quality-audit.md), [testing.md](testing.md).

## Generation loop (pipeline)

For each output path:

1. Resolve candidate list (pinned service → single entry; else table, capped by `eval.max_attempts`).
2. For each candidate provider: generate `variant_count` variants (existing logic).
3. If an `eval` block exists and the evaluator endpoint is reachable:
   score every variant; best variant passes (weighted score ≥ pass_threshold,
   all `required_pass` criteria individually ≥ threshold, no `reject_if` hit) → accept, stop.
   Otherwise remember global best (score, path) and continue to next provider.
4. No eval block / evaluator unreachable: legacy behavior (multi-variant → pick best
   via evaluator if reachable, else Groq fallback if GROQ_API_KEY set, else first;
   accept immediately, no provider fallback).
5. Candidates exhausted without a pass: keep global best, emit warning with scores.

## Evaluator (eval.rs)

OpenAI-compatible chat completions client.

Endpoint resolution (first reachable wins; probe = GET `{base}/models`, 2s timeout):
1. `MEDIA_EVAL_BASE_URL` env / `--eval-url`
2. `http://192.168.68.59:3713/v1` — LAN inference server hosting Qwen 3.6 (also
   forwards to noizu.server, which the cluster's socat bridge targets)
3. `http://noizu.server:3713/v1` — noizu.server forward of the LAN inference server
4. `http://lmstudio-proxy:3713/v1` — in-cluster service (platform-ai ns; socat bridge)
5. `http://127.0.0.1:3713/v1` — `kubectl port-forward` of the in-cluster bridge

Model: `MEDIA_EVAL_MODEL` / `--eval-model`, else auto-discover from `/models`:
prefer id containing `qwen3.6` or `qwen-3.6`, else any `qwen`, else first
non-embedding model. API key: `MEDIA_EVAL_API_KEY`, default `lm-studio`.
Request timeout: `MEDIA_EVAL_TIMEOUT` seconds, default 300 — the hosted model is a
reasoning model (slow first token); responses may carry a `<think>…</think>` block
which is stripped before JSON parsing.

Scoring request: system prompt instructs the model to grade the artifact against the
criteria and reply ONLY with JSON:
`{"scores": {"<criterion>": 0-10, ...}, "reject_hits": ["<matched reject_if>"], "notes": "..."}`.
Weighted score = Σ(w·s/10)/Σw. Parse defensively (strip fences); on parse failure
retry once, then treat as un-scorable.

Artifact handling by output extension:
- images (png/jpg/jpeg/webp): base64 `image_url` part
- text-like (svg/mmd/puml/dot/html/tsx/ts/js/md/json/txt/css): inline text (truncate ~32KB); with `eval.visual: true`, HTML may use Puppeteer screenshot and SVG may rasterize via rsvg-convert/convert for vision
- video (mp4/webm/mov): hybrid — ≤4 ffmpeg frames for vision when possible + structural stream/duration probes
- audio (mp3/wav/ogg/flac): **structural** scoring via ffprobe duration + ffmpeg volumedetect (no audio-LLM yet)

`eval.mode` (optional): `llm` | `structural` | `hybrid`. Audio/video default to hybrid when mode is omitted so structural never silently no-ops.

When no scorer can run, generation warns and accepts (legacy). Structural reject hits (e.g. silence) can fail the eval gate.

## Decision judge (`judge` subcommand)

Standalone grader for text-like outputs, ported from the Elixir reference
(`genai` System One providers). Talks the **System One** wire protocol:
`POST {base}/systemone` with bearer auth, request
`{model, state, questions: {qid: {type, instructions, criteria}}}`, response
`{answers: {qid: {choice|score|noul, probabilities, legend, confidence}},
request_id, latency_ms, usage}`. Responses are decoded strictly (probability
vector sums to 1 within 1e-6, choice == argmax, legend keys match the labels);
the Alibaba **rounded-hundredths** policy (inferred wire precision:
hundredths-grid probabilities, renormalized mass, attainable-band score check)
applies itself automatically to `decision-model-preview` responses AND to
`jev-*` models (user decision 2026-11-11 — live `jev-1.14.0` reports native
`score` scalars that drift from Σp·i by up to 0.06, which exact equality
rejects). Other models get the strict check only; `judge: policy: strict`
forces exact Σp·i equality everywhere and `policy: rounded` forces the
hundredths policy.

One deliberate deviation from the Elixir reference (live capture 2026-11-11):
real workspace traffic sums to as little as 0.98 across ten on-grid bins, so
the reference's `min(count·0.005, 0.01)` mass tolerance rejects valid answers.
The port widens the missing-mass allowance to the rounding-eaten bound
`count × 0.005` — the bounds and attainable-pre-image checks still reject any
distribution no unit-sum pre-rounding could have produced, and strict stays
exact. Captured payloads are pinned as regression fixtures
(`live_alibaba_missing_mass_within_rounding_slack_is_accepted`,
`live_jev_score_drift_is_strict_rejected_rounded_accepted`).

Limits: ≤255 choice options, ≤10 score levels, text-only state.

### Backends & key resolution

`--judge alibaba` (default) or `--judge jev`; `--judge-model` overrides the
model. Each field resolves CLI > `judge:` config section (media-tool.yaml) >
environment > dc store > backend default:

| Field | Alibaba | Jev |
|---|---|---|
| model | `decision-model-preview` | `jev-latest` |
| base | `QWEN_DECISION_BASE_URL` → dc `services apps.watching_qwen_base_url` (the dedicated decision workspace compatible-mode endpoint, shared with the codemoshiach C1 classifier) → `https://dashscope-intl.aliyuncs.com/compatible-mode/v1` | `JEV_BASE_URL` → `https://api.typesafe.ai/v1` |
| key | `QWEN_API_KEY` → `DASHSCOPE_API_KEY` → dc `services apps.npl_mcp_qwen_api_key` → dc `services apps.npl_mcp_dashscope_api_key` | `JEV_API_KEY` → `TYPESAFE_API_KEY` → dc `defaults inference.jev.key` |

Secrets resolve through `src/secrets.rs`: env vars first (empty and literal
`null` treated as absent), then `dc get <subject> <key> --reveal --raw`
(missing binary, non-zero exit, empty output, literal `null` all absent).
Scheme-less base values (the dc store keeps bare MaaS hosts) are assumed
`https://`. Resolved values are held in a redacting newtype — they never reach
logs, `Debug`, or error strings; diagnostics carry source *names* only. Keys
fail closed: with no source usable the judge refuses to run.

`judge:` config section (all fields optional):

```yaml
judge:
  backend: alibaba        # alibaba | jev
  model: decision-model-preview
  base: https://workspace.example/compatible-mode/v1   # literal override
  policy: strict          # strict | rounded (default: auto-gated by model)
  key:                    # ordered sources, first usable hit wins
    - env: QWEN_API_KEY
    - dc: { subject: services, key: apps.npl_mcp_qwen_api_key }
```

### CLI

```sh
generate-media-prompt judge <file.media.prompt> --output <path> \
  [--judge alibaba|jev] [--judge-model M] [--json]
```

Text-like outputs (svg/html/txt/md/code/…) are judged directly from their
content. Images and video go through an **observe → decide** pass first (see
below); audio still exits 2 — no observation path exists for it. The prompt's
`eval` section becomes the rubric: each criterion → one 10-level `score`
question (levels built from its `description` + `fail_signals`), each
`reject_if` entry → a `noul` question. Per-criterion probabilities and the
weighted score (`EvalScore` semantics shared with the generation-loop eval)
print to the terminal; `--json` emits the full verdict including request_id,
latency and token usage.

### Observe -> decide (raster images and video)

The decision backends are text-only, so a vision **observer** turns the
artifact into a factual per-criterion description first, and the judge scores
the description instead of pixels. Text-like artifacts skip the observer
entirely; video contributes extracted frames (ffmpeg) to the observer.

Observer model chain (best first, first model that answers with usable
per-criterion observations wins):

1. `inference.observer_models` in media-tool.yaml (literal override)
2. `MEDIA_OBSERVER_MODEL` env (prepended)
3. compiled-in default: `qwen3.8-flash` → `qwen-vl-plus` → `qwen-vl-max`

`qwen3.8-flash` leads the default chain because it answered the DashScope
compatible-mode image probe (2026-11-11 live check: a 1×1 PNG sent as an
`image_url` part came back with real image description, not a refusal or
empty content). The VL fallbacks cover a future regression; the observer
validates that every criterion got a non-empty observation and falls down the
chain (and finally errors) when one doesn't.

### Eval-gated generation (best-of-N / attempt loop)

The generation loop shares the same machinery: each attempt's output is
scored, the loop continues while the rubric fails, and multi-candidate runs
(`-n`) pick the best-scoring variant. Candidates score against the prompt's
own `eval:` block when it has one, else the asset-type default
(relevance ×3 + quality ×2, threshold 0.7, relevance required). Scoring
against an empty eval — the pre-2026-11 behavior — made every weighted score
0.0 and candidate 1 win unconditionally; `default_eval_for_type` now anchors
the fallback.

Generation and the `judge` subcommand pick the grader per prompt: `eval.judge:
decision` forces the System One judge, `eval.judge: llm` forces the legacy
text/vision chat grader, absent auto-resolves to `decision` when a judge
backend resolves at startup and `llm` otherwise. The legacy grader itself now
rides the qwen3.8-flash inference profile (`inference:` config section) when
no explicit eval endpoint (`MEDIA_EVAL_BASE_URL` / `--eval-url`) is set, with
Groq (`GROQ_API_KEY`) as the last fallback.

Metadata sidecars (`<output>.metadata.yaml`) carry the eval outcome: attempt
number, verdict, weighted score, per-criterion scores, judge backend/model/
request id, observer model when one ran, and the per-question probability
distributions.

Live smoke against both real backends: `cargo test --test live_judge --
--ignored --nocapture` (skips cleanly when no credentials resolve).

## Duration

`output.duration` (seconds, serde alias `length`) flows into `GenerationOptions.duration_seconds`
and is wired per provider where the API supports it (veo `durationSeconds`,
grok-video duration param, suno track-length hint); ignored elsewhere with a verbose note.

## CLI additions

`--quality <low|medium|high>` (override), `--service <svc>` (pin), `--no-eval`
(skip grading/fallback), `--eval-url <url>`, `--eval-model <id>`.

## Local access to the cluster evaluator

Helper: `bin/media-eval-port-forward` — loops
`kubectl --context noizu -n platform-ai port-forward svc/lmstudio-proxy 3713:3713`.
Needed when the LAN server (192.168.68.59:3713) isn't directly reachable from this
machine. Note: the in-cluster bridge targets the host-side tunnel on node
k8s-mvm-2 (127.0.0.1:3713), fed by the LAN server's forward to noizu.server — that
leg must be up for in-cluster (and port-forwarded) eval to work.
