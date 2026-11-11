---
id: US-112
title: "Override a grade with a reason"
slug: gui-override-grade
personas: [P-006, P-002]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: low
tags: [gui, grade, override]
---

# US-112: Override a grade with a reason

## User Story

**As a** design lead who disagrees with the machine grade
**I want to** override an attempt's grade and record why
**So that** human judgement wins while the record stays honest

## Acceptance Criteria

- **Given** a graded attempt
  **When** I choose Override…
  **Then** I must enter a new pass/fail (and optional score) and a non-empty reason before saving

- **Given** an override is saved
  **When** I view the attempt
  **Then** the badge carries an override glyph, and the drawer shows machine score, override and reason side by side

- **Given** an overridden run is exported or archived
  **When** the payload is produced
  **Then** it includes the machine grade, the override and the reason as separate fields

## Notes
v1 §5.4. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
