---
id: US-113
title: "Revise from a candidate and see whether it helped"
slug: gui-revise-lineage
personas: [P-002, P-006]
epic: "Media Workbench (GUI)"
priority: should-have
complexity: medium
tags: [gui, revise, lineage]
---

# US-113: Revise from a candidate and see whether it helped

## User Story

**As a** creator iterating on a prompt
**I want to** press `r` on a candidate to start a revision and see parent → child prompts as a diff with each grade
**So that** I can tell whether a revision improved the result

## Acceptance Criteria

- **Given** a candidate is selected in Results
  **When** I press `r`
  **Then** Revise opens with that attempt as parent and the parent prompt pre-loaded

- **Given** a child run has completed
  **When** I view lineage
  **Then** parent and child prompts are shown as a diff with margin marks and each node shows its weighted grade and per-criterion delta

- **Given** a lineage chain of 3+ runs
  **When** I view it
  **Then** the chain is navigable by keyboard and the best-graded node is marked

## Notes
v1 §5.2 screen 7; Atelier borrow (revision marks in margin). Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
