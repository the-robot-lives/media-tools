---
id: US-102
title: "Review an LLM-drafted prompt as a diff and accept part of it"
slug: gui-review-drafted-prompt-diff
personas: [P-002, P-006, P-008]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: medium
tags: [gui, prepare, diff, author-assist]
---

# US-102: Review an LLM-drafted prompt as a diff and accept part of it

## User Story

**As a** technical writer with an existing prompt for an article hero
**I want to** see the LLM's proposed prompt as a diff against my file (or against empty) and accept all, some hunks, or none
**So that** I stay in control of what changes and can review it like a document instead of a chat transcript

## Acceptance Criteria

- **Given** a request has finished drafting
  **When** the Prepare view opens
  **Then** the left pane shows the proposed `.media.prompt` as a line diff with added/removed gutters and **Accept**, **Accept partial**, **Reject** actions

- **Given** I choose Accept partial
  **When** I toggle individual hunks and confirm
  **Then** only the selected hunks are written to disk and the file re-validates against schema 0.4

- **Given** drafting is in flight
  **When** I watch the Prepare view
  **Then** the diff streams in progressively and `⌘.` cancels drafting within 1s, leaving the file unchanged

- **Given** the prep LLM call fails
  **When** the error state renders
  **Then** the provider's own error text is shown verbatim, and an "Edit YAML manually" fallback is offered

- **Given** no prep profile is configured
  **When** I open Prepare
  **Then** the degraded state offers manual YAML editing instead of a dead button

## Notes
Extends v1 §5.2.3 Author-assist. Not a chat window. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
