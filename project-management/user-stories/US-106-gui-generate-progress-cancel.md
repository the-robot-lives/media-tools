---
id: US-106
title: "See generation progress and cancel it"
slug: gui-generate-progress-cancel
personas: [P-002, P-008, P-007]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: medium
tags: [gui, generate, progress, cancel]
---

# US-106: See generation progress and cancel it

## User Story

**As a** creator generating several attempts that take minutes
**I want to** see per-attempt progress, elapsed time and estimated cost, and cancel with `⌘.` or a visible button
**So that** a slow or wrong paid operation never traps me

## Acceptance Criteria

- **Given** a generation is running
  **When** I look at the Runs queue rail
  **Then** each job row shows run name, provider, attempt n/m, a progress bar or indeterminate state, elapsed time and a cancel control

- **Given** a generation is running
  **When** I press `⌘.`
  **Then** the in-flight attempt is cancelled at the core, the row shows "Cancelled", and completed attempts are kept

- **Given** a provider returns 4xx/5xx, quota or timeout
  **When** the attempt fails
  **Then** the provider's own error message is shown verbatim on the attempt

- **Given** VoiceOver is running
  **When** progress changes
  **Then** progress is announced at no more than one update per 10s and on completion/failure, not per percent tick

## Notes
v1 §5.1 Generate row; §3.3 progress problem. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
