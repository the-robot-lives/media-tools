---
id: US-117
title: "Inherit, override, suppress or append rubric rules per layer"
slug: gui-rubric-layer-ops
personas: [P-006]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: high
tags: [gui, styles, rubric, layers]
---

# US-117: Inherit, override, suppress or append rubric rules per layer

## User Story

**As a** design lead tailoring the house rubric for one project
**I want to** set each rubric row's operation in a lower layer to inherit, override, suppress or append
**So that** projects adapt house standards without forking them

## Acceptance Criteria

- **Given** a rubric row inherited from House
  **When** I set it to override in Project
  **Then** weight, required flag, levels and fail signals become editable for this layer only and the House row is untouched

- **Given** I set a row to suppress in Prompt
  **When** the composite is built
  **Then** the criterion is absent from grading and shown struck-through with tag `Prompt` in the effective composite

- **Given** I append a criterion
  **When** the composite is built
  **Then** its anchor set has exactly 10 increasing levels, else the editor blocks saving with the offending level named

- **Given** ops are saved
  **When** the layer is serialised
  **Then** they use the `rubric-layer-ops.v1` op vocabulary (`replace | amend | insert_* | append_child | delete | suppress`)

## Notes
Reuses the codemoshiach/evalonomi contract verbatim; UI verbs map onto ops (override→replace/amend, suppress→suppress, append→append_child). Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
