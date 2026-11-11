---
id: US-119
title: "Compose styles offline"
slug: gui-offline-compose-fallback
personas: [P-002, P-006, P-001]
epic: "Media Workbench (GUI)"
priority: should-have
complexity: high
tags: [gui, styles, offline, degraded]
---

# US-119: Compose styles offline

## User Story

**As a** creator working on a plane
**I want to** keep composing and generating with my style layers when evalonomi is unreachable
**So that** network availability never blocks my work

## Acceptance Criteria

- **Given** evalonomi is unreachable
  **When** the composite is requested
  **Then** the app walks the ladder fresh cache → ETag revalidate → stale-while-revalidate → bundled fallback → local YAML via the Rust compose engine, and labels which rung served the result

- **Given** the local compose served the result
  **When** connectivity returns
  **Then** the app re-fetches and compares checksums, and surfaces any difference

- **Given** I am offline
  **When** I run the CLI on the same workspace
  **Then** it produces the same composite checksum as the app

## Notes
P-001 benefits through the CLI path only. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
