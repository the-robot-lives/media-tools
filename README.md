# media-tool

**Repo:** https://github.com/the-robot-lives/media-tools

Generate media assets (image, audio, TTS, video) from declarative `.media.prompt` YAML files.

## What

A Rust CLI (`generate-media-prompt`) that reads `.media.prompt` files, resolves dependency DAGs and attachments, and calls generation APIs. Gemini/Imagen is fully implemented, with a pluggable provider architecture (OpenAI, Stability, Replicate, Suno, ElevenLabs, Qwen, Veo, and more — some stubbed). Includes an interactive refinement loop and a browser-based test lab UI.

## Why

Repeated prompt-and-download work across many provider APIs doesn't compose. Declarative prompt files make asset generation reproducible, diffable, and pipeline-able — with dependency chaining (reference a generated logo inside a hero prompt), variant generation, and an evaluation harness for grading model output against criteria.

## Getting Started

```bash
make install          # builds and installs to ~/.local/bin (make test for the suite)
make lab              # test lab UI → http://127.0.0.1:8787
```

```bash
generate-media-prompt hero.media.prompt            # generate from a prompt file
generate-media-prompt -n 3 hero.media.prompt       # 3 variants
generate-media-prompt --dry-run --verbose assets/  # preview plan, no API calls
generate-media-prompt --refine hero.media.prompt   # interactive regenerate loop
```

## How It Works

- **Prompt files**: YAML, schema v0.4 — `type` (image/audio/tts/video), `quality` (low/medium/high drives provider selection), `prompt.text`/`negative`, `output` formats/dimensions, optional `depends_on` and `attachments`.
- **Dependencies**: `depends_on` refs pull earlier outputs into later prompts; `collapse` modes control whether a dependency arrives as a file or inline reference. A DAG resolver orders generation.
- **Attachments**: reference files with roles (`style`, `subject`, …); sent per provider rules.
- **Refinement**: `--refine` loops generation with feedback until satisfied.
- **Post-processing**: pluggable output transforms (resize, format conversion, etc.).
- **Evaluation**: cluster-backed evaluator grades outputs against a criteria catalog; model discovery and endpoint resolution are configurable.
- **API keys**: resolved from environment (e.g. `GEMINI_API_KEY`) with layered fallbacks; never committed.

Deep reference — full schema fields, CLI flags, provider matrix, attachments/dependencies algorithms, eval setup — lives in `docs/` (`PROJ-SCHEMA.md`, `PROJ-ARCH.md`, `providers.md`, `quality-selection-and-eval.md`, `docs/howto/`).

## Repo Layout

- `src/` — Rust crate (`providers/`, `renderers/`, DAG resolver, test lab, TUI)
- `bin/` — `generate-media-prompt`, `media-eval-port-forward`
- `web/` — test lab frontend · `helm/` — vendored chart · `skill/` — content-media-engine agent skill
- `demos/`, `media-tool.yaml` — examples and provider/prompt config

## Gemini image model minimum

Image generation requires **Gemini 3 or newer**. Gemini 2.x models and unversioned
aliases are rejected before prompt prep, eval, or any API call — including under
`--dry-run` — whether they arrive from `service:`/`model:` in a prompt file, `--model`
on the CLI, a `media-tool.yaml` fallback tier, or a `generate_content_model` provider
option. The whole 3.x family is allowed, `gemini-3-pro-image` included. Gemini
chat/text models are unaffected.

`generate-media-prompt models` prints the documented image catalog and the policy
offline. See `docs/providers.md` for details.
