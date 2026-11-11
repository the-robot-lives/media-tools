---
id: US-116
title: "Set element construction rules"
slug: gui-style-element-construction
personas: [P-006]
epic: "Media Workbench (GUI)"
priority: could-have
complexity: medium
tags: [gui, styles, construction]
---

# US-116: Set element construction rules

## User Story

**As a** design lead with a component system
**I want to** define construction rules (buttons, cards, icon stroke, corner radius, grid) as a checklist with examples
**So that** UI-like generated assets match our components

## Acceptance Criteria

- **Given** the Element construction facet
  **When** I add a rule
  **Then** it has a short name, a value, an optional example image and an "also grade" toggle

- **Given** "also grade" is on
  **When** the composite is built
  **Then** the rule appears as a `check` node in the rubric section

- **Given** a rule references a missing snippet or palette role
  **When** Styles validates
  **Then** the reference is named in a non-fatal warning on that rule

## Notes
Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
