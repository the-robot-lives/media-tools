---
id: US-120
title: "Configure and test a judge backend"
slug: gui-judges-config-test
personas: [P-006, P-002]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: medium
tags: [gui, settings, judges]
---

# US-120: Configure and test a judge backend

## User Story

**As a** design lead setting up automated grading
**I want to** pick a judge backend (Alibaba default, Jev), model, key source and base URL, and run a one-question Test
**So that** I know grading works before I rely on it

## Acceptance Criteria

- **Given** Settings → Judges
  **When** I select Alibaba
  **Then** model defaults to `decision-model-preview` and key source shows env / keychain / dc status without revealing the value

- **Given** Alibaba is selected and Base URL is empty
  **When** I view the tab
  **Then** a blocking state explains that a workspace URL is required, and Test is disabled

- **Given** a complete config
  **When** I click Test
  **Then** a one-question probe runs and shows pass/fail, latency in ms and the returned answer with its probability

- **Given** the probe returns 401/404
  **When** the result renders
  **Then** the backend's own error text is shown verbatim

- **Given** default settings
  **When** I view Agreement mode
  **Then** it is off

## Notes
Never print key values; key spec mirrors v1 §4.1. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
