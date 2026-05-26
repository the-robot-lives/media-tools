# Provider Implementation Guide

> Tracking doc for implementing generation providers in `generate-media-prompt`.
>
> Each provider is one function (~60-130 lines) following the `generate_gemini` reference implementation, plus one line in the `PROVIDERS` dispatch table.

---

## Status Overview

| Provider | Type | Status | Priority | Effort | API Key Env |
|----------|------|--------|----------|--------|-------------|
| `gemini` | Image | **Done** | — | — | `GEMINI_API_KEY` |
| `openai` | Image | Todo | P0 | Low | `OPENAI_API_KEY` |
| `stability` | Image | Todo | P0 | Low | `STABILITY_API_KEY` |
| `elevenlabs` | Audio | Todo | P1 | Low | `ELEVENLABS_API_KEY` |
| `replicate` | Image | Todo | P1 | Medium | `REPLICATE_API_TOKEN` |
| `runway` | Video | Todo | P1 | Medium | `RUNWAY_API_KEY` |
| `ideogram` | Image | Todo | P2 | Low | `IDEOGRAM_API_KEY` |
| `recraft` | Image | Todo | P2 | Low | `RECRAFT_API_KEY` |
| `fal` | Image | Todo | P2 | Medium | `FAL_KEY` |
| `together` | Image | Todo | P2 | Low | `TOGETHER_API_KEY` |
| `fireworks` | Image | Todo | P2 | Low | `FIREWORKS_API_KEY` |
| `local` | Image | Todo | P2 | Medium | none |
| `midjourney` | Image | Todo | P3 | High | varies |
| `bark` | Audio | Todo | P3 | Medium | none |
| `musicgen` | Audio | Todo | P3 | Medium | none |
| `suno` | Audio | Todo | P3 | High | `SUNO_API_KEY` |
| `udio` | Audio | Todo | P3 | High | `UDIO_API_KEY` |
| `pika` | Video | Todo | P3 | Medium | `PIKA_API_KEY` |
| `kling` | Video | Todo | P3 | Medium | `KLING_API_KEY` |
| `minimax` | Video | Todo | P3 | Medium | `MINIMAX_API_KEY` |

---

## Provider Function Signature

Every provider implements the same interface:

```python
def generate_<name>(
    prompt_text: str,        # The generation prompt
    output_path: str,        # Where to write the output file
    api_key: str,            # Provider API key
    model: str = "<default>",
    aspect_ratio: str | None = None,
    negative_prompt: str | None = None,
    provider_options: dict | None = None,
    attachments: list[dict] | None = None,   # [{path, role, mime_type, data_b64}]
    verbose: bool = False,
) -> bool:                   # True on success, False on recoverable failure
```

Then register: `"<name>": generate_<name>` in the `PROVIDERS` dict.

---

## Shared Infrastructure Needed

Before implementing multiple providers, extract these helpers:

### 1. Generic API Key Resolution

Currently the bash wrapper only resolves `GEMINI_API_KEY`. Needs a generic lookup:

```bash
# Resolution order per provider:
# 1. <PROVIDER>_API_KEY env var
# 2. .k8-secrets.yaml at $INFRA_ROOT → <provider>.api_key via yq
# 3. Die with instructions
```

The Python engine should accept all keys via env vars. The bash wrapper should resolve each provider's key from `.k8-secrets.yaml` if the env var is unset.

**`.k8-secrets.yaml` structure:**
```yaml
gemini:
  api_key: "..."
openai:
  api_key: "..."
stability:
  api_key: "..."
replicate:
  api_token: "..."
elevenlabs:
  api_key: "..."
runway:
  api_key: "..."
```

### 2. Async Polling Helper

Replicate, Runway, Pika, Kling, and Minimax all use submit-then-poll patterns. Extract:

```python
def poll_until_complete(
    status_url: str,
    headers: dict,
    status_field: str = "status",        # JSON path to status value
    success_value: str = "succeeded",
    failure_values: list[str] = ["failed", "canceled"],
    output_field: str = "output",        # JSON path to result
    interval: float = 2.0,              # Poll interval in seconds
    timeout: float = 300.0,             # Max wait time
    verbose: bool = False,
) -> dict | None:
    """Poll a prediction/task URL until completion. Returns response dict or None on failure."""
```

