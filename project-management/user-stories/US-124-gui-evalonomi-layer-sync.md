---
id: US-124
title: "Sync style layers with evalonomi by checksum"
slug: gui-evalonomi-layer-sync
personas: [P-006]
epic: "Media Workbench (GUI)"
priority: could-have
complexity: high
tags: [gui, styles, evalonomi, sync]
---

# US-124: Sync style layers with evalonomi by checksum

## User Story

**As a** design lead whose house style lives in evalonomi
**I want to** pull house layers from evalonomi and push the project layer (`.media/rubric.yaml`) gated on checksum
**So that** the app and codemoshiach use the same rules, with no silent drift

## Acceptance Criteria

- **Given** connected
  **When** Styles opens
  **Then** House layers are fetched from evalonomi and labelled with source and version

- **Given** I edit the project layer
  **When** I save
  **Then** `.media/rubric.yaml` is written locally and PUT to evalonomi only if the server checksum matches the base I edited

- **Given** the server checksum moved
  **When** I push
  **Then** the push is refused with a compare view, never a silent overwrite

## Notes
Depends on Q1 and on the taxonomy v2 proposal filed with evalonomi. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
