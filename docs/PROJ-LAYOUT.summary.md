# Project Layout (Summary)

```
media-tool/
├── src/                            # Rust source code
│   ├── lib.rs                      # Library surface (CLI + GUI front-ends)
│   ├── main.rs                     # CLI entry point
│   ├── orchestrator.rs             # Run orchestration (input resolution, DAG grouping)
│   ├── schema.rs                   # .media.prompt YAML parsing
│   ├── structural.rs               # Structural format detection
│   ├── provider_config.rs          # Runtime config loader (media-tool.yaml)
│   ├── pipeline.rs                 # Generation pipeline
│   ├── dag.rs                      # Dependency DAG resolution
│   ├── attachments.rs              # File attachment handling
│   ├── output.rs                   # Output file management
│   ├── postprocess.rs              # Image crop/resize post-processing
│   ├── imagefmt.rs                 # Image magic-number/format reconciliation
│   ├── text.rs                     # Char-boundary-safe truncation
│   ├── eval.rs                     # Evaluation and scoring
│   ├── refine.rs                   # Interactive refinement
│   ├── prep.rs                     # Groq-based prompt preparation
│   ├── validate.rs                 # Output validation (SVG lint/fix)
│   ├── fim.rs                      # FIM solution loader
│   ├── telemetry.rs                # Progress telemetry events
│   ├── term_layer.rs               # CLI terminal subscriber for telemetry
│   ├── ui.rs                       # Terminal UI
│   ├── providers/                  # 16 provider implementations
│   ├── renderers/                  # 4 renderer implementations
│   └── test_lab/                   # Local eval/test-lab web server
├── bin/                            # Bash wrapper + eval port-forward helper
├── lib/                            # Legacy Python engine
├── web/                            # Phoenix + Hologram landing site (app, config, priv, test)
├── helm/media-tool-landing/        # Helm chart for the landing site
├── macos/                          # SwiftUI MediaWorkbench desktop app + design-direction mockups
├── demos/                          # Example .media.prompt files (11 asset types)
├── design-direction/               # Themed design mockup prompts
├── skill/content-media-engine/     # Claude Code skill definition (assets, references/fim)
├── scripts/                        # Utility scripts (live-eval-report.sh)
├── project-management/             # Personas (8) and user stories (100+)
├── docs/                           # Layout, arch, schema, FAQ, howto, provider, quality/eval, design docs
├── .github/workflows/ci.yml        # CI
├── Cargo.toml                      # Rust package config
├── Makefile                        # Build targets
├── media-tool.yaml                 # Runtime provider/model config
├── HOW-TO.md                       # Quick-start prompt writing guide
├── CLAUDE.md                       # Claude Code agent guidance
├── AGENT.md / AGENTS.md            # Multi-agent (Grok Build) guidance
├── CHANGELOG.md
├── LICENSE
└── README.md                       # Full documentation
```
