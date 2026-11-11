---
id: US-127
title: "See unavailable media types disabled with an install hint"
slug: gui-capability-disabled-types
personas: [P-008, P-004, P-002]
epic: "Media Workbench (GUI)"
priority: should-have
complexity: low
tags: [gui, capabilities, requests]
---

# US-127: See unavailable media types disabled with an install hint

## User Story

**As a** user without graphviz or puppeteer installed
**I want to** see unavailable types greyed out in the Type picker with the reason and the install command
**So that** I learn about missing tooling before I write a request, not after generation fails

## Acceptance Criteria

- **Given** graphviz is not installed
  **When** I open the Type picker in New Request
  **Then** the Diagram (graphviz) entry is disabled with a caption naming the missing binary

- **Given** I hover or focus the disabled entry
  **When** the hint renders
  **Then** it shows the install command (e.g. `brew install graphviz`) and is readable by VoiceOver

- **Given** I install the tool and reopen the picker
  **When** capabilities are re-probed
  **Then** the type becomes enabled without restarting the app

## Notes
v1 §5.3 capabilities panel. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
