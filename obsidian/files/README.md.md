---
tags: [file, docs]
---
# `README.md`

**Module:** [[project-root]] · **Language:** Markdown

## Purpose

The human-facing project document: prerequisites, setup, architecture summary,
known limitations, measured footprint, and the manual test checklist.

**Relationship to this vault:** the README is the *practical* entry point — how
to build and what to test. This vault is the *explanatory* one. Where they
overlap (architecture, limitations), the vault goes deeper. See [[MOC]].

## Sections

| Section | Vault equivalent |
| --- | --- |
| Prerequisites | [[build-and-config]] |
| Setup / commands | [[build-and-config]] |
| Pinned dependency (`vergen`) | [[Cargo.lock]] |
| Architecture tree | [[architecture]] |
| Two logins, two client IDs | [[auth-and-tokens]] |
| Playing a context, not a track | [[playback-and-connect]] |
| Why `Spirc` rather than raw `Player` | [[playback-and-connect]] |
| Premium enforcement | [[auth-and-tokens]] |
| Media keys | [[media_keys.rs]] |
| Known limitations | [[known-limitations]] |
| Measured footprint | below |
| Manual test checklist | below |

## Measured footprint

Recorded 2026-08-17 on the development machine, release build, both apps idle:

| | Rustify | Official client |
| --- | --- | --- |
| Processes | 7 (1 Rust + 6 WebView2) | 7 |
| Private (commit) | **157 MB** | 1442 MB |
| Working set | 347 MB | 818 MB |

> **The README flags this as not apples-to-apples**, and the caveat matters:
> Rustify was on the login screen while the official client was logged in
> with a home feed rendered. The ratio is an upper bound, not a result.

The breakdown that *is* clean: the Rust process is **5.9 MB private / 27.5 MB
working set**; WebView2 accounts for ~151 MB of the 157 MB. Further memory work
means shrinking the webview, not optimising Rust.

## Manual test checklist

15 numbered steps covering login, playback, **context continuation**,
transport, Connect inbound and outbound, library/search/queue, save/unsave,
drill-down, pagination, media keys, token expiry, restart, and resource usage.

Two are worth calling out as regression guards for real past bugs:

- **Step 4, context continuation** — let a track end and confirm the *next*
  track in the playlist plays. Guards the `load_context` fix ([[commands.rs]]).
- **Step 13, token expiry** — leave the app running over an hour, then load a
  playlist. A 401 means the refresher in [[auth.rs]] is broken.

## Inputs / outputs / side effects

None. Documentation only.

## Dependencies

**Describes:** the whole project
**Related:** every note in this vault

## Notable logic / gotchas

- **The prerequisites table records verified versions** for this machine
  (Rust 1.97.1, Node 24.x, WebView2 151.x), not minimums.
- **The status claims are honest and worth preserving:** the app compiles,
  links and launches, but login, playback, Connect and the Web API calls are
  **not yet exercised against a live Premium account**. See
  [[known-limitations]].

## See also

[[architecture]] · [[build-and-config]] · [[known-limitations]] ·
[[auth-and-tokens]] · [[playback-and-connect]] · [[project-root]] · [[MOC]]
