---
id: US-101
title: "Describe a needed asset without writing YAML"
slug: gui-request-intake
personas: [P-008, P-002, P-006]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: medium
tags: [gui, requests, intake]
---

# US-101: Describe a needed asset without writing YAML

## User Story

**As a** growth marketer who never writes `.media.prompt` files
**I want to** open a New Request sheet (`⌘N`) and describe my need in one sentence plus type, usage, style stack, references and quality
**So that** I can get a generated asset without learning the prompt schema

## Acceptance Criteria

- **Given** the app is open on any section
  **When** I press `⌘N`
  **Then** a sheet titled New Request opens with labelled fields Intent, Type, Usage, Style, References, Quality, and focus starts in Intent

- **Given** Intent is empty
  **When** I look at the primary button
  **Then** **Draft prompt** is disabled and Intent shows a persistent label, not placeholder-only

- **Given** the Requests list is empty
  **When** I open the Requests section
  **Then** the empty state shows "Describe what you need" and three clickable example intents that pre-fill the sheet

- **Given** I submit a request
  **When** drafting completes
  **Then** a request record (`R-<n>`) exists with intent, type, usage, constraints and status, and an ordinary `.media.prompt` file is written to the workspace with `request_id` lineage

## Notes
Every request produces a plain `.media.prompt` on disk, so the CLI remains the source of truth. P-001 (Mira) avoids GUIs; this story must not introduce GUI-only state she would miss from the terminal. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
