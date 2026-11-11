---
id: US-118
title: "View the effective composite with provenance"
slug: gui-effective-composite
personas: [P-006, P-002]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: medium
tags: [gui, styles, composite, provenance]
---

# US-118: View the effective composite with provenance

## User Story

**As a** creator puzzled by why a prompt rendered a certain colour
**I want to** toggle the effective composite (`⌘E`) for a sample prompt and see every rule tagged with its winning layer and override trail
**So that** I can explain any outcome back to the rule and layer that caused it

## Acceptance Criteria

- **Given** Styles is open
  **When** I press `⌘E` and pick a sample prompt
  **Then** a table lists every composed rule with value, winning-layer tag and trail (e.g. `House → Project`)

- **Given** the composite came from evalonomi
  **When** I view the header
  **Then** it shows the source and checksum, and a mismatch with the local compose is flagged

- **Given** a layer has invalid YAML
  **When** composite is requested
  **Then** the error names the layer and line and the composite is not shown as if valid

## Notes
Equivalent to the evalonomi `preview`/`render`; no app-specific resolution rules. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
