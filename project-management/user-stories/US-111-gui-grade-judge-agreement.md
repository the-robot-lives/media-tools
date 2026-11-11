---
id: US-111
title: "See when two judges disagree"
slug: gui-grade-judge-agreement
personas: [P-006]
epic: "Media Workbench (GUI)"
priority: could-have
complexity: medium
tags: [gui, grade, judge, agreement]
---

# US-111: See when two judges disagree

## User Story

**As a** design lead running agreement mode on important assets
**I want to** see both judges named and a disagreement marker on criteria where they differ
**So that** I spend my attention where machine grading is least reliable

## Acceptance Criteria

- **Given** agreement mode is on and both judges ran
  **When** I open the Grade drawer
  **Then** a judge chip names both backends/models

- **Given** the two judges' expected scores for a criterion differ by more than the configured threshold
  **When** I view that row
  **Then** a disagreement marker appears with both scores in its tooltip/accessibility value

- **Given** agreement mode is off
  **When** I view the drawer
  **Then** no disagreement markers are drawn

## Notes
Phase 4 of the decision-judge workstream. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
