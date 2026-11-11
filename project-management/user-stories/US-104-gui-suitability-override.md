---
id: US-104
title: "Generate anyway, with the override recorded"
slug: gui-suitability-override
personas: [P-006, P-002]
epic: "Media Workbench (GUI)"
priority: should-have
complexity: low
tags: [gui, suitability, override, audit]
---

# US-104: Generate anyway, with the override recorded

## User Story

**As a** design lead who sometimes knows better than the judge
**I want to** click **Generate anyway** on a non-ready verdict and give a reason
**So that** I can proceed, while the run honestly records that it went ahead against the judge

## Acceptance Criteria

- **Given** the verdict is not `ready`
  **When** I click Generate anyway
  **Then** a reason field is required before the run starts

- **Given** the override was confirmed
  **When** the run is created
  **Then** the run record stores the verdict, its probabilities, the override flag and the reason string

- **Given** a run started under a suitability override
  **When** I view it in Runs or Results
  **Then** it carries a distinct override glyph and is never indistinguishable from a run that passed suitability

## Notes
Mirrors v1 grade-override semantics. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
