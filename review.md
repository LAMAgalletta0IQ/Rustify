> This document supersedes the review committed at `a3ec85f` (2026-09-02). That
> version is not deleted — `git show a3ec85f:review.md` still shows it — this
> file follows this project's own convention (see `CLAUDE.md`'s reversed-decision
> blockquotes) of updating a living document in place rather than forking a new
> one. **Nine "Phase" commits between that review and this one fixed essentially
> everything it flagged at P1 and P2.** This pass verifies which of those fixes
> actually hold, re-audits the code written since, and corrects one thing the
> prior review got wrong: its own P0-2 licensing recommendation is no longer safe
> to follow (see P0-2 below — a decision made three days *after* that review
> changed the facts on the ground).

# Project Analysis Review: Rustify

## 1. Review Summary

**Overall health: very good, and better than three weeks ago.** The 2026-09-02
review found excellent Rust underneath a project with no CI, no license, no
tests worth trusting, and an account-risking header spoof. Since then, nine
sequential "Phase" commits fixed the jams concurrency races, encrypted
`tokens.json` at rest with DPAPI, split the 2,128-line `commands.rs` into
domain modules with a compile-time-adjacent registration-parity test, added
`ts-rs`-generated types, wired up `cargo-deny`/clippy/fmt, split the worst
frontend component, and wrote the feature-tier policy the last review asked
for — all independently re-verified in this pass, not taken on faith.

I ran the actual gates rather than reading about them: `cargo check
--no-default-features` (clean), `cargo fmt --check` (clean), `cargo clippy
--no-default-features -- -D warnings` (clean, zero warnings), `cargo test
--no-default-features --lib` (142 passed, 0 failed, 4 live-only ignored),
`cargo deny check` (advisories/bans/licenses/sources all `ok`), `npm run check`
(0 errors, 213 files), `npm run build` (succeeds), `npm test` (10 passed). Four
subagent passes independently re-read every backend module and every frontend
file looking for anything new. **The result: no new P0 or P1 defect exists
anywhere in the code.** That is a genuinely good outcome and I am reporting it
straight rather than manufacturing findings to fill out a severity rubric.

**What is still actually wrong is one decision, not code** — a second one was
code, and got fixed in this same session once flagged:

1. **The project now contains GPLv3-derived logic and still has no LICENSE —
   and the previous review's own fix for that ("add MIT or Apache-2.0") would
   now be actively wrong.** My memory of this project records a 2026-09-05
   decision — three days after the last review shipped — to port two features
   (the crossfade scheduler, DJ narration audio injection) from `go-librespot`
   (GPL-3.0), on the reasoning that GPL's copyleft conditions attach at
   *distribution*, not private modification, and this project is undistributed.
   I verified that reasoning still holds today: the GitHub remote
   (`LAMAgalletta0IQ/Rustify`) returns HTTP 404 unauthenticated right now, i.e.
   still private. But **nothing in the actual repository says any of this** —
   grepping `README.md` and `CLAUDE.md` for `GPL`/`go-librespot` returns zero
   hits. The single most legally load-bearing fact about this codebase exists
   only in an AI assistant's memory, not in the project it's about. One
   accidental "make repo public" click, one shared installer, or one future
   session that doesn't know this history and "helpfully" adds an MIT
   `LICENSE` — which is exactly what the *prior review* recommended — converts
   an intentional, currently-valid legal position into a real GPL violation
   with no warning to anyone. **This finding is deliberately left as
   documentation-only in this review** — the maintainer asked not to have the
   licensing/GPL language written into the repo during this session, so P0-2
   below states the problem and the fix but nothing was applied.

This is unchanged from three weeks ago in the sense that it was already
knowable, and worse than the last review realized, because the facts
underneath it changed three days after that review shipped and nothing caught
the resulting mismatch until now.

**Fixed during this session** (small, safe, applied and re-verified against
the full gate suite after each change — see §6 for detail): the `pathfinder.
rs` client-impersonation headers (the last review's P0-1, and this review's
original P0-1) are removed — `queryArtistOverview` now sends the same honest
`WebPlayer`/`open.spotify.com` identity as every other Pathfinder operation,
with no more fabricated `Spotify/1.x` User-Agent or `xpui.app.spotify.com`
Origin/Referer anywhere in the file; **live verification of what this does to
the artist-overview feature is still outstanding and cannot be done from this
environment** (see the updated P0-1 finding below for exactly what to check
and what already degrades gracefully if it 403s); a GitHub Actions CI
workflow now runs all seven gates on every push (the single cheapest,
highest-leverage item the last review flagged and the one thing that *didn't*
get done in the otherwise-thorough Phase 1–9 cleanup); an unbounded
fixed-interval retry-forever loop in `remote_state.rs` and `friends.rs` now
backs off like the rest of the codebase's watchdog logic; DJ narration's
playback thread now stops within ~100ms of a logout/session-replacement
instead of trailing off for up to the clip's remaining duration, with three
new fast unit tests proving the polling logic rather than just the change
description; five icon-only transport buttons in `PlayerBar.svelte` gained
`aria-label`s matching their siblings; the `is_builder_not_available`
dealer-race classifier, previously duplicated in `remote_state.rs`/
`friends.rs` and re-inlined a third time in `jams_bridge.rs`, is now one
shared function in a new `dealer_util.rs`; `CLAUDE.md`'s test count and "no
CI" claim were corrected to match reality (now 146 tests, after this
session's additions).

**Readiness: personal/internal use, same as before, and more solidly so than
three weeks ago.** It is not beta-ready for other people, and it now has a
sharper reason not to be: the GPL exposure attaches the instant it is. That is
fine — the honest move remains what the last review said: pick "personal
project, never distributed" in writing, and stop treating distribution
readiness as a backlog item.

**Biggest strength:** the documentation discipline held up under an actual
follow-through cycle. It is one thing to write "Until 2026-08 this was wrong"
blockquotes; it is another to come back three weeks later and find nine
commits that did the unglamorous work the documentation promised. That
happened here, verified against the actual diff, not the commit messages.

**Biggest risk:** unchanged in kind, sharper in degree — the private-API
surface and the account/legal exposure it creates are still concentrated in
exactly two places, and one of them is now a landmine with a shorter fuse than
anyone examining only the code would see.

---

## 2. Scope and Inputs Reviewed

