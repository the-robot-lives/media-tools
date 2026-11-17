---
id: US-121
title: "Connect therobot.is and recall what worked before"
slug: gui-therobot-connect-recall
personas: [P-002, P-008, P-006]
epic: "Media Workbench (GUI)"
priority: should-have
complexity: high
tags: [gui, accounts, therobot.is, oauth, recall]
---

# US-121: Connect therobot.is and recall what worked before

## User Story

**As a** creator who keeps rediscovering good prompts
**I want to** connect therobot.is with OAuth and see past wins for this project and type while drafting
**So that** new requests start from what already worked

## Acceptance Criteria

- **Given** Settings → Accounts, therobot.is not connected
  **When** I click Connect
  **Then** the system browser opens an OAuth 2.1 PKCE flow with a loopback redirect `http://127.0.0.1:<port>/callback` and scopes `memory:read memory:write`, while the row shows "Waiting for browser"

- **Given** authorisation succeeds
  **When** I return to the app
  **Then** the row shows Connected, the account, scopes and last sync, and the token is stored in the macOS Keychain

- **Given** connected
  **When** I open New Request with a type
  **Then** a recall row shows the count of past wins for project + type and opens them on click

- **Given** the token has expired
  **When** the app syncs
  **Then** the row shows a Reconnect state, not a silent failure

## Notes
v2 §5: server-side ready. Source: `.tmp/design/media-workbench-v2.md`; v1 `docs/design/media-workbench.md`.
