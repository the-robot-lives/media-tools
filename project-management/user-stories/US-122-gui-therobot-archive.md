---
id: US-122
title: "Archive promoted attempts to therobot.is"
slug: gui-therobot-archive
personas: [P-002, P-006]
epic: "Media Workbench (GUI)"
priority: should-have
complexity: medium
tags: [gui, accounts, therobot.is, archive]
---

# US-122: Archive promoted attempts to therobot.is

## User Story

**As a** creator who promotes the attempt I chose
**I want to** have each promoted attempt written as a memory (prompt, grade, provider, style layers), with per-workspace routing
**So that** the record of what worked builds up without extra work

## Acceptance Criteria

- **Given** connected and workspace routing is "My account"
  **When** I promote an attempt (`↩`)
  **Then** a memory write is queued and the run's `synced_to[]` gains `therobot.is` on success

- **Given** I am offline
  **When** I promote an attempt
  **Then** the write is queued, the Accounts row shows "offline queue n", and promotion is not blocked

- **Given** workspace routing is "Don't sync"
  **When** I promote an attempt
  **Then** nothing is sent

- **Given** I click Disconnect
  **When** the confirmation appears
  **Then** it states that remote data is not deleted

## Notes
Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
