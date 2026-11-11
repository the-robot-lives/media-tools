---
id: US-114
title: "Define a house or project palette with WCAG contrast checks"
slug: gui-style-palette-layer
personas: [P-006, P-007]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: medium
tags: [gui, styles, palette, wcag]
---

# US-114: Define a house or project palette with WCAG contrast checks

## User Story

**As a** design lead owning brand colour
**I want to** edit palette swatches with roles, see live WCAG contrast ratios for role pairs, and flag must-use/avoid colours
**So that** generated assets are on-brand and readable, and the rules grade themselves

## Acceptance Criteria

- **Given** the Palette facet of a layer
  **When** I add or edit a swatch
  **Then** it requires a role (canvas, ink, accent, …) and a hex value

- **Given** two or more swatches exist
  **When** I view Contrast pairs
  **Then** each defined pair shows its WCAG 2.2 ratio to one decimal and an AAA / AA / Large only / Fail label, recomputed on every edit

- **Given** I mark a colour must-use or avoid
  **When** I save
  **Then** the layer's compiled output contains a prompt clause and a rubric criterion for that flag, visible in the effective composite

- **Given** no house style exists
  **When** I open Styles
  **Then** a starter template is offered rather than an empty editor

## Notes
Facets compile to rubric sections (v2 §5b). Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
