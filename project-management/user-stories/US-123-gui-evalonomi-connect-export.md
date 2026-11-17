---
id: US-123
title: "Connect evalonomi and export graded runs"
slug: gui-evalonomi-connect-export
personas: [P-006]
epic: "Media Workbench (GUI)"
priority: should-have
complexity: high
tags: [gui, accounts, evalonomi, oauth, otlp]
---

# US-123: Connect evalonomi and export graded runs

## User Story

**As a** design lead tracking quality across projects
**I want to** connect evalonomi.com and have each graded run exported as OTLP spans/logs with scores
**So that** evalonomi dashboards become our cross-project quality history

## Acceptance Criteria

- **Given** evalonomi is not connected
  **When** I view its Accounts row
  **Then** it explains why to connect (eval runs, traces, style layers) and offers Connect

- **Given** connected
  **When** a run is graded
  **Then** run → attempts → judge calls are exported as nested spans with per-criterion scores, and `synced_to[]` gains `evalonomi.com`

- **Given** an export fails
  **When** the queue retries
  **Then** the run is not blocked and the row shows the queue count and last error verbatim

## Notes
BLOCKED on ledger Q1: evalonomi has no OAuth server today (org API tokens + SSO only). Connect UX is spec'd assuming the Q1 outcome is per-user OAuth. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
