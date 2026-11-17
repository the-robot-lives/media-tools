---
id: US-107
title: "A generation keeps running when the window closes"
slug: gui-generation-survives-window-close
personas: [P-002, P-006]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: high
tags: [gui, generate, durability]
---

# US-107: A generation keeps running when the window closes

## User Story

**As a** creator who closes windows by habit
**I want to** close the window (or the last window) mid-generation and reattach to the same run later
**So that** paid API calls are never lost to a UI event

## Acceptance Criteria

- **Given** a generation is in flight
  **When** I close the window
  **Then** the job continues in the core and the Dock/menu-bar indicates work in progress

- **Given** the job continued with no window open
  **When** I reopen the window
  **Then** the Runs rail reattaches to the same handle and shows current progress, not a restarted job

- **Given** I quit the app with jobs running
  **When** the quit is attempted
  **Then** a confirmation names the number of running jobs and offers Keep running / Cancel jobs / Quit after finish

## Notes
Hard rule from v1 §5.1. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
