---
id: US-125
title: "Hear distribution strips with VoiceOver"
slug: gui-voiceover-distribution-strips
personas: [P-007]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: medium
tags: [gui, accessibility, voiceover, grade]
---

# US-125: Hear distribution strips with VoiceOver

## User Story

**As a** visually impaired accessibility consultant
**I want to** have each criterion's distribution strip expose a meaningful accessibility value
**So that** I get the same confidence signal sighted users get from the shading

## Acceptance Criteria

- **Given** VoiceOver is on and the Grade drawer is open
  **When** focus lands on a criterion row
  **Then** VoiceOver reads name, weight, expected score, pass/fail and a confidence summary, e.g. "palette, weight 0.30, expected 86, confident, mode 9"

- **Given** a bimodal distribution
  **When** focus lands on that row
  **Then** VoiceOver says "split" and names the two peak levels

- **Given** I interact with the strip
  **When** I use VO arrow keys
  **Then** each of the 10 cells reads its level and probability as a percentage

- **Given** pass/fail and verdict colours
  **When** checked with colour filters (Increase Contrast, greyscale)
  **Then** every state is distinguishable by text or glyph, not colour alone

## Notes
Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
