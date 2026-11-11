---
id: US-115
title: "Define a project's copy voice"
slug: gui-style-copy-voice
personas: [P-008, P-006, P-002]
epic: "Media Workbench (GUI)"
priority: should-have
complexity: medium
tags: [gui, styles, voice]
---

# US-115: Define a project's copy voice

## User Story

**As a** marketer responsible for brand voice
**I want to** set tone on labelled scales, add do/don't example pairs and banned words
**So that** any text the generator writes sounds like us, and the judge can score it

## Acceptance Criteria

- **Given** the Copy voice facet
  **When** I adjust a tone scale
  **Then** each scale has labelled endpoints and a numeric value readable by VoiceOver

- **Given** I add a banned word
  **When** a judged attempt contains it
  **Then** the `brand-voice` criterion with chip `project:voice` reports it as a fail signal

- **Given** do/don't pairs exist
  **When** the prompt is composed
  **Then** the pairs appear as clauses in the composed prompt preview

## Notes
Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
