---
id: US-108
title: "Compare candidates in the Darkroom results view"
slug: gui-compare-candidates
personas: [P-006, P-002]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: medium
tags: [gui, results, darkroom, compare]
---

# US-108: Compare candidates in the Darkroom results view

## User Story

**As a** design lead judging four attempts at a pricing hero
**I want to** see the selected candidate large with a scored rail of all attempts and a context bar of run, provider/model, attempt, elapsed and cost
**So that** I can pick the survivor without leaving the screen to check what produced it

## Acceptance Criteria

- **Given** a run with several attempts
  **When** Results opens
  **Then** the hero is fit-to-window (never upscaled), the rail is left-aligned, and each thumbnail shows a 0-100 badge coloured only by the pass/fail gate

- **Given** I press `←`/`→`
  **When** selection moves
  **Then** the selected thumbnail gets a 2pt brass ring and the hero updates without the rail shifting

- **Given** a further attempt is still generating
  **When** Results is open
  **Then** the hero keeps showing the newest complete attempt and the rail shows a placeholder tile with progress

- **Given** the type has no preview
  **When** Results opens
  **Then** a metadata card with Reveal in Finder and Open With replaces the hero

## Notes
v1 §5.4; Darkroom tokens apply only here and in Grade. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