### 3. Download Helper

For providers that return a URL instead of base64:

```python
def download_to_file(url: str, output_path: str, verbose: bool = False) -> bool:
    """Download a file from URL to output_path. Returns True on success."""
```

---

## P0 — Implement First

### `openai`

**Why first:** Most users already have an API key. Well-documented, simple REST.

**Endpoint:** `POST https://api.openai.com/v1/images/generations`

**Auth:** `Authorization: Bearer {api_key}`

**Request body:**
```json
{
  "model": "dall-e-3",
  "prompt": "...",
  "n": 1,
  "size": "1024x1024",
  "quality": "hd",
  "style": "natural",
  "response_format": "b64_json"
}
```

**Response:** `data[0].b64_json` → base64-decode → write to file

**`provider_options` mapping:**
| Option | API Field | Values |
|--------|-----------|--------|
| `quality` | `quality` | `hd`, `standard` |
| `size` | `size` | `1024x1024`, `1024x1792`, `1792x1024`, `256x256`, `512x512` |
| `style` | `style` | `natural`, `vivid` |

**Aspect ratio mapping:** `1:1` → `1024x1024`, `9:16` → `1024x1792`, `16:9` → `1792x1024`

**Attachments:** DALL-E 3 generation doesn't support reference images. DALL-E 2 edits endpoint does (separate flow).

**Models:**
- `dall-e-3` — default, best quality
- `dall-e-2` — faster, cheaper, supports edits/variations
- `gpt-image-1` — newest, native image generation in GPT-4o

**Estimated lines:** ~80

---

### `stability`

**Why second:** Popular, good at specific styles, straightforward REST.

**Endpoint:** `POST https://api.stability.ai/v2beta/stable-image/generate/core`

**Auth:** `Authorization: Bearer {api_key}` + `Accept: image/*`

**Request:** Multipart form data (not JSON):
```
prompt: "..."
negative_prompt: "..."
aspect_ratio: "16:9"
output_format: "png"
seed: 42
```

**Response:** Raw image bytes (not base64, not JSON) — write directly to file.

**`provider_options` mapping:**
| Option | Form Field | Notes |
|--------|-----------|-------|
| `aspect_ratio` | `aspect_ratio` | `1:1`, `16:9`, `9:16`, `3:2`, `2:3`, etc. |
| `output_format` | `output_format` | `png`, `jpeg`, `webp` |
| `seed` | `seed` | Integer for reproducibility |
| `style_preset` | `style_preset` | `photographic`, `anime`, `digital-art`, etc. |

**Attachments:** Supported for img2img — send as `image` field in multipart form with `strength` parameter.

**Models:**
- `stable-image-core` — default, balanced
- `stable-diffusion-3.5-large` — highest quality
- `stable-diffusion-3.5-large-turbo` — fast

**Note:** Multipart form encoding differs from the JSON pattern used by Gemini/OpenAI. Need `urllib.request` with manual multipart boundary construction (no `requests` library).

**Estimated lines:** ~100 (multipart encoding adds complexity)

---

## P1 — Implement Next

### `elevenlabs`

**Why:** Unlocks audio generation — the first non-image provider.

**Endpoint:** `POST https://api.elevenlabs.io/v1/text-to-speech/{voice_id}`

**Auth:** `xi-api-key: {api_key}`

**Request body:**
```json
{
  "text": "...",
  "model_id": "eleven_multilingual_v2",
  "voice_settings": {
    "stability": 0.5,
    "similarity_boost": 0.8
  }
}
```

**Response:** Raw audio bytes (mp3) — write directly to file.

