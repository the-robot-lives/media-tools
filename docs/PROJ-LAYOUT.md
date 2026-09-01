# Project Layout

```
media-tool/
├── src/                            # Rust source (CLI + test-lab server) → [layout/src.md](layout/src.md)
│   ├── main.rs                     #   CLI entry point (clap, pipeline orchestration)
│   ├── schema.rs                   #   .media.prompt YAML parsing and normalization
│   ├── provider_config.rs          #   Runtime provider config loader (media-tool.yaml)
│   ├── test_lab/                   #   Local eval/test-lab web server → [layout/src.md](layout/src.md)
│   ├── providers/                  #   16 provider implementations → [layout/src.md](layout/src.md)
│   └── renderers/                  #   5 markup → visual renderers → [layout/src.md](layout/src.md)
├── bin/                            # Shell entry points
│   ├── generate-media-prompt       #   Bash wrapper (k8-lib, Python engine dispatch)
│   └── media-eval-port-forward     #   kubectl port-forward for in-cluster eval proxy
├── lib/                            # Legacy Python engine
│   └── media-prompt-engine.py      #   Single-file Python engine (stdlib + pyyaml)
├── web/                            # Phoenix 1.8 + Hologram landing/documentation site
│   ├── app/                        #   Hologram components, layouts, pages (home/format/providers/extensibility)
│   ├── lib/media_tool_web/         #   Endpoint, router, application supervision
│   ├── config/                     #   Mix configs (config/prod/runtime/test.exs)
│   ├── priv/static/                #   Static assets (css, favicon, robots.txt)
│   ├── test/                       #   ExUnit tests
│   ├── Dockerfile                  #   Container build for the site
│   └── mix.exs / mix.lock          #   Elixir deps (phoenix, hologram, bandit)
├── helm/                           # Deployment charts for the landing site
│   └── media-tool-landing/         #   Chart + values; wraps static-site subchart
├── demos/                          # Working .media.prompt examples by asset type
│   ├── image/                      #   Hero images, logos (Gemini Imagen)
│   ├── svg/                        #   SVG illustrations (chat + render)
│   ├── diagram/                    #   Mermaid, PlantUML diagrams
│   ├── html/                       #   HTML pages, React components
│   ├── component/                  #   UI component generation (sample button)
│   ├── document/                   #   Document generation (sample readme)
│   ├── video/                      #   Veo, Grok video clips
│   ├── music/                      #   Lo-fi beat (Suno)
│   ├── sfx/                        #   Sound effects (Suno, sample whoosh)
│   ├── voice/                      #   OpenAI TTS, ElevenLabs, Qwen TTS
│   └── game/                       #   HTML5 game (Breakout clone)
├── design-direction/               # Themed design mockup prompts (terminal-tessera, blueprint-dag, …)
├── skill/                          # Claude Code skill definitions
│   └── content-media-engine/       #   Content media engine skill
│       ├── SKILL.md                #     Skill entry point and triggers
│       ├── assets/                 #     Templates, trackers, example prompts, fim/ assets
│       └── references/             #     FIM library (fim/, fim-index.md), prompt-templates/, guides
├── scripts/                        # Utility scripts
│   └── live-eval-report.sh         #   Generates report from live eval runs
├── project-management/             # Product management artifacts
│   ├── personas/                   #   8 user personas with index.yaml
│   └── user-stories/               #   100+ user stories with index.yaml
├── docs/                           # Documentation
│   ├── PROJ-LAYOUT.md              #   This file
│   ├── PROJ-LAYOUT.summary.md      #   Quick-reference tree
│   ├── PROJ-ARCH.md / .summary.md  #   Architecture doc
│   ├── PROJ-FAQ.md / .summary.md   #   FAQ
│   ├── PROJ-HOWTO.md / .summary.md #   How-to index
│   ├── howto/                      #   Task guides (first-hour, dependencies, rich formats, troubleshooting)
│   ├── layout/                     #   Detailed breakdowns (src.md)
│   ├── providers.md                #   Provider implementation guide
│   ├── quality-selection-and-eval.md  # Quality selection + eval system design
│   ├── eval-criteria-catalog.md    #   Eval criteria reference
│   ├── prompt-quality-audit.md     #   Prompt quality audit notes
│   └── testing.md                  #   Test approach
├── .github/workflows/ci.yml        # CI (build, test)
├── .gitignore                      # Rust target/, Python caches, lab-workspace/, tmp/
├── Cargo.toml                      # Rust package definition (bin + deps)
├── Cargo.lock                      # Locked dependency versions
├── Makefile                        # build, test, install, clean targets
├── media-tool.yaml                 # Runtime provider/model config (defaults, tiers, limits)
├── HOW-TO.md                       # Quick reference for writing .media.prompt files
├── LICENSE                         # License file
└── README.md                       # Full user documentation (schema, CLI, providers)
```

## Key Files Requiring Setup

| File | Action |
|------|--------|
| API keys | Set `GEMINI_API_KEY`, `SUNO_API_KEY`, `OPENAI_API_KEY`, etc. in `.envrc` or environment (never commit) |
| `media-tool.yaml` | Optional — copied to `./media-tool.yaml` or `~/.config/media-tool/` to override compiled-in defaults |
| `make install` | Builds Rust binary and installs to `~/.local/bin/generate-media-prompt` |

## Generated / Local-Only Artifacts

- **demos/.genai.\*** directories contain cached generation outputs (timestamped); these are working artifacts, not source
- **lab-workspace/** is gitignored — generated at runtime by the test-lab server (prompts, outputs, `settings.json`, `examples-index.yaml`)
- **target/**, **\_\_pycache\_\_/** are build caches (gitignored)
- **merge-notes.md** is a transient working note, not maintained documentation
