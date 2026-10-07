# Test Health — media-tools
_Last measured: 2026-10-07 · branch develop@715eae9_

| Metric | Before | After |
|---|---|---|
| CI PR wall-clock — critical path (warm / cold) | — / 86s (docker smoke only; no tests ran) | see PR run (rust-test is the critical path cold) |
| Main release build (warm / cold) | — / ~3m44s (smoke 87s → build-push 127s → bump 6s; always cold, image built twice) | build-push reuses smoke's gha cache |
| CI acceptance test job (warm / cold) | none — no tests ran in CI | rust-test + web-test (see PR) |
| Local full-suite runtime (uptime load) | Rust: 2m28s cold incl. instrumented compile, ~11s test exec (load 24–40) · web: 4.4s (load 32) | unchanged |
| Docker build (warm / cold) | — / 73s (PR scope never written; 0 CACHED layers) | cache-to on PR smoke + nightly warmer |
| Tests in acceptance / slow tier | 0 in CI (109 Rust + 4 web exist locally) | 109 Rust + 4 web / 1 slow (nightly) + 3 live (manual) |
| Async modules / total | web 0/1 (single module; n/a) · Rust: cargo runs tests in parallel threads | unchanged |
| Coverage — acceptance pass | not measured | Rust 30.15% lines (cargo-llvm-cov) · web: not measurable (see debt) |
| Coverage — full pass | not measured | ~= acceptance (ignored tests exercise one HTTP path) |
| Coverage gate | — | Rust 25% lines (`--fail-under-lines 25`) |

## Caching status
- GitHub Actions: cargo (Swatinem/rust-cache) ✅ · mix deps/_build ✅ · npm n.a. · .next/cache n.a. · PLT n.a. · develop-ref seeding ✅ (push: develop added; develop is the default branch)
- Docker: buildx gha cache ✅ (smoke now writes scope=web) · cache mounts ✅ (hex/rebar) · .dockerignore ✅ (added web/.dockerignore) · release-cache warmer ✅ (nightly.yml warm-docker-cache)

## Slow tests (tier: nightly)
| Test | Time | Why slow | Fix idea |
|---|---|---|---|
| tests/slow_provider_http.rs (ignored case) | ~90s | reproduces a ~60s server-side cut against a local mock | keep nightly; `MEDIA_TEST_SLOW_SECS` can shorten it |
| tests/live_qwen_routes.rs (3 ignored) | varies | live DashScope calls, spends credits | manual only (`cargo test --test live_qwen_routes -- --ignored`) |

## Test debt
| Item | Kind | Notes |
|---|---|---|
| web `mix test --cover` crashes | coverage gap | Hologram 0.10.1 compiler (`Hologram.Reflection.beam_defs/1`) gets `{:error, :non_existing}` for cover-compiled modules; `ignore_modules` for Pages/Layouts/Components does not help. Site has no coverage gate. |
| macOS MediaWorkbench Swift tests | coverage gap | `macos/MediaWorkbench/Tests` not run in CI (needs a macOS runner). |
| `make test` demo dry-run (`cargo run -- --dry-run demos/`) | coverage gap | not in CI; would add a second non-instrumented compile. Candidate for nightly. |
| Release gating | note | build-push now `needs: [smoke, rust-test, web-test]` (campaign rule: releases gated on backend tests). Before, the site released with no tests at all. |

## Nightly
`.github/workflows/nightly.yml` — cron `17 7 * * *` + `workflow_dispatch`: full Rust suite + offline slow tier, and a build-only docker cache warmer (scope=web).