**`provider_options` mapping:**
| Option | API Field | Notes |
|--------|-----------|-------|
| `voice_id` | URL path param | Required — no default. E.g., `21m00Tcm4TlvDq8ikWAM` |
| `stability` | `voice_settings.stability` | 0.0-1.0 |
| `similarity_boost` | `voice_settings.similarity_boost` | 0.0-1.0 |
| `model_id` | `model_id` | `eleven_multilingual_v2`, `eleven_turbo_v2` |

**Attachments:** Voice cloning uses a separate endpoint (`/v1/voice-generation/generate-voice`). Not needed for initial implementation.

**Estimated lines:** ~70

---

### `replicate`

**Why:** Generic runner for any open model — Flux, SDXL, LLaMA, Whisper, etc.

**Endpoint:** `POST https://api.replicate.com/v1/predictions`

**Auth:** `Authorization: Bearer {api_key}`

**Pattern:** Async (submit → poll → download)

**Request body:**
```json
{
  "model": "black-forest-labs/flux-1.1-pro",
  "input": {
    "prompt": "...",
    "aspect_ratio": "16:9",
    "num_inference_steps": 28,
    "guidance_scale": 3.5
  }
}
```

**Response flow:**
1. POST returns `{ "id": "abc123", "status": "starting", "urls": { "get": "..." } }`
2. Poll `GET /v1/predictions/abc123` until `status` = `succeeded`
3. `output` field contains URL(s) to generated file(s)
4. Download first output URL → write to file

**`provider_options` mapping:** Passed directly as `input` fields (model-specific). Common ones:
| Option | Notes |
|--------|-------|
| `guidance_scale` | CFG scale (Flux default: 3.5, SD default: 7.0) |
| `num_inference_steps` | Steps (Flux: 28, SD: 30) |
| `seed` | Reproducibility |
| `width` / `height` | Override dimensions |

**Attachments:** Supported — send as base64 data URL in `input.image` field.

**Needs:** `poll_until_complete` helper, `download_to_file` helper.

**Estimated lines:** ~100

---

### `runway`

**Why:** Unlocks video generation — the first video provider.

**Endpoint:** `POST https://api.dev.runwayml.com/v1/image_to_video`

**Auth:** `Authorization: Bearer {api_key}` + `X-Runway-Version: 2024-11-06`

**Pattern:** Async (submit → poll → download)

**Request body:**
```json
{
  "model": "gen3a_turbo",
  "promptImage": "<base64 or URL>",
  "promptText": "...",
  "duration": 5,
  "ratio": "16:9"
}
```

**Response flow:**
1. POST returns `{ "id": "task-abc123" }`
2. Poll `GET /v1/tasks/task-abc123` until `status` = `SUCCEEDED`
3. `output` field contains URL to generated video
4. Download → write to file

**`provider_options` mapping:**
| Option | API Field | Notes |
|--------|-----------|-------|
| `duration` | `duration` | 5 or 10 seconds |
| `ratio` | `ratio` | `16:9`, `9:16`, `1:1` |
| `motion_amount` | Not in official API | Depends on model version |
| `seed` | `seed` | Reproducibility |

**Attachments:** The `promptImage` field IS the primary attachment (base image for image-to-video). Maps from `attachments[role=base]` or from `depends_on` with `collapse: file`.

**Needs:** `poll_until_complete` helper, `download_to_file` helper.

**Estimated lines:** ~110

---

## P2 — Fill Out Coverage

### `ideogram`

**Endpoint:** `POST https://api.ideogram.ai/generate`

**Auth:** `Api-Key: {api_key}`

**Request body:**
```json
{
  "image_request": {
    "prompt": "...",
    "negative_prompt": "...",
    "aspect_ratio": "ASPECT_16_9",
    "model": "V_2",
    "style_type": "DESIGN"
  }
}
```

**Response:** `data[0].url` → download to file.

**Notes:** Best at text-in-image. Aspect ratios use enum format (`ASPECT_1_1`, `ASPECT_16_9`, etc.).

**Estimated lines:** ~75

---

### `recraft`

**Endpoint:** `POST https://external.api.recraft.ai/v1/images/generations`

**Auth:** `Authorization: Bearer {api_key}`

