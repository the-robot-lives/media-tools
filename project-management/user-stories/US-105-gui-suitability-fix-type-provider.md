---
id: US-105
title: "Fix a type or provider mismatch from the verdict"
slug: gui-suitability-fix-type-provider
personas: [P-008, P-002]
epic: "Media Workbench (GUI)"
priority: should-have
complexity: medium
tags: [gui, suitability, providers]
---

# US-105: Fix a type or provider mismatch from the verdict

## User Story

**As a** content creator who picked the wrong asset type
**I want to** use **Change type/provider** pre-filled from a `wrong-asset-type` or `provider-mismatch` verdict, or **Revise with feedback** to send the verdict back into prep
**So that** fixing the problem is one click, not a research task

## Acceptance Criteria

- **Given** the verdict is `provider-mismatch`
  **When** I click Change type/provider
  **Then** the picker opens pre-filled with the judge-suggested provider/model and only capability-available options are enabled

- **Given** the verdict is `needs-revision`
  **When** I click Revise with feedback
  **Then** a new draft is produced that includes the judge's reason as feedback, shown as a diff against the current prompt

- **Given** I apply either fix
  **When** the suitability card re-runs
  **Then** the new verdict replaces the old one, and the previous verdict remains in the request's history

## Notes
Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
