# merge-notes — media-tool (sep-1 branch sweep, 2026-09-01)

Media generation CLI/tools (the-robot-lives/media-tools). **Base chosen: `develop`
@ 735dd77 (= origin/main, 2026-08-31; develop==main).** `sep-1` tag on 735dd77.
Local develop synced ff to origin/develop; main checkout ends on develop.

## Review/merge sequence
1. **PR #3 (draft) — `develop-2` → `develop`**: provider retry/routing work
   (Gemini retries=9, generateContent routing, SKILL.md split) + WIP provider
   checkpoint. Keep draft until the checkpoint is done, then mark ready.
2. Nothing else queued; `develop` is the default/release branch.

## Skip/ignore list
- `feat/marketing-site` @ e299b80: already fully merged into develop — local +
  remote branches deleted in this sweep (work is live on the site).
- `mono-repo-dev` local-only @ 133084f: stale-name checkpoint but fully contained
  in `develop-2`; kept (protected name, no remote counterpart).
- Local `main` @ 607932d is an ancestor of origin/main (735dd77) — content
  already present; left as-is (main untouched per rules). The
  `staging/media-tool-site` worktree sits on it, untouched.

## Open PRs
| # | Branch | Base | What |
|---|--------|------|------|
| 3 (draft) | develop-2 | develop | Provider retry/routing work + WIP checkpoint |
