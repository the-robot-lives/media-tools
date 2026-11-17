---
id: US-103
title: "Check a prompt's suitability before spending money"
slug: gui-suitability-check
personas: [P-008, P-006, P-002]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: high
tags: [gui, suitability, judge, pre-flight]
---

# US-103: Check a prompt's suitability before spending money

## User Story

**As a** marketer who does not know which provider suits which asset
**I want to** run a suitability check (`⌘K`) that tells me whether the prompt is ready, needs revision, is the wrong asset type, or mismatches the provider
**So that** I don't pay for generations that were doomed by the prompt

## Acceptance Criteria

- **Given** a drafted prompt and a configured judge
  **When** I press `⌘K`
  **Then** the Suitability card shows the winning verdict and a probability bar with all four outcomes `ready | needs-revision | wrong-asset-type | provider-mismatch`, whose values sum to 100% ±1

- **Given** the check completes
  **When** I read the card
  **Then** Specificity and Constraint fit each render as a 10-cell distribution strip with an expected-score value

- **Given** the check completes
  **When** I read the card
  **Then** a judge chip names backend and model (e.g. `alibaba · decision-model-preview`) and provenance chips list the style layers composed into the prompt; clicking a chip opens Styles at that layer

- **Given** no judge is configured
  **When** I open the Suitability card
  **Then** it reads "Unchecked" with a Set up judge action, and nothing on screen implies the prompt is ready

- **Given** the judge returns 4xx/5xx
  **When** the error state renders
  **Then** the judge's own error text is shown verbatim and Generate remains available but is labelled as unchecked

## Notes
v2 §3 R2; state matrix row "Suitability". Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