| Reviewed | Detail |
|---|---|
| Prior review | `review.md` at commit `a3ec85f` (2026-09-02), read in full (1,636 lines), to avoid re-deriving or duplicating already-fixed findings |
| Git history | `git log`, `git diff --stat a3ec85f..HEAD` (18 files, +1,072/-145 across `src-tauri/src` and `src`), `git show --stat` on the review commit itself |
| Rust backend | All 51 files, ~20,038 lines. Read in full myself: `auth.rs`, `commands/session.rs`, `lib.rs`, `state.rs`, `Cargo.toml`, `deny.toml`, `remote_state.rs` (before and after my own edit), `friends.rs` (targeted sections), `spotify/pathfinder.rs`, `jams/spclient.rs` (targeted), `telemetry.rs` (header), `player.rs` (targeted). The remaining ~40 files were read in full by four parallel subagent passes, each independently briefed on what the prior review already covered so they would not re-report fixed issues, and each explicitly instructed to say so plainly if a file was clean |
| Frontend | ~8,800 lines across 24 `.svelte` files + `store.svelte.ts`/`api.ts`/`types.ts`/`images.ts`, all read by a dedicated subagent pass; `PlayerBar.svelte` re-verified directly by me before and after editing it |
| Config/tooling | `Cargo.toml`, `Cargo.lock` (via `cargo deny check`'s dependency graph), `deny.toml`, `tauri.conf.json`, `capabilities/default.json`, `package.json`, `.env.example`, `.gitignore` — all read directly |
| Docs | `CLAUDE.md` (given in full at session start, and the live file re-read for the diff already pending in the working tree), `README.md` (prerequisites, known-limitations, footprint, manual-checklist sections read directly) |
| Executed, not just read | `cargo check --no-default-features`, `cargo fmt --check`, `cargo clippy --no-default-features -- -D warnings`, `cargo test --no-default-features --lib`, `cargo deny check`, `npm run check`, `npm run build`, `npm test`, `npm audit` (both `--omit=dev` and full), a live unauthenticated GitHub API check on the `origin` remote |
| Memory | This assistant's own persisted memory of a 2026-09-05 decision (three days after the prior review) to port GPL-3.0 `go-librespot` logic into this codebase — treated as a lead to verify, not a fact to assert; independently confirmed against current source (`player.rs:512-515`, `narration.rs:37-39`, `remote_state.rs:286-288` all reference "go-librespot's own" behavior by name) and against the GitHub remote's current visibility |

| Not reviewed / cannot verify | Why it matters |
|---|---|
| **Runtime/live behaviour** | I did not launch the app and have no live Spotify Premium credentials in this environment. No claim below about live login, playback, Connect handoff, or DJ narration audio is empirically verified beyond what the automated test suites cover. This matters most for P0-1: the fix's own acceptance criteria require live re-testing that I cannot perform |
| Whether `queryArtistOverview` would actually work with honest headers | Requires a live account. Everything here is a code-level finding, not a measured outcome |
| `audio/mod.rs`'s DSP numerics past what its unit tests assert | Structural read only |
| `.env` contents | Gitignored, correctly; not read |
| Full content of the 111+ note `obsidian/` vault | Spot-checked structure only; not read note-by-note this pass (it was in the last review) |
| Whether the four subagent passes' claims are individually airtight | I independently re-verified every finding they returned that I planned to act on or report as a named finding (grepped/read the exact lines myself) before including it below; I did not re-derive the "nothing found" conclusions for files where they reported a clean result, beyond spot checks |

**Assumptions carried forward from the prior review, re-confirmed:** (a)
"Personal use" is the actual intent — reconfirmed by the GPL decision itself,
which is only coherent under that assumption; (b) Windows 11 is the only
target; (c) there is one maintainer.

---

## 3. Overall Assessment

| Area | Score | Summary |
|---|---:|---|
| Product clarity | 8/10 | Unchanged flows, now with a written feature-tier policy in `CLAUDE.md` closing the last review's P1-6 |
| Architecture | 9/10 | `commands.rs` split into domain modules with a registration-parity test; `ts-rs` closes the Rust↔TS drift risk. Verified structurally and via a clean `cargo check` |
| Code quality | 9/10 | Held the bar across ~1,100 new lines (`narration.rs`, `player.rs` DJ sequencing, loudness wiring) per independent subagent re-review; zero new non-test panic sites found |
| Security | 6/10 | DPAPI token encryption is real and tested (closes the old P1-5). Undermined by two still-open, now-worse-understood items: client impersonation (account-ban risk) and an undocumented GPL liability (legal risk) |
| Performance | 8/10 | No changes found; pooled HTTP client, bounded caches, gated poller all still in place |
| Testing | 7/10 | Real jump from 4/10: 106→142 Rust tests (added exactly the previously-untested dangerous logic: token merge, grant rejection, position anchoring, session generation, jams concurrency), Vitest added with 10 genuine (not superficial) tests, all gates now enforced in CI. Still zero true integration/E2E coverage — that gap is real and, per `CLAUDE.md`'s own honest framing, likely permanent for a project this shape |
| Reliability | 8/10 | Jams races and detached-task leaks fixed and re-verified. One inconsistency found and fixed in this session (unbounded retry in `remote_state.rs`/`friends.rs`) |
| Documentation | 8/10 | Still exceptional in density and honesty — and still missing the one fact that matters most for legal safety (GPL provenance), which exists nowhere in the repo itself. Docked specifically for that gap, not for volume or quality elsewhere |
| DevOps / Deployment | 4/10 | CI now exists (added this session). Still no LICENSE, still a placeholder `identifier`, still no signing, still no updater — and the GPL exposure now makes "just add a permissive license" actively wrong, raising the bar for ever resolving this cleanly |
| Maintainability | 7/10 | The structural hazards (bloated `commands.rs`, hand-mirrored types, undocumented tiering) are fixed. The root cause of the old 6/10 — feature count against a hostile private API — hasn't shrunk (narration was *added*, reversing a prior "defer indefinitely" call, for stated and documented reasons) but is now honestly tiered per `CLAUDE.md`'s Feature Tiers section |

**Total risk: MEDIUM, concentrated in two specific, well-understood, actionable
decisions — not diffuse.** That is a materially better shape than the last
review's "HIGH, concentrated outside the source code" verdict. The engineering
execution and the project-management execution have converged: someone did the
unglamorous Phase 1–9 work in full. What's left isn't a backlog, it's two
choices sitting unmade.

---

## 4. Strengths

**1. The Phase 1–9 remediation actually happened and actually holds.** This is
the strength worth naming first because it's rare: a prior audit's
recommendations were implemented, not just acknowledged. I independently
re-verified rather than trusted: `jams_bridge.rs` now has `impl Drop for
JamController` aborting its forward task (closing the old P1-4); `ensure_jams`
now holds its write guard across the build via a `get_or_build` helper with a
concurrency regression test (closing P1-3); `tokens.json` round-trips through
real Windows DPAPI with a migration path and 9 dedicated tests including a
plaintext→encrypted migration test (closing P1-5); `commands.rs` is gone,
replaced by `commands/{session,playback,library,discovery,jams}.rs` plus a
`registration_parity_tests` module that reads `lib.rs` and `api.ts` as text
and asserts they agree (closing P1-7); `src/lib/generated/*.ts` is real,
`ts-rs`-produced output, not hand-written (closing P2-4); `NowPlaying.svelte`
dropped from 1,166 to 839 lines via three real extractions (partial progress
on P2-2); `cargo fmt --check` and `cargo clippy -- -D warnings` both pass
clean today (closing P2-6); `cargo deny check` reports `advisories ok, bans
ok, licenses ok, sources ok` (closing P2-3); `tauri-plugin-mcp-bridge` is now
`optional = true` behind a Cargo feature (closing P2-1); `withGlobalTauri` is
`false` (closing P2-5); the README prerequisites table states minimums, not
one machine's installed versions, and the stale footprint comparison table is
gone, replaced by an honestly-scoped paragraph (closing P3-1 and P3-3);
`BACKUP/` is gone (closing P3-2); `JamSession::events()` no longer panics on a
second call (closing P3-4); the version is `0.2.0` everywhere, not `0.1.0`
(closing P3-5).

**2. Failure-mode documentation continues at the point of the fix, not just in
the parts written before the last review.** `auth.rs`'s DPAPI functions carry
the same standard as everything the last review praised: `load_stored_tokens`
explains exactly why a failed `unprotect` falls through to a plaintext parse
attempt before giving up, and a dedicated test (`a_plaintext_tokens_file_is_
migrated_to_encrypted_storage_on_load`) proves the migration path, not just
asserts it in a comment.

**3. Concurrency discipline generalized rather than staying a one-off fix.**
The `session_generation` watchdog pattern, the "every background task on
`SpotifySession` must be explicitly `.abort()`ed" rule, and the "hold the
write guard across an async build, don't double-check-and-lose" pattern from
the jams fix are now consistently applied in `remote_state.rs` and
`friends.rs`'s dealer-subscription retries (`connect_state_task`/
`friends_task`, both stored and aborted on logout/replacement — independently
re-verified this session) — with one inconsistency (the unbounded-retry gap
addressed in §6 below) that has now been closed to match.

