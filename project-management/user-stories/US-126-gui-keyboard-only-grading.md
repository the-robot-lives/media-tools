---
id: US-126
title: "Grade and promote with the keyboard only"
slug: gui-keyboard-only-grading
personas: [P-007, P-006]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: medium
tags: [gui, accessibility, keyboard, grade]
---

# US-126: Grade and promote with the keyboard only

## User Story

**As a** reviewer working through many candidates
**I want to** move, compare, grade, override, reject, promote and revise without a mouse
**So that** repetitive judging stays fast and accessible

## Acceptance Criteria

- **Given** Results opens
  **When** nothing is clicked
  **Then** focus is on the candidate rail and `←`/`→` move selection

- **Given** a candidate is selected
  **When** I use `g`, `space`, `x`, `↩`, `r`
  **Then** the drawer toggles, fit/100% toggles, reject, promote and revise each act on the selected candidate

- **Given** the Override sheet is open
  **When** I use Tab and Return
  **Then** every field is reachable and saving works without a pointer

- **Given** any icon-only control
  **When** it receives focus
  **Then** it has an accessibility label and tooltip

## Notes
Key map: v1 §5.4 plus v2 §6. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
