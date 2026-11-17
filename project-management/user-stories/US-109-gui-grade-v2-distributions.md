---
id: US-109
title: "Read per-criterion score distributions in Grade v2"
slug: gui-grade-v2-distributions
personas: [P-006]
epic: "Media Workbench (GUI)"
priority: must-have
complexity: high
tags: [gui, grade, judge, distributions]
---

# US-109: Read per-criterion score distributions in Grade v2

## User Story

**As a** design lead who has to tell a confident 70 from a coin-flip 70
**I want to** see each criterion's 10-level probability distribution with an expected-score tick next to its weighted score
**So that** I know which scores to trust and which to look at myself

## Acceptance Criteria

- **Given** an attempt graded by the decision model
  **When** I open the Grade drawer (`g`)
  **Then** each criterion row shows name, weight × expected score, and a 10-cell strip whose cell shading is proportional to P(level), with a tick at the expected score

- **Given** one criterion's distribution is bimodal
  **When** I view the drawer
  **Then** its strip visibly shows two separated clusters and the row is marked low-confidence (entropy above a documented threshold)

- **Given** criteria are listed
  **When** I read the order
  **Then** failed `required_pass` criteria are first, then triggered `reject_if`, then the rest by weight descending

- **Given** `reject_if` rules exist
  **When** I view the drawer
  **Then** each rule shows P(triggered)

- **Given** the image was judged via observe→decide
  **When** I view the drawer
  **Then** a "described" badge says the judge saw a description, not pixels

## Notes
v2 §3 R3. The 320pt drawer width from v1 §5.4 must still fit name + strip + chip; see spec tension in the report. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
