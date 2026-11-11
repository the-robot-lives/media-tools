# media-tool
〚📁:b0395857-c31c-5990-b364-7b9263836612〛 media-tool README :: Entry point — what/why, getting started, how it works, repo layout.

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

## Named LLM profiles and local document generation

`media-tool` (an installed alias for `generate-media-prompt`) can use independent
local or cloud LLM connections for text-backed assets. Run the keyboard wizard:

```sh
media-tool llm configure
```

The wizard populates provider URLs and credential environment-variable names,
queries available models, supports manual model entry when a server is offline,
and imports connections from LLM Toolkit or queue-populator. Arrow keys/Enter
select; Escape cancels. Edits are saved only after confirmation. Credentials are
never stored in the profile file: enter `OPENAI_API_KEY`, not an API key value.
Import never copies plaintext/encrypted utility credentials or changes the source
utility. Imported remote profiles may need a credential environment reference.

Profiles live in `$XDG_CONFIG_HOME/media-tool/llm-profiles.yaml` (default
`~/.config/media-tool/llm-profiles.yaml`). Override with `MEDIA_TOOL_LLM_CONFIG`.
Selection is **`--llm-profile NAME` > `MEDIA_TOOL_LLM_PROFILE` > saved default**.
An unknown profile fails, rather than falling back to a paid cloud provider.

```sh
# Inspect candidates, then import an existing connection without overwriting one
media-tool llm import
media-tool llm import --name local-qwen
media-tool llm list

# Or define a connection noninteractively (use the actual URL/model from your server)
media-tool llm add local-qwen --provider lmstudio \
  --base-url http://localhost:3717/v1 --model qwen/qwen3.8-27b

MEDIA_TOOL_LLM_PROFILE=local-qwen media-tool llm discover
MEDIA_TOOL_LLM_PROFILE=local-qwen media-tool llm test

# Existing .media.prompt generation uses the selected profile for text/code assets
MEDIA_TOOL_LLM_PROFILE=local-qwen media-tool document.media.prompt

# Or generate a document directly; extension supplies the format contract
MEDIA_TOOL_LLM_PROFILE=local-qwen media-tool llm generate \
  --prompt 'Create an XML catalog of three books' --output catalog.xml
MEDIA_TOOL_LLM_PROFILE=local-qwen media-tool llm generate \
  --prompt 'Create a complete HTML page with a table' --output index.html
MEDIA_TOOL_LLM_PROFILE=local-qwen media-tool llm generate \
  --prompt 'Export a JavaScript function add(a,b)' --output add.js

# Read a longer prompt from a file (or --prompt-file - for stdin)
media-tool --llm-profile local-qwen llm generate \
  --prompt-file requirements.txt --output index.html --max-tokens 16384

# Optional stored default (no shell environment changes)
media-tool llm default local-qwen
```

Providers: OpenAI-compatible endpoints (including Groq, Cerebras, DeepSeek, Z.ai,
Alibaba, OpenRouter, LM Studio and llama.cpp), native Anthropic Messages, and
native Ollama chat. Exact model IDs are discovered from the endpoint; local
servers/models must already be running. The wizard never downloads a model.
`base_url` is the API root (for example `/v1` for OpenAI-compatible servers,
without `/chat/completions`; Ollama uses its host root).

The selected profile overrides prompt-file service/model defaults for text-backed
types (XML, HTML, JavaScript, documents, diagrams, components, etc.). Explicit
`--service` still wins, and `--model` overrides the selected model. Image, audio,
and video provider routing remains unchanged. With a profile selected, implicit
cloud prompt preparation, grading, and multi-variant selection are skipped;
`--eval-url` opts into an explicit evaluator. This keeps local document generation
local. To use a profile with batch `--jobs`, its selection is forwarded to workers.
Direct generation refuses to overwrite an output without `--force` and rejects
empty/truncated responses. Component prompts default to Lit TypeScript unless
React/JSX/TSX is explicitly requested; `react-page` remains React.

For slow local inference, set `MEDIA_TOOL_LLM_TIMEOUT_SECONDS=300` (default180,
allowed1–900seconds). Model discovery remains limited to10seconds. Injected FIM
references are capped at16KiB with an explicit excerpt marker; full guides remain
available in Reference. `make install` bundles them under
`~/.local/share/media-tool/fim/solution` so generation works outside a source checkout.
`MEDIA_FIM_DIR` can override the reference directory; `--no-fim` disables injection.

### Review a native document

```sh
media-tool view score.mjs --type music21j
media-tool view score.musicxml
media-tool view index.html
# Print the local review URL without opening a browser:
media-tool view score.mjs --type music21j --no-open
```

The loader opens a read-only browser view of the chosen file. Ctrl+C closes it.
Music21j documents stay JavaScript modules: export `buildScore(music21)` returning
an initialized Score, Part or Stream. The loader supplies music21j and renders the
returned score. It also accepts a named/default `score` export. Keep runtime
imports, DOM setup and playback controls out of the generated score document.
MusicXML is rendered as notation through the shared OSMD host. The review host
loads pinned browser runtimes; viewing does not call an LLM or start the lab.
Only the chosen file is served; there is no arbitrary-file or generation endpoint.