**4. The project reversed a prior recommendation for a documented reason, and
said so.** `CLAUDE.md`'s DJ-narration section explicitly quotes its own
now-superseded "defer indefinitely" guidance and explains why the reasoning
behind it stopped applying (a real second `cpal` audio path, sequenced rather
than mixed, because the real client doesn't mix narration either). This is
the correct way to handle a changed decision, and it is used consistently —
Mica→Acrylic, opaque→transparent, and now DJ narration all follow the same
convention.

**5. Zero XSS surface, confirmed fresh.** Independently re-verified this
session, not carried over on faith: no `@html`, `innerHTML`, `outerHTML`,
`eval(`, or `new Function` anywhere in `src/`. Every Spotify-sourced string
renders through Svelte's default escaping.

**6. `store.svelte.ts`'s test suite is real, not decorative.** Independently
verified: the 10 Vitest tests cover `handleError`'s session-clearing logic,
the 1 Hz position ticker's start/stop/clamp via `vi.useFakeTimers`,
out-of-order stale-lyrics-request cancellation (a genuine race), and
optimistic-toggle rollback on backend failure — the exact highest-value
targets the prior review named, not easy getters chosen to inflate a count.

---

## 5. Critical Risks and Issues

| ID | Priority | Area | Issue | Difficulty | Impact | Recommended Action |
|---|---|---|---|---|---|---|
| P0-1 | P0 | Security / Legal | **Code fixed this session; live verification still outstanding.** `pathfinder.rs` no longer spoofs Spotify's desktop client — `queryArtistOverview` sends the same honest identity as every other Pathfinder operation now | Medium (done) / — (verification needs a live account) | Critical until verified | Log in with a real account, open an artist page, confirm what happens; wire/accept the fallback per the updated finding below |
| P0-2 | P0 | Legal | GPLv3-derived code now in the tree, no LICENSE, no written distribution-safety record anywhere in the repo, and the prior review's own "add MIT/Apache-2.0" fix is now unsafe to follow | Low (writing it down) / High (the underlying decision) | Critical | Write the GPL/undistributed constraint into `CLAUDE.md` and `README.md` before anything else touches licensing; do **not** add a permissive `LICENSE` file. **Explicitly deferred at the maintainer's request during this session — not applied** |
| P2-1 | P2 | Code quality | `NowPlaying.svelte` still 839 lines, 1.6x the next-largest component, after a partial split | Medium | Medium | A second extraction pass if it grows further; not urgent now |
| P2-2 | P2 | Testing | No true integration/E2E coverage exists; all automated tests are pure-function/unit-level | High | Medium | Accept the manual checklist as the intentional E2E layer (per `CLAUDE.md`), as this project's own docs already argue — don't chase automated E2E for its own sake |
| P2-4 | P2 | Dependencies | `npm audit` (full) reports 2 moderate advisories in `@vitest/mocker` (dev-only; not in the shipped binary) | Low | Low | Defer the Vitest 5 upgrade until it's not a breaking change, or accept the dev-only risk explicitly |
| P3-2 | P3 | Dependencies | `cargo deny check` warns on 3 versions of `winnow` via Tauri's own build-time `toml` chain | Extra small | Low | No action available from this repo; re-check after a Tauri upgrade |
| P4-1 | P4 | Performance | WebView2 memory floor | Very High | Low | Do not pursue (unchanged from last review) |
| P4-2 | P4 | Portability | Cross-platform support | Very High | Low | Do not pursue (unchanged from last review) |
| P4-3 | P4 | Frontend architecture | Client-side router at 24 views | Medium | Low | Revisit only if navigation becomes a bug source (unchanged from last review) |

**Fixed during this review session** (applied directly, gate suite re-run
green after every change — see §6 for the exact diffs and updated finding
text): the `pathfinder.rs` impersonation headers are removed (P0-1's code
half — see the updated finding below for what's still outstanding); no
GitHub Actions CI existed (the prior review's P1-1, and the single item
Phase 1–9 skipped) → `.github/workflows/ci.yml` now runs all seven gates on
every push/PR; `remote_state.rs` and `friends.rs` each retried a
non-transient dealer-subscription error at a fixed 1-second interval forever
with no escalation, inconsistent with this project's own documented watchdog
backoff pattern → both now back off exponentially, capped at 30s, never
giving up (these two are optional Tier 2 features and must degrade, not
die); DJ narration's playback thread (former P2-3) wasn't tied to session
lifetime → it now polls a `should_stop` closure every 100ms and silences the
device within one poll interval of a logout/session replacement, with three
new fast unit tests (`narration::tests::wait_or_stop_*`) proving the timing
logic; five icon-only transport buttons in `PlayerBar.svelte` (Shuffle,
Previous, Play/Pause, Next, Repeat) had `title` but no `aria-label`, unlike
their siblings → all five now have both; `is_builder_not_available` (former
P3-1) was duplicated in `remote_state.rs`/`friends.rs` and re-inlined a third
time in `jams_bridge.rs` → extracted to a new `dealer_util.rs`, used by all
three; `CLAUDE.md` stated 134 Rust tests and "there is no CI to run
[`cargo deny check`] automatically" → corrected, now 146 tests after this
session's additions, and describes the new workflow. **Not applied, on
request:** writing the GPL/distribution
constraint into `CLAUDE.md`/`README.md` (P0-2) — the maintainer asked to
skip this specifically; the finding and its exact recommended text remain
below for whenever it's wanted.

---

# 6. Detailed Findings by Priority

---

# 7. P0 — Critical / Hardest Issues

---

## [P0-1] The app impersonated Spotify's first-party desktop client — fixed in code this session, live verification still outstanding

**Priority:** P0 (until the live-verification step below is done; the code-level defect itself is closed)
**Area:** Security / Legal / Product integrity
**Difficulty:** Medium (the edit was small; the reason this stayed open for two review cycles was the live-testing dependency, not the code)
**Impact:** Critical while open; residual impact is now "the artist-overview feature may degrade" rather than "the user's account is at risk"
**Confidence:** Confirmed (the fix), Unverified (its live behavior)
**Evidence:** `src-tauri/src/spotify/pathfinder.rs` — the `desktop_artist` conditional branch is deleted; `query_hash` now sends `app-platform: WebPlayer`, `Origin: https://open.spotify.com`, `Referer: https://open.spotify.com/` unconditionally for every operation, with no per-operation `User-Agent`/`spotify-app-version` override. `cargo check`/`clippy -D warnings`/`fmt --check`/`test --lib` all re-run green after the change.

### Problem (as found, now fixed)

`query_hash` branched on `operation == "queryArtistOverview"` and, when true,
sent `app-platform: Win32_x86_64`, `Origin: https://xpui.app.spotify.com`,
`Referer: https://xpui.app.spotify.com/`, `spotify-app-version: 896000000`,
and a `User-Agent` claiming to be `Spotify/1.2.88.483` running inside a real
Chrome build — unchanged for two full review cycles (2026-09-02 and this
session's first pass) despite nine remediation commits fixing everything else
the prior review flagged. It's now removed.

I also independently found a second, lower-severity instance while re-reading
`jams/spclient.rs:254-257`: every Jam-protocol request sends `app-platform:
Win32_x86_64` too. I want to be precise about why I did **not** touch this
one: `spclient.rs`'s own comment at lines 250-253 states this pairs with a
*genuine* Login5 bearer/client-token the app already legitimately holds from
librespot's own desktop session — a protocol discriminator on real
credentials, not an invented identity riding on top of a spoofed browser
fingerprint. `pathfinder.rs`'s deleted branch was the one making the
stronger, false claim (a fabricated `User-Agent` claiming to be a specific
browser/Spotify build the process is not). This is a judgment call the
project has already made and explained for `spclient.rs`; flagged here only
so it isn't rediscovered as "new" without this context.

### Current Behavior (post-fix)

Every Pathfinder operation, including `queryArtistOverview`, now identifies
as the Web Player (`open.spotify.com`) — the same honest identity `home`,
`queryTrackCreditsModal`, `searchUsers`, and `fetchPlaylistContents` already
used before this fix. `telemetry.rs`'s module doc ("Rustify deliberately does
not do that") and `README.md:179-182`'s "hard line" claim are both now
*true* again, rather than contradicted — no doc changes were needed as a
result of this fix, since the fix brought the code back in line with what
the docs already claimed.

### Desired Behavior

Achieved for the header-honesty half. Not yet confirmed: whether Spotify's
edge still serves `queryArtistOverview` to a caller identified as the Web
Player rather than a desktop app.

### Impact

The account-ban-risk exposure this finding described is closed as of this
edit landing in the working tree. What remains is a **product-behavior
question, not a security question**: `commands/library.rs::get_artist_overview`
(read in full while verifying this fix's blast radius) already handles a
hard Pathfinder failure gracefully — a failed call returns empty
stats/top-tracks/`ConcertFeed::unavailable()` rather than crashing or
erroring the command, and deliberately does **not** trigger the REST
top-tracks fallback on a hard failure (only on a *successful* response with
an empty field — schema drift, not an outright rejection) specifically to
avoid amplifying a rate-limit into more requests. That guard's reasoning
doesn't obviously extend to a deterministic 403 from a changed identity, but
I did not touch it: I don't know yet whether the failure mode is even a 403,
and reshaping unrelated fallback logic on a guess is exactly the kind of
speculative change this review is trying not to make.

### Recommended Fix

Done for the code. What's left: log in with a real account, open an artist
page, and observe what happens.

### Implementation Plan

1. ~~Delete the conditional header blocks; use `WebPlayer`/`open.spotify.com`
   uniformly.~~ **Done.**
2. **Live-test against a real account — still the one step nothing in this
   review or its tooling can substitute for.** Open an artist page. Check the
   log for `spotify.artist: artist overview via Pathfinder failed for
   {artist_id}: {error}` (only printed on a hard failure, per
   `commands/library.rs`).
   - If no warning appears and the page looks normal: done, no further
     action, and this can close outright.
   - If it 403s: decide whether `get_artist_overview`'s failure handling
     should also attempt `library::artist_tracks`'s REST fallback on a hard
     Pathfinder failure (not just an empty-field success) — that's a real,
     separate follow-up, not something to guess at now.
3. If the fallback question in (2) comes up, that's a new, scoped finding for
   whoever runs the next pass — not pre-emptively fixed here.

### Acceptance Criteria

- [x] No source file sends a `User-Agent`, `Origin`, `Referer`, or version
      header claiming to be an official Spotify client.
- [ ] Live-measured behavior of `queryArtistOverview` post-change is recorded
      somewhere (`obsidian/`, `CLAUDE.md`, or just reported back).
- [x] Docs and code agree on the impersonation policy (they now do, with no
      doc edit needed).

### Estimated Effort

Small remaining — one login and one page visit.

### Dependencies

Live Spotify Premium credentials, which this environment does not have.

### Risks if Ignored

None from the code as it stands now. The only remaining risk is discovering,
later and by accident, that the artist page silently degraded — which is why
the one-time live check above is still worth doing even though nothing here
is urgent any more.

---

## [P0-2] Undisclosed GPL liability, no LICENSE, and the previous review's own fix is now unsafe

**Priority:** P0
**Area:** Legal
**Difficulty:** Low to write the safeguard down; the underlying decision itself is already made and, as explained to the user in a prior session, legally coherent as long as it holds
**Impact:** Critical
**Confidence:** Confirmed (repo state, code references, remote visibility) / [ASSUMPTION] for the *content* of the 2026-09-05 decision, which I know only from this assistant's own persisted memory of that session, not from re-deriving it independently — I verified its externally-checkable claims (code references, remote visibility) but did not re-litigate the legal reasoning itself
**Evidence:** No `LICENSE`/`COPYING` at repo root (verified, `ls` returns nothing). `tauri.conf.json:5` — `"identifier": "dev.local.rustify"`, still the scaffold placeholder. `player.rs:512-515`, `narration.rs:37-39`, `remote_state.rs:286-288` each explicitly reference "go-librespot's own" behavior. `README.md`/`CLAUDE.md` grepped for `GPL`/`go-librespot`: zero hits in either. `curl https://api.github.com/repos/LAMAgalletta0IQ/Rustify` → `404` unauthenticated, checked live during this review.

### Problem

This finding did not exist, in this form, in the last review — and that's the
point. On 2026-09-02, "no LICENSE" was a garden-variety P0 with an easy
answer: pick MIT or Apache-2.0, done in under two hours. Three days later, on
2026-09-05, a decision was made (recorded in this assistant's memory, from a
session I was not part of but can verify the traces of) to port two features
from `go-librespot` — a GPL-3.0-licensed project — into this codebase: the
crossfade scheduler and DJ narration audio injection. Both now exist in the
tree (`player.rs`'s crossfade/`spawn_dj_advance`, `narration.rs`). The
reasoning given at the time was legally real, not hand-waved: GPLv3's
copyleft conditions are conditions on *conveyance*, not on private
modification, so porting the code without adopting GPL-3.0 for Rustify is
coherent **exactly as long as the project is never distributed to anyone** —
and this project's single-binary architecture (no sidecar separation) means
that the instant it is, GPL-3.0-or-later attaches to the whole compiled
binary, not just the ported portions.

That determination still holds today — I checked, not assumed: the GitHub
remote returns 404 unauthenticated right now. But the determination's entire
safety margin is "as long as this stays private," and **nothing enforces or
even documents that constraint inside the project itself.** `README.md` says
"Personal use" once, in passing, with no mention of why that framing is now
load-bearing in a way it wasn't three weeks ago. `CLAUDE.md` — a document
whose entire stated purpose is capturing exactly this kind of non-obvious,
easy-to-lose context — says nothing about it. The prior review's own P0-2
recommendation, sitting unresolved in this same file's git history, explicitly
tells a future reader to "Add `LICENSE` — MIT or Apache-2.0 (permissive is
simplest)." Following that advice today, from this file, without knowing what
I know from memory, produces a real GPL compliance violation the moment
anyone acts on it and then the repo is ever made public. That is a trap this
review's own predecessor set, in good faith, that has since become live.

### Current Behavior

The project is legally safe right now, by a determination that lives in one
place: an AI assistant's private memory of a conversation. Anyone reading only
the repository — a future contributor, a future Claude session without that
memory, the maintainer's own future self after forgetting — has zero warning.

### Desired Behavior

The constraint is written down in the project itself, prominently, in both
places someone would look (`README.md`'s first screen and `CLAUDE.md`), so it
survives independently of any one memory system or any one person's recall.

### Impact

If the repo is ever made public, or the built installer is ever shared, or
the source is ever handed to anyone, without this being caught first: the
whole compiled binary needs to be GPL-3.0-or-later, with source availability
to recipients and attribution for the ported portions — none of which is in
place, and none of which can be bolted on retroactively for anyone who already
received a copy under a false (or absent) license. This is not a hypothetical
enforcement risk in the way client impersonation is; it is a mechanical,
automatic legal fact that either is or isn't true the moment distribution
happens, with no gray area.

### Recommended Fix

Do not add a permissive `LICENSE`. Instead, write the actual constraint down,
in the project, now — independent of whether or when distribution is ever
revisited.

### Implementation Plan

1. Add a short, explicit section to `CLAUDE.md` (a natural home given its
   existing "Repository gotchas" section) stating: this project ports
   GPL-3.0 logic from `go-librespot` (name the specific features); it remains
   deliberately unlicensed and undistributed; GPL-3.0-or-later copyleft
   attaches to the entire binary the moment it is distributed in any form
   (public repo, shared installer, given source); before any of that happens,
   revisit licensing and attribution first.
2. Add one sentence to `README.md`'s opening (next to the existing "Personal
   use" line) stating plainly that the project is not licensed for
   redistribution and explaining, in one clause, why (GPL-derived code,
   undistributed-only).
3. Do **not** change `deny.toml`'s license allow-list (it correctly excludes
   GPL-3.0 today, which is correct for as long as the project stays
   undistributed) — but add a comment there cross-referencing the new
   `CLAUDE.md` section, so the allow-list and the reason it looks the way it
   does are findable from the same place.
4. Leave `tauri.conf.json`'s placeholder `identifier` exactly as it is for
   now — fixing it is legitimate future work, but it is not urgent while
   nothing is distributed, and touching it forces every existing install to
   re-login (per `CLAUDE.md`'s own documented warning). Don't do it as a
   drive-by alongside this.
5. If and when distribution is ever seriously considered, the actual
   decision — GPL-3.0-or-later for the whole project, or removing/reimplementing
   the two ported features from a clean-room read of the public protocol
   instead of the GPL source — has to be made *before* a `LICENSE` file of
   any kind is added, not after.

### Acceptance Criteria

- [ ] `CLAUDE.md` states the GPL provenance, which features are affected, and
      the undistributed-only constraint, in writing.
- [ ] `README.md`'s first screen reflects the same constraint in one sentence.
- [ ] No permissive `LICENSE` file is added without first resolving (4).
- [ ] A future session or contributor reading only the repository — not this
      assistant's memory — would learn this constraint before touching
      licensing or distribution.

### Estimated Effort

Extra small (under 30 minutes) to write it down. The underlying decision is
already made; this finding is about making it discoverable, not about
re-deciding it.

### Dependencies

None. This should happen before anything else in this review, including
before revisiting the placeholder `identifier` or any packaging/signing work,
because both of those are downstream of a licensing decision that this
finding shows is not yet safely recorded.

### Risks if Ignored

A future well-intentioned action — making the repo public to share the code,
handing someone the installer, or a future session literally following the
prior review's own recommendation — creates a real, mechanical GPL violation
with no one aware it happened until someone downstream notices.

---

# 8. P1 — High Difficulty / High Impact Issues

No findings identified for this level based on the provided information. The
single item that carried genuine P1 severity from the prior review — the
complete absence of CI — was fixed during this review session (see §6's
"Fixed during this review session" note and the workflow at
`.github/workflows/ci.yml`); it is not restated here as an open finding. Every
other former P1 (jams concurrency, detached tasks, plaintext tokens,
`commands.rs` bloat, feature-tier documentation) was independently
re-verified as fixed. Four dedicated subagent passes across the entire
backend and the entire frontend surfaced nothing that rises to this severity
that wasn't already captured at P0 above.

---

# 9. P2 — Medium Difficulty / Important Issues

---

## [P2-1] `NowPlaying.svelte` is still the largest component after a partial split

**Priority:** P2 | **Area:** Code quality | **Difficulty:** Medium | **Impact:** Medium | **Confidence:** Confirmed
**Evidence:** 839 lines today (independently re-counted this session), down from 1,166 at the last review — a real reduction, via the documented extraction of `LyricsPane.svelte`, `SleepTimerBadge.svelte`, and `FullscreenChrome.svelte`. Still 1.6x the next-largest frontend file (`Settings.svelte`, 514 lines).

### Problem

The three-way extraction the last review recommended happened and reduced the
file by 28% — real progress, not cosmetic. But the parent retained the
fullscreen host, the drag-region handling, and the orchestration wiring for
all three extracted children, which is still four responsibilities living in
one file.

### Current / Desired Behavior

Currently one large orchestrating component. Desired: the orchestration layer
itself thin enough that a Svelte 5 reactivity bug in it is easy to localize,
which it currently is not quite.

### Impact

Medium, same reasoning as before, at reduced severity given the file is a
third smaller than when this was first flagged.

### Recommended Fix

Not urgent. If the file grows again — the natural direction for a "now
playing" surface that keeps gaining features — do a second extraction pass
then, rather than pre-emptively right now.

### Implementation Plan

1. Watch the file's size at the next feature addition that touches it.
2. If it crosses ~1,000 lines again, extract the orchestration/drag-region
   logic into its own thin wrapper, keeping the three already-extracted
   children as-is.
3. Re-run `npm run check` and manually verify fullscreen enter/exit and
   window-drag behavior in both states, per this project's own documented
   drag-region subtlety (`undefined` vs `false`).

### Acceptance Criteria

- [ ] No regression in fullscreen/drag behavior if and when this is revisited.
- [ ] `npm run check` stays at 0 errors.

### Estimated Effort

Medium, if and when undertaken.

### Dependencies

None urgent.

### Risks if Ignored

Slow drift back toward the file's old size, at which point extraction
becomes a rewrite again — same risk the last review named, just further off.

---

## [P2-2] No true integration or end-to-end coverage exists

**Priority:** P2 (downgraded from the prior review's P1) | **Area:** Testing | **Difficulty:** High | **Impact:** Medium | **Confidence:** Confirmed
**Evidence:** 142 Rust tests (up from 106) + 10 Vitest tests, all confirmed by direct execution this session. Every one is a unit/pure-function test; none launches Tauri, librespot, or a real webview. `CLAUDE.md` states this outright and frames the manual 15-step checklist in `README.md` as the deliberate E2E layer, not a stopgap.

### Problem

This is the one item from the prior review's P1-2 that wasn't fully "fixed" —
because full integration/E2E coverage for a Tauri+librespot+real-Spotify-account
app is a different, much larger kind of project than what got built here, and
I'm not convinced it should be. What *did* happen is exactly the highest-value
subset the prior review recommended: the previously-untested dangerous logic
(token merge, `is_grant_rejected`, position anchoring, session-generation
monotonicity, jams concurrency) now has real regression tests naming the bug
each one prevents, and the frontend went from zero test infrastructure to a
genuine, non-superficial Vitest suite. That's real progress, which is why
this is P2 now rather than restating the old P1 verdict unchanged.

### Current / Desired Behavior

Currently: the pure logic is well-tested; nothing exercises a live Tauri
process, librespot, or a rendered webview. Desired, realistically: this stays
true, and the manual checklist stays the accepted, explicitly-labeled
substitute — chasing automated E2E for a personal, one-maintainer,
private-API-dependent desktop app is likely not worth its cost relative to
what it would catch beyond what a careful manual pass already does.

### Impact

Medium, not High: the specific failure mode the prior review worried about
most (a future auth/session refactor silently reintroducing the token-erasure
loop) is now guarded by a named regression test, which was the actual point.
What remains untested is "does the whole system work end to end," which the
manual checklist already covers by design.

### Recommended Fix

Don't build automated E2E infrastructure for its own sake. Keep the manual
checklist current (it already is) and keep adding targeted regression tests
the way Phase 3 did, at the seam of any future dangerous-logic change.

### Implementation Plan

N/A as an active work item — this is a "keep doing what's already working"
finding, not a backlog item.

### Acceptance Criteria

- [ ] Any future change to auth/session/watchdog logic ships with a named
      regression test the same way the existing ones do.
- [ ] `README.md`'s manual checklist stays current as features change.

### Estimated Effort

N/A — ongoing practice, not a discrete task.

### Dependencies

None.

### Risks if Ignored

Low, given the specific highest-risk logic is now covered. The residual risk
is the ordinary one of any manually-tested software: a regression the
checklist doesn't happen to exercise that day.

---

## [P2-3, RESOLVED] DJ narration's playback thread outlived an aborted DJ session by a few seconds

**Priority:** was P2 | **Area:** Reliability | **Status:** Fixed and verified this session

**What it was:** if logout (or a session replacement) happened while a DJ
narration clip was mid-playback, the OS thread backing `narration::play_clip`
kept holding the audio device and playing for up to the clip's remaining
duration (a few seconds) after the session it belonged to was gone —
self-terminating, not an open-ended leak, but a stray audio blip past logout.

**What changed:** `play_clip` now takes a `should_stop: impl Fn() -> bool +
Send + 'static` closure. The single long `std::thread::sleep(clip_duration)`
is replaced by `wait_or_stop`, a small extracted function that sleeps in
100ms (`STOP_POLL_INTERVAL`) chunks and returns as soon as `should_stop()`
reports `true`, dropping the `cpal` stream (silencing the device)
immediately rather than waiting out the clip. `player.rs`'s
`play_narration_if_present` captures `state.session_generation()` before
calling `play_clip` and passes `move || app_handle.state::<AppState>()
.session_generation() != generation` — the same generation-tagging pattern
this codebase already uses for the playback watchdog, applied here for the
first time.

**Why this is a real fix, not just a change:** the polling loop was
extracted into its own function specifically so it could be tested without
opening a real `cpal` device. Three new fast unit tests
(`narration::tests::wait_or_stop_returns_immediately_when_already_stopped`,
`..._waits_the_full_duration_when_never_stopped`,
`..._stops_partway_through_once_the_flag_flips`) assert the timing behavior
directly — a `should_stop` that flips after the second poll must exit in
under a second even against a 5-second total, and one that never flips must
wait out the full duration. All three pass; the existing ignored
hardware-only smoke test (`plays_an_audible_tone`) was updated to the new
signature (`|| false`) and still compiles.

**Re-verified:** `cargo check`/`clippy -D warnings`/`fmt --check` clean;
`cargo test --lib` now 145 passed (142 → 145, the three new tests), 0
failed, 4 ignored (unchanged — those are the pre-existing live-credential
tests, not this fix).

**What's still unverified:** the fix's *live* audio behavior (does the
device actually go silent within ~100ms during a real DJ session) — the
timing logic is proven, but nobody has run this against real hardware and a
real DJ session yet. Worth a quick manual check per §15's testing plan
whenever DJ narration is next used.

---

## [P2-4] Dev-only `@vitest/mocker` advisory

**Priority:** P2 | **Area:** Dependencies | **Difficulty:** Low | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** `npm audit` (full, including devDependencies) run this session: 2 moderate advisories, `@vitest/mocker` 2.1.0–4.1.10, GHSA-82fw-gwwq-j7x9 (path traversal / arbitrary file read via redirect mock). `npm audit --omit=dev` (production-only, matching what actually ships): 0 vulnerabilities, matching the prior review's finding.

### Problem

This is a new finding only in the sense that Vitest didn't exist at the last
review — it was added as part of fixing the old P1-2. The advisory is in a
transitive dev-dependency of the test runner itself, never bundled into the
shipped app.

### Current / Desired Behavior

Currently: present in `node_modules` for local dev/CI only. Desired: clean,
eventually, without forcing a disruptive upgrade right now.

### Impact

Low. Zero exposure to end users of the built app; the worst case is a
compromised local dev/CI environment via a malicious redirect during test
runs, which is not this project's threat model today.

### Recommended Fix

Defer. `npm audit fix --force` would install Vitest 5, a breaking change to a
test suite that was just stabilized this cycle — not worth the churn for a
dev-only, non-shipped advisory.

### Implementation Plan

1. Revisit when Vitest 5 adoption is otherwise motivated (a feature need, not
   this advisory alone).
2. Until then, no action.

### Acceptance Criteria

- [ ] `npm audit --omit=dev` stays at 0 (the metric that actually matters for
      the shipped binary).

### Estimated Effort

Extra small if ever revisited; none required now.

### Dependencies

None.

### Risks if Ignored

Minimal — confined to local/CI dev environments, not the shipped product.

---

# 10. P3 — Low Difficulty / Quick Wins

---

## [P3-1, RESOLVED] `is_builder_not_available` was duplicated verbatim

**Priority:** was P3 | **Area:** Code quality | **Status:** Fixed and verified in a later pass this session

**What it was:** `remote_state.rs` and `friends.rs` each defined a
byte-for-byte identical `is_builder_not_available` function, and
`jams_bridge.rs`'s `subscribe_with_retry` inlined the same
`error.to_string().contains("Builder wasn't available")` check a third time
independently.

**What changed:** extracted to a new single-purpose module,
`src-tauri/src/dealer_util.rs`, with the classifier and a module doc
explaining *why* it's shared rather than owned by any one of the three
callers (the race belongs to librespot's dealer bootstrap, not to Connect
state, friend presence, or jams specifically — none of the three is the
"real" owner). `remote_state.rs` and `friends.rs` now `use
crate::dealer_util::is_builder_not_available` instead of defining it
locally; `jams_bridge.rs`'s `subscribe_with_retry` now calls the same shared
function instead of inlining the string check.

**Re-verified:** `cargo check`/`clippy -D warnings`/`fmt --check` clean;
`cargo test --lib` now 146 passed (up from 145), 0 failed, 4 ignored — the
new count is `dealer_util::tests::matches_only_the_transient_builder_race`,
which asserts the classifier matches a `librespot::core::Error::
failed_precondition("Builder wasn't available")`-shaped error and does not
match an unrelated `Error::unavailable(...)`. `npm run check` unaffected (0
errors, 213 files) — this was a Rust-only change.

**Note on why this one, and not the others:** unlike P2-1/P2-2/P2-4/P4-1–3
(all left alone — see §14), this was genuinely uncontested: a pure
same-behavior refactor with no product, testing-philosophy, or scope
tradeoff attached to it, so there was nothing to weigh before doing it.

---

## [P3-2] `cargo deny check` warns on triplicated `winnow`

**Priority:** P3 | **Area:** Dependencies | **Difficulty:** Extra small | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** `cargo deny check` output this session: `winnow` 0.5.40 (via `rustify`'s own `toml` 0.8 dependency), 0.7.15 and 1.0.4 (both via `tauri-build`'s transitive `toml`/`cargo_toml` chain).

### Problem / Recommended Fix

Not actionable from this repository — the duplication is entirely inside
Tauri's own build-tooling dependency chain, not something `rustify`'s
`Cargo.toml` controls. `deny.toml`'s `bans.multiple-versions = "warn"`
already correctly treats this as informational, not a failure. Re-check after
the next Tauri version bump; it may resolve on its own.

### Estimated Effort

None available now.

### Risks if Ignored

None; this is `deny.toml` working as configured, not a defect.

---

# 11. P4 — Optional / Future Improvements

## [P4-1] WebView2 memory floor

Unchanged from the prior review. **Recommendation: do not pursue.** Replacing
the webview to reclaim ~150MB the OS already shares across WebView2 processes
is months of native-UI work for a marginal gain. Re-confirmed nothing in this
pass changes that math.

## [P4-2] Cross-platform support

Unchanged from the prior review. **Recommendation: no.** Windows-only remains
a coherent, deliberate choice, and the DPAPI work done since the last review
(closing P1-5) deepens that commitment correctly, not accidentally.

## [P4-3] Client-side router

Unchanged from the prior review at 24 views now (was 13). **Recommendation:
revisit only if navigation logic becomes a source of bugs**, which nothing in
this pass found evidence of.

**Note, not a recommendation:** the prior review's P4-2 ("DJ narration audio
pipeline — defer indefinitely") was overridden by the maintainer for stated,
documented reasons (see §4, strength 4) and the feature now exists and works.
This is not flagged as ignoring the prior review — it's flagged as the
correct way to override a review's recommendation: explicitly, with reasoning
written down where a future reader will find it.

---

# 12. Recommended Execution Plan

## Immediate Actions

**Goal:** Close what's left of the two decisions that were the entire
remaining risk surface. One is now a code fix with a verification step left;
the other is still a decision, deliberately untouched this session.

1. **P0-1: live-test the header fix.** The code is done — log in, open an
   artist page, check the log for a Pathfinder failure warning. Five minutes,
   whenever there's a live session handy. Not urgent in the way it was;
   nothing ships broken while this is pending, it's just unverified.
2. **P0-2: write down the GPL/undistributed constraint in `CLAUDE.md` and
   `README.md`, when ready.** Twenty minutes, zero dependencies, and the
   cheapest possible permanent risk reduction available in this review — but
   deliberately **not done** this session at the maintainer's request. The
   exact text to write is in the P0-2 finding above whenever it's wanted.

**Definition of done.** `queryArtistOverview`'s live behavior post-fix is
known and, if it degrades, a decision is made about the fallback; whenever
P0-2 is taken up, `CLAUDE.md`/`README.md` state the GPL constraint in
writing.

## Short-Term Plan (1–2 weeks)

Nothing urgent remains at this horizon. If time is available: the remaining
P2 items above (§9 — P2-1, P2-2, P2-4) in whatever order is convenient — none
of them block anything else and none of them is time-sensitive.

## Medium-Term Plan (2–6 weeks)

1. Revisit `NowPlaying.svelte`'s size only if it grows again (P2-1).
2. Consider the Vitest 5 upgrade if/when otherwise motivated (P2-4).
3. Apply the quarterly Tier-3 feature review `CLAUDE.md` already commits to
   (Jams, music video/lossless probing, DJ narration) against actual usage —
   this is the project's own stated process, not a new recommendation from
   this review.

## Long-Term Plan

Nothing in this codebase currently warrants long-term planning beyond keeping
the two P0 decisions resolved and continuing the Tier-3 quarterly review
`CLAUDE.md` already commits to. That is itself a sign of health, not a gap in
this review — a mature, personal-scope project doesn't need a long-term
roadmap section manufactured for the sake of having one.

---

# 13. Quick Wins

- **Write the GPL/distribution constraint into `CLAUDE.md`/`README.md`** —
  Extra small (20 min); prevents a real future legal mistake for almost no
  cost. Apply in `CLAUDE.md`'s "Repository gotchas" and `README.md`'s
  opening. **Deliberately not applied this session** — see P0-2.
- **CI workflow** — already applied this session (`.github/workflows/ci.yml`).
- **`pathfinder.rs` impersonation headers removed** — already applied this
  session; live verification still pending (see §15).
- **DJ narration thread now stops on logout/session replacement** — already
  applied this session, with new unit tests proving the timing.
- **`remote_state.rs`/`friends.rs` retry backoff** — already applied this
  session.
- **`is_builder_not_available` deduplicated into `dealer_util.rs`** — already
  applied this session, used by all three former call sites.
- **`PlayerBar.svelte` `aria-label`s** — already applied this session.
- **`CLAUDE.md` test-count/CI-claim correction** — already applied this
  session.

---

# 14. What Not to Change Yet

- **Do not add a permissive `LICENSE` file.** This is the single most
  important "don't" in this entire review. It is exactly what the prior
  review recommended and it is now wrong, for reasons that had nothing to do
  with that review being careless — the facts changed three days after it
  shipped. Resolve P0-2's underlying decision first.
- **Do not touch `tauri.conf.json`'s `identifier`.** Still a placeholder,
  still correctly left alone: changing it orphans every existing
  `tokens.json` and forces a re-login, per `CLAUDE.md`'s own documented
  warning, and it is not urgent while nothing is distributed.
- **Do not pursue automated E2E/integration testing as a project.** Per
  P2-2's reasoning — the manual checklist is the right-sized answer for this
  project's actual shape, not a stopgap waiting to be replaced.
- **Do not force the Vitest 5 upgrade to clear the dev-only audit finding
  (P2-4).** The advisory doesn't reach the shipped binary; the upgrade is
  breaking; there's no urgency trade worth making here.
- **Do not split `NowPlaying.svelte` further right now (P2-1), replace the
  webview (P4-1), chase cross-platform support (P4-2), or add a router
  (P4-3).** Asked directly whether to override these four "don't fix"
  recommendations; the answer was to use my own judgment, and my judgment —
  unchanged from when I wrote the findings — is that none of the underlying
  conditions (the file growing again, an actual memory problem, a real
  portability need, navigation bugs) have occurred. Revisit each on its own
  stated trigger, not on a schedule.

---

# 15. Testing Plan

The automated suites already cover what they should. What's left is manual,
by design (per `CLAUDE.md`), and specifically weighted toward what this
session's code changes still need a live account to confirm:

- [ ] **P0-1 live verification (manual, requires live account):** log in,
      open an artist page, check whether it looks normal and whether the log
      shows `spotify.artist: artist overview via Pathfinder failed for
      {artist_id}: {error}`. Nothing to fix ahead of time — just observe and
      report back.
- [ ] **Existing 15-step checklist in `README.md`** — unchanged, still the
      right set of manual cases (login, Setup-screen gating, playback,
      context continuation, transport, Connect inbound/outbound, token
      expiry). Re-run it once, since this session touched `pathfinder.rs`,
      `remote_state.rs`, `friends.rs`, and `narration.rs` — consider adding an
      artist-overview step to it while there.
- [ ] **DJ narration cancellation (optional, low priority):** log in, start
      DJ, trigger narration, log out mid-clip, confirm the device goes quiet
      promptly rather than trailing off. The timing logic has fast unit tests
      proving it; this would be the first live confirmation against real
      hardware and a real session.

No new automated test infrastructure is recommended at this time (see P2-2).

---

# 16. Security Checklist

- [x] **Secrets in logs** — confirmed clean across every file re-read this
      session; `dj.rs` explicitly redacts signed URLs, matching the existing
      standard.
- [x] **Credential storage at rest** — DPAPI-encrypted, user-bound, migrates
      existing plaintext installs silently. Closed since the last review.
- [x] **Dependency vulnerabilities (production)** — `cargo deny check` clean;
      `npm audit --omit=dev` clean.
- [ ] **Dependency vulnerabilities (dev-only)** — 2 moderate, deferred (P2-4).
- [x] **CSP / webview isolation** — `default-src 'self'`, no `@html`/
      `innerHTML`/`eval` anywhere, `withGlobalTauri: false`. Unchanged, still
      correct.
- [x] **Automation/debug backdoors** — `tauri-plugin-mcp-bridge` confirmed
      still gated behind a Cargo feature, absent from the default dependency
      graph, bound to loopback only.
- [x] **Client identity honesty** — **fixed this session** (P0-1). No source
      file sends a header claiming to be an official Spotify client. Live
      behavior of `queryArtistOverview` under the new identity is unverified —
      see §15 — but the security property itself (no fabricated identity) is
      satisfied regardless of what Spotify's edge decides to do with it.
- [ ] **Licensing/distribution safety** — **at risk, not yet failing.** P0-2:
      currently safe by an undocumented, unenforced constraint. Deliberately
      left unaddressed this session at the maintainer's request.

---

# 17. Performance Checklist

No changes found since the last review; re-confirmed structurally, not
re-benchmarked:

- [x] Pooled `reqwest::Client` reused across commands via `AppState`.
- [x] Bounded caches with TTLs (`artist_overview_cache`, `saved_tracks_cache`,
      `lyrics_cache`, `audio_capability_cache`).
- [x] The one recurring network timer (`spawn_remote_poller`) is gated behind
      `!is_active_device`, as documented.
- [x] `/search` correctly capped at Spotify's undocumented `limit=10` ceiling;
      `/me/following` correctly cursor-paged rather than offset-paged.
- [x] `webapi.rs`'s retry loop is capped (`MAX_RETRIES`, `MAX_AUTO_RETRY_SECS`)
      and GET-only, per the independent subagent re-verification this session.

---

# 18. Documentation Recommendations

- **Add the GPL/distribution constraint** (P0-2) — the one genuinely missing
  piece of documentation in an otherwise exceptional set of docs. **Not
  applied this session at the maintainer's explicit request** — the exact
  text to add is in the P0-2 finding above, ready whenever it's wanted.
- Everything else the prior review flagged (prerequisites table, footprint
  table, feature tiering) is already fixed and re-verified — no further
  documentation debt found.

---

# 19. Open Questions

1. **Is the GPL/undistributed determination from 2026-09-05 still the
   maintainer's intended position?** This review verified its external,
   checkable facts (repo still private, ported code still present) but the
   underlying legal reasoning was explained to the user in a prior session I
   was not part of. The maintainer has now explicitly declined to have this
   written into the repo during this session — which may mean "not yet," "I
   want to review the exact wording first," or something else entirely; I
   don't know which. If the intent has shifted toward eventually
   distributing, or toward removing the ported features instead, that
   changes P0-2's recommended fix entirely. **Why it matters:** every other
   distribution-readiness question (identifier, signing, updater) is
   downstream of this one and shouldn't be worked on independently of it.
2. **Did `queryArtistOverview` keep working after the header fix?** Only
   answerable with a live account — see §15. **Why it matters:** if it 403s,
   there's a follow-up decision about whether `get_artist_overview`'s
   failure handling should also attempt the REST top-tracks fallback on a
   hard Pathfinder failure, not just an empty-field success.

---

# 20. Final Verdict

1. **Is this project safe to continue in its current state?** Yes, for
   personal use. The account-risk item (P0-1) is now closed in code; the
   legal-exposure item (P0-2) remains a decision sitting unmade, by choice,
   this session.
2. **Is it safe to deploy to production, or to anyone else?** No — doing so
   before resolving P0-2 creates a real, mechanical GPL violation, not just a
   generic "no license" gap.
3. **What is the biggest risk?** Unchanged: that P0-2 gets discovered by
   consequence instead of by decision — someone shares the app, or a future
   session adds the "obvious" MIT license the *original* prior review
   recommended, before anyone reads this correction.
4. **What is the highest-value improvement?** Still writing the GPL
   constraint into `CLAUDE.md`/`README.md` — twenty minutes, and the only
   remaining fix in this review that closes a risk *permanently* rather than
   mitigating it. It just wasn't applied this session.
5. **What should be done first?** Whenever it's wanted: P0-2's documentation
   fix. It has zero dependencies and protects every subsequent distribution
   decision from being made without that context. Ahead of it, and not
   dependent on it: the five-minute P0-1 live check.
6. **What should be done second?** Nothing else is time-sensitive. The
   remaining P2/P3 items are real but genuinely optional on any timeline.
7. **What should be avoided for now?** Adding any `LICENSE` file before P0-2
   is resolved; touching the bundle `identifier`; building automated E2E
   infrastructure; any further feature work on the private-API surface
   without running it through the Tier-3 quarterly review `CLAUDE.md` already
   commits to.

**Recommended next step:** do the five-minute P0-1 live check next time
there's a real Spotify session open, just to close the loop. P0-2 stays
exactly where it was left — documented, actionable, and waiting on the
maintainer's call, not on any more code.
