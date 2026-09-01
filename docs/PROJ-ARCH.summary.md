# Project Architecture — Summary

## Overview

media-tool generates media assets from declarative YAML `.media.prompt` files (schema v0.4). Authors declare intent (asset type, quality tier, prompt, eval criteria); the tool handles provider auto-selection, dependency-DAG ordering, generation across 16 provider APIs, markup rendering, LLM eval grading with provider fallback, and interactive refinement. Primary implementation: Rust binary `generate-media-prompt` (clap/tokio/ratatui) installed to `~/.local/bin`; legacy bash+Python engine kept as no-cargo fallback. Repo also ships a Phoenix+Hologram landing site (`web/`, helm `media-tool-landing`) and a local test-lab web server (`src/test_lab/`).

## Core Components

- `main.rs` — CLI entry, input resolution, dispatch
- `schema.rs` — YAML parsing, legacy v0.1–v0.3 normalization
- `provider_config.rs` — runtime `media-tool.yaml` overrides (defaults, tiers, limits)
- `structural.rs` — markup format detection/normalization
- `dag.rs` — dependency DAG, cycle detection, Kahn's tiering
- `pipeline.rs` — orchestration, dry-run, quality→provider selection
- `providers/` — 16 implementations: image (Gemini Imagen, Qwen image, DashScope, ZAI), music/SFX (Suno), TTS (OpenAI, ElevenLabs, Qwen), video (Grok, Veo, Wan), chat (Gemini, Anthropic, OpenAI, Groq, OpenRouter, LiteLLM/ZAI)
- `renderers/` — Mermaid, PlantUML, Graphviz, Puppeteer (markup → visual)
- `test_lab/` — local browser lab server (catalog, settings, workspace persistence)
- `eval.rs` — weighted-criteria vision grading (Qwen 3.6), drives provider fallback
- `refine.rs` / `prep.rs` / `validate.rs` — feedback loop, prompt expansion, SVG lint auto-fix
- `bin/` + `lib/` — legacy bash wrapper (k8-lib) and Python engine; `media-eval-port-forward` kubectl helper
- `web/` + `helm/` — Phoenix+Hologram landing/docs site and its deploy chart
- `skill/content-media-engine/` — Claude Code skill packaging; `demos/` — working examples per asset type (10 kinds)

## Provider Architecture

Three categories dispatched via `MediaProvider` trait registry: media APIs (binary output), chat-completion APIs (text/code), and local renderers (post-generation markup → visual). Model defaults, tier ladders, and prompt limits overridable at runtime via `media-tool.yaml` (local file or URL) without rebuilding. New provider = trait impl + registration.

## Generation Flow

Parse/normalize → resolve depends_on DAG (cycles abort) → generate tier by tier with `${alias}` substitution (collapse: file/inline/context) → output naming/formats → optional eval grading.

## Quality Selection & Eval

`quality: low|medium|high` drives candidate provider order (compiled-in or `image_tiers` ladder); each output is graded against the prompt's eval block by a hosted Qwen 3.6 vision model, with fallback to the next provider until pass or exhaustion (best kept with warning). Evaluator endpoint probed: MEDIA_EVAL_BASE_URL → LAN server → noizu.server → in-cluster lmstudio-proxy → local port-forward.

## Test Lab

`src/test_lab/` serves a local browser UI over a gitignored `lab-workspace/` (prompts, outputs, `settings.json`, `examples-index.yaml`; legacy `tmp/live-eval/` auto-migrated) and drives generation through the same pipeline/providers as the CLI.

## Web Landing Site

`web/` — Phoenix 1.8 + Hologram 0.10 (bandit) docs/marketing site; pages: home, format, providers, extensibility/getting-started; deployed via `helm/media-tool-landing` (static-site subchart), separate lifecycle from the CLI.

## Ecosystem Integration

`make install` → `~/.local/bin` (repo-wide make install-utilities flow); bash wrapper sources k8-lib common.sh; API keys resolve env → `.envrc.k8.dc` at `$INFRA_ROOT`; runtime config via `MEDIA_TOOL_CONFIG`/`MEDIA_TOOL_CONFIG_URL`/local `media-tool.yaml`; eval can use the noizu k8s cluster's lmstudio-proxy service; packaged as the content-media-engine Claude Code skill.

## Key Decisions

- Rust rewrite for single binary + TUI + async polling; Python engine as fallback only
- Declarative YAML with quality tiers instead of provider pinning
- Runtime config layer (`media-tool.yaml`) so model/tier changes need no rebuild
- Cross-file dependency DAG as first-class batch model
- Eval-gated generation with graceful degradation when no evaluator reachable
- Renderers as post-transforms, separate from generation providers

## Known Gaps

Post-processing actions stubbed; within-tier parallelism, inline/context collapse substitution, and several image providers still planned.
