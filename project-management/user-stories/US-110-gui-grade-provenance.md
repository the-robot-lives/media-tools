---
id: US-110
title: "Trace a criterion back to the style rule that created it"
slug: gui-grade-provenance
personas: [P-006, P-002]
epic: "Media Workbench (GUI)"
priority: should-have
complexity: medium
tags: [gui, grade, provenance, styles]
---

# US-110: Trace a criterion back to the style rule that created it

## User Story

**As a** design lead whose rubric comes from house and project layers
**I want to** see a provenance chip per criterion (`house:palette`, `project:voice`, `prompt`) and click it to open that rule
**So that** a bad score can be fixed at its source instead of patched per prompt

## Acceptance Criteria

- **Given** a graded attempt
  **When** I view a criterion row
  **Then** it shows exactly one provenance chip equal to `Composite.provenance[node_id]` for that criterion

- **Given** I click a provenance chip
  **When** navigation happens
  **Then** Styles opens with that layer and facet selected and the rule highlighted (deep link `mediaworkbench://style/<layer>/<facet>`)

- **Given** a criterion came from the prompt's own eval block
  **When** I view its chip
  **Then** the chip reads `prompt`

## Notes
Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
