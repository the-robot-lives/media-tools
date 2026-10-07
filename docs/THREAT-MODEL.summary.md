# Threat Model — Summary

Local developer tool (CLI + loopback test-lab server + macOS app); crown jewels are provider API keys and the operator filesystem. Static landing site has no security surface. Full model: [THREAT-MODEL.md](THREAT-MODEL.md)

**Trust boundaries**: internet→machine (remote config fetch, provider/eval egress) · prompt-file→engine (semi-trusted input drives filenames, attachment paths, renderer binaries, rendered markup) · browser→loopback lab (no auth) · engine→render subprocesses.

**Register (10 entries)**: 2 mitigated · 2 partial · 2 accepted · 4 open.

| ID | Sev | Issue | Status |
|----|-----|-------|--------|
| T-001 | High | Remote `media-tool.yaml` fetch allows plain `http://` — MITM redirects providers, harvests keys | Open |
| T-002 | Med | Graphviz `layout:` from prompt names the executable (`Command::new`) | Open |
| T-003 | Med | Output `filename:` joined without traversal checks — `../` writes escape output dir | Open |
| T-004 | Med | Test-lab HTTP API unauthenticated — drive-by local pages can generate/rewrite settings | Open |
| T-005 | Med | Puppeteer renders LLM HTML via `file://` + network — egress/local-file references possible | Partial |
| T-006 | Med | Lab `settings.json` stores api_key plaintext (gitignored; `env:` form preferred) | Partial |
| T-007 | Low | `MEDIA_EVAL_*`/`MEDIA_PREP_*` base-URL overrides route prompts/keys | Accepted |
| T-008 | Low | `MEDIA_DEBUG=1` dumps raw provider bodies | Accepted |
| T-009 | Low | Async provider polling | Mitigated (bounded) |
| T-010 | Low | Landing site spoofing | Mitigated (static, no auth) |

**Residual risk**: prompt files remain semi-trusted until T-002/T-003 close — running prompts from untrusted repos is the operator's call, like running a repo's Makefile.
