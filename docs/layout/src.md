# src/ — Application Source

```
src/
├── main.rs                  # CLI entry point — clap parser, input resolution, pipeline dispatch
├── schema.rs                # YAML .media.prompt parsing — schema v0.1–v0.4 normalization
├── structural.rs            # Structural format detection/normalization (mermaid, html, etc.)
├── provider_config.rs       # Runtime provider config loader — media-tool.yaml / MEDIA_TOOL_CONFIG
├── pipeline.rs              # Generation pipeline — dry-run preview, execution, tier ordering
├── dag.rs                   # Dependency DAG — cycle detection, topological sort (Kahn's)
├── attachments.rs           # Attachment loading — file reads, MIME detection, base64 encoding
├── output.rs                # Output handling — filename derivation, multi-format, variant numbering
├── eval.rs                  # Evaluation — criteria matching, vision-based scoring via LLM
├── refine.rs                # Refinement loop — feedback collection, prompt rewriting via LLM
├── prep.rs                  # Prompt preparation — Groq LLM prompt expansion per asset type
├── validate.rs              # Output validation — SVG lint errors, bounded auto-fix attempts
├── fim.rs                   # FIM loader — reads skill/content-media-engine/references/fim/solution/
├── ui.rs                    # Terminal UI — ratatui TUI, indicatif progress bars, dialoguer prompts
├── providers/               # 16 generation provider implementations
│   ├── mod.rs               #   MediaProvider trait, provider registry/dispatch, compiled-in defaults
│   ├── gemini.rs            #   Google Imagen — image generation (synchronous)
│   ├── gemini_chat.rs       #   Gemini chat — text/code generation for markup assets
│   ├── anthropic.rs         #   Anthropic Claude — text/code generation
│   ├── openai_chat.rs       #   OpenAI chat — text/code generation
│   ├── groq_chat.rs         #   Groq chat — fast text/code generation
│   ├── openrouter.rs        #   OpenRouter — multi-provider chat gateway
│   ├── zai.rs               #   ZAI — image generation
│   ├── dashscope.rs         #   Alibaba DashScope — Qwen image generation
│   ├── qwen_image.rs        #   Qwen image — DashScope intl endpoint (qwen-image models)
│   ├── suno.rs              #   Suno — music generation (async polling with timeout)
│   ├── openai_tts.rs        #   OpenAI TTS — text-to-speech (synchronous)
│   ├── elevenlabs.rs        #   ElevenLabs — TTS with voice cloning (synchronous)
│   ├── qwen_tts.rs          #   Qwen TTS — Alibaba DashScope TTS (synchronous)
│   ├── grok_video.rs        #   xAI Grok — video generation (async polling)
│   ├── wan_video.rs         #   Wan — video generation (DashScope, async polling)
│   └── veo.rs               #   Google Veo — video generation (async polling)
├── renderers/               # Markup → visual output renderers
│   ├── mod.rs               #   Renderer trait, availability detection
│   ├── mermaid.rs           #   Mermaid — mmdc CLI or Puppeteer-based rendering
│   ├── plantuml.rs          #   PlantUML — server or CLI rendering
│   ├── graphviz.rs          #   Graphviz DOT — dot CLI rendering
│   └── puppeteer.rs         #   Puppeteer — headless Chrome screenshot capture
└── test_lab/                # Local eval/test-lab server (browser-based prompt lab)
    ├── mod.rs               #   Module wiring
    ├── server.rs            #   HTTP server serving the lab UI + generation endpoints
    ├── catalog.rs           #   Prompt catalog — scans workspace for .media.prompt files
    ├── registry.rs          #   Generation job registry/status tracking
    ├── persist.rs           #   Workspace persistence — settings.json, examples-index.yaml
    ├── settings.rs          #   Lab settings model (provider/model/key selection)
    └── static/              #   Lab UI assets (index.html, viewer.html)
```

## Module Dependencies

```
main.rs
  ├── schema.rs          (YAML parsing)
  ├── dag.rs             (dependency resolution)
  ├── pipeline.rs        (orchestration)
  │     ├── providers/*  (generation dispatch)
  │     ├── renderers/*  (markup rendering)
  │     ├── output.rs    (file writing)
  │     └── eval.rs      (quality scoring)
  ├── provider_config.rs (runtime config overrides)
  ├── attachments.rs     (file loading)
  ├── refine.rs          (interactive loop)
  ├── prep.rs            (prompt expansion)
  ├── validate.rs        (output validation)
  ├── structural.rs      (format detection)
  ├── fim.rs             (FIM reference loading)
  └── ui.rs              (terminal display)
test_lab/*               (standalone lab server; reuses pipeline/providers)
```