**Request body:**
```json
{
  "prompt": "...",
  "negative_prompt": "...",
  "model": "recraftv3",
  "style": "vector_illustration",
  "substyle": "flat_2",
  "size": "1024x1024",
  "response_format": "b64_json"
}
```

**Response:** `data[0].b64_json` → decode → write.

**Notes:** Can output SVG natively — only provider that does this. Use `response_format: "url"` for SVG outputs.

**Estimated lines:** ~75

---

### `fal`

**Endpoint:** `POST https://fal.run/{model_id}` (sync) or `POST https://queue.fal.run/{model_id}` (async)

**Auth:** `Authorization: Key {api_key}`

**Pattern:** Sync for small models, async with queue for larger ones.

**Request body:** Model-specific input dict.

**Response:** `images[0].url` → download.

**Notes:** Generic runner. Model IDs like `fal-ai/flux/dev`, `fal-ai/stable-diffusion-v35-large`.

**Estimated lines:** ~90

---

### `together`

**Endpoint:** `POST https://api.together.xyz/v1/images/generations`

**Auth:** `Authorization: Bearer {api_key}`

**Notes:** OpenAI-compatible endpoint. Same request/response format as OpenAI. Supports Flux, SDXL.

**Estimated lines:** ~60 (reuse OpenAI logic with different URL)

---

### `fireworks`

**Endpoint:** `POST https://api.fireworks.ai/inference/v1/image_generation/{model}`

**Auth:** `Authorization: Bearer {api_key}`

**Response:** `data[0].b64_json` → decode → write.

**Estimated lines:** ~70

---

### `local`

Two sub-targets:

**ComfyUI** (`http://localhost:8188`):
- Submit workflow JSON via `POST /prompt`
- Poll `GET /history/{prompt_id}` until complete
- Download output from `GET /view?filename=...`
- Needs a workflow JSON file (from `provider_options.workflow`)
- ~120 lines

**Automatic1111** (`http://localhost:7860`):
- `POST /sdapi/v1/txt2img` with JSON body
- Response: `images[0]` as base64 → decode → write
- Simpler than ComfyUI
- ~70 lines

**Detect which is running** via port or `provider_options.backend: comfyui | a1111`.

**Estimated lines:** ~150 (both backends)

---

## P3 — Lower Priority

### `midjourney`

No official API. Requires a third-party proxy service (useapi.net, midjourney-api, etc.). Each proxy has its own endpoint/auth pattern. Async with long generation times (30-60s).

**Recommendation:** Implement via Replicate's Midjourney-compatible models instead, or wait for official API.

**Estimated lines:** ~130 (proxy-dependent)

---

### `bark` / `musicgen`

Both can run locally via Python packages or via Replicate API.

**Local:** Shell out to `python3 -c "from bark import generate_audio; ..."` or `transformers` pipeline. Requires models downloaded locally.

**Replicate:** Use the `replicate` provider with model IDs like `suno/bark` or `meta/musicgen`.

**Recommendation:** Implement as Replicate model presets rather than standalone providers. Add model aliases:
```python
MODEL_ALIASES = {
    "bark": "suno/bark:latest",
    "musicgen": "meta/musicgen:latest",
}
```

---

### `suno` / `udio`

No stable public APIs. Third-party wrappers exist but break frequently. Not recommended for implementation until official APIs launch.

**Estimated lines:** Unknown (API instability)

---

### `pika` / `kling` / `minimax`

All follow the same async submit-poll-download pattern as Runway. Implementation is straightforward once the `poll_until_complete` helper exists.

**Estimated lines:** ~80 each

---

## Implementation Checklist (per provider)

- [ ] Write `generate_<name>()` function in `media-prompt-engine.py`
- [ ] Add to `PROVIDERS` dispatch dict
- [ ] Add API key env var to bash wrapper resolution logic
- [ ] Add to `.k8-secrets.yaml` example structure
- [ ] Add `provider_options` examples to README.md
- [ ] Test with `--dry-run` (YAML parsing, output naming)
- [ ] Test with real API key (actual generation)
- [ ] Update provider status table in this doc
