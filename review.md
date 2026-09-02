I'll analyze the actual repo in `D:\projects\Rustify` (the template placeholders are unfilled, but the working directory makes the target unambiguous). Starting with structure and size.

All three verification gates pass (`cargo check --no-default-features` ✅, `cargo test --lib` — 106 passed ✅, `svelte-check` — 0 errors ✅). Full review below.

---

# Project Analysis Review: Rustify

## 1. Review Summary

**Overall health: the code is good. The project is not.**

Those are two different statements and the distinction is the whole review.

The Rust in `src-tauri/src/` is genuinely above professional average. Four non-test `panic!`/`expect` sites in 18,109 lines. Error taxonomy modelled as a 17-variant enum with a stable `kind()` contract to the frontend. Race conditions that most codebases ship with — detached `JoinHandle`s, token-rotation loss on degraded restore, position-anchor staleness — are each identified, fixed, and documented *with the failure mode that motivated the fix*. `svelte-check` reports 0 errors across 153 files under `strict` + `noUnusedLocals` + `verbatimModuleSyntax`. The documentation is better than most funded products'.

The problems are all one level up from the code:

1. **The app impersonates Spotify's first-party desktop client.** `spotify/pathfinder.rs:199` sends `User-Agent: ... Spotify/1.2.88.483`, with spoofed `Origin: https://xpui.app.spotify.com` and `app-platform: Win32_x86_64`. The project's own `telemetry.rs` module doc calls impersonation "a hard line" it will not cross. It already crossed it. This risks the user's Spotify account, not just the app.
2. **It cannot be distributed.** No LICENSE file. `identifier: "dev.local.rustify"` is a placeholder. The NSIS installer is unsigned. There is no updater. If a security fix were needed tomorrow there is no mechanism to ship it.
3. **There is no CI.** Not "weak CI" — `.github/` does not exist. Every gate is a human remembering to run three commands.
4. **The test suite proves almost nothing about the app.** 106 tests, all pure functions over fixed payloads. Zero tests touch Tauri, librespot, or a webview. The frontend has no test framework at all. `CLAUDE.md` states this honestly, which is to its credit, and then does nothing about it.
5. **The scope is unmaintainable by one person.** Jams, DJ, lyrics, credits, concerts, music videos, friend presence, lossless probing, telemetry, Listening DNA, Last.fm enrichment — a large fraction built on private endpoints and scraped persisted-query hashes that Spotify changes without notice and has no obligation to keep working.

**Readiness: internal/personal use only.** It is not beta-ready for other people and it is not production-ready by any definition. That is fine if "personal use" (README, line 8) is the real goal — but then several P1 items are wasted effort, and the honest move is to say so and stop building features.

**Biggest strength:** documentation-of-reasoning discipline. `CLAUDE.md` and the 111-note `obsidian/` vault preserve *why*, including reversed decisions in blockquotes. This is rare and it is the single reason the codebase is still tractable at 27k LOC.

**Biggest risk:** the private-API surface. It is simultaneously the legal risk, the account-ban risk, the maintenance burden, and the reason the project cannot be shared.

---

## 2. Scope and Inputs Reviewed

The prompt's `{{PROJECT_NAME}}`-style placeholders were left unfilled. I reviewed the repository in the working directory, `D:\projects\Rustify` (branch `main`, 81 commits, clean tree).

| Reviewed | Detail |
|---|---|
| Rust backend | All 45 files, 18,109 LOC. Read in full: `lib.rs`, `auth.rs`, `state.rs`, `webapi.rs`, `error.rs`, `jams/config.rs`. Read in part: `commands.rs`, `jams_bridge.rs`, `jams/spclient.rs`, `jams/client_token.rs`, `jams/session.rs`, `spotify/pathfinder.rs`, `telemetry.rs` |
| Frontend | 8,775 LOC. Read in full: `store.svelte.ts`. Scanned: `api.ts`, all views/features for `@html`, `innerHTML`, `eval`, timers, catch patterns |
| Config | `tauri.conf.json`, `capabilities/default.json`, `Cargo.toml`, `package.json`, `vite.config.ts`, `tsconfig.json`, `.gitignore`, `.env.example` |
| Docs | `CLAUDE.md`, `README.md` (all sections), `README_jams.md` (skimmed), `obsidian/` inventory |
| Executed | `cargo check --no-default-features` (exit 0), `cargo test --no-default-features --lib` (106 passed, 3 ignored), `npm run check` (0 errors, 153 files), `npm audit --omit=dev` (0 vulns), `cargo tree` (32 direct deps, 1,157 nodes) |

| Not reviewed / cannot verify | Why it matters |
|---|---|
| **Runtime behaviour** | I did not launch the app. No claim below about live playback, Connect, or recovery is empirically verified — matching `CLAUDE.md`'s own warning that "most real bugs in this codebase have only ever been found by running the app" |
| `audio/mod.rs` DSP correctness | Read structurally, not analysed numerically. The biquad EQ has unit tests; I did not verify filter math |
| `dist/` contents | Build artifact, gitignored |
| `.env` | Gitignored; not read. Assumed to contain a real `RUSTIFY_CLIENT_ID` |
| `cargo audit` | Not installed on this machine — **the Rust dependency tree has not been checked for known CVEs** |
| Whether private endpoints currently work | Requires live credentials. All statements about Spotify's behaviour are taken from the repo's own documented observations |

**Assumptions made:** (a) `README.md`'s "Personal use" framing is the actual intent; (b) the target is Windows 11 only; (c) there is one maintainer.

---

## 3. Overall Assessment

| Area | Score | Summary |
|---|---:|---|
| Product clarity | 7/10 | Purpose and flows are unambiguous; scope has sprawled far past "lightweight client" |
| Architecture | 8/10 | Clean module boundaries, correct concurrency primitives, `commands.rs` is the one bloated seam |
| Code quality | 9/10 | Best-in-class comment discipline, 4 non-test panic sites, zero type errors under strict mode |
| Security | 5/10 | Good credential hygiene and CSP; undermined by client impersonation, plaintext tokens at rest, unsigned installer |
| Performance | 8/10 | Pooled HTTP client, bounded caches, request coalescing, one deliberately gated poller |
| Testing | 4/10 | 106 real tests over pure functions; nothing exercises the runtime; no frontend tests at all |
| Reliability | 6/10 | Excellent recovery *design* (watchdog, generation tagging) with zero automated verification of it |
| Documentation | 9/10 | `CLAUDE.md` + `obsidian/` are exceptional; no LICENSE, no CONTRIBUTING |
| DevOps / Deployment | 2/10 | No CI, no signing, no updater, placeholder identifier, no release process |
| Maintainability | 6/10 | Code is maintainable; the *feature surface* is not, at one maintainer against a hostile moving API |

**Total risk: HIGH — concentrated almost entirely outside the source code.**

Weighted honestly: engineering execution is ~8/10; project management, distribution readiness, and verification are ~3/10. The gap between those two numbers *is* the finding. Someone has spent their effort on the part they enjoy (careful Rust) and skipped the part they don't (CI, licensing, integration testing, saying no to features).

---

## 4. Strengths

These are real, evidenced, and should not be refactored away.

**1. Failure modes are documented at the point of the fix.** `auth.rs:433-449` (`save_stored_tokens`) does not just merge — it explains the self-sustaining 429 loop that merging prevents, in four sentences, with the causal chain. Same at `commands.rs:702-706` (detached `JoinHandle`s → duplicate refreshers → 429). This is the highest-value property in the codebase: it prevents a future maintainer from "simplifying" a fix back into the bug.

**2. The two-credential split is correct and correctly reasoned.** `auth.rs:95-118`. Removing the baked-in fallback client ID was the right call even though it added a Setup screen, and `webapi_client_id`'s three-tier resolution (env → settings.json → error) with `restore_session` interpreting the error as "setup needed" rather than "failure" is the right shape.

**3. Concurrency primitives are chosen deliberately, not by habit.** `AtomicU64` session generation for watchdog disambiguation (`state.rs:262-277`), `watch` channel for the active-device signal, `Mutex<HashMap<String, Arc<Mutex<()>>>>` per-key gates so two navigations to the same artist fire one Pathfinder call. `AudioRuntime` is kept synchronous and off the Tokio state so the sink never blocks on an async lock (`state.rs:243-245`). Each has a stated reason.

**4. Error handling degrades rather than fails.** `is_grant_rejected` limits credential deletion to `invalid_grant`/`invalid_client`. `restore_login` returns `Ok(shared)` on Web API refresh failure specifically because the streaming refresh already rotated. Friend-activity subscription failure downgrades to `FriendFeedStatus::Failed` instead of taking down login (`commands.rs:731-750`). `WebApi::get` absorbs ≤8s rate limits on idempotent verbs only.

**5. Redaction and privacy are actual engineering, not a comment.** `jams/spclient.rs` has a `safe_url()` that redacts join secrets, with a test asserting it (`spclient.rs:679`). `auth.rs` has a test asserting the serialised `DeviceAuthorization` contains no `device_code`/access/refresh substring. `telemetry.rs` is a *bounded, never-transmitted* ledger with `delivery_blocker` surfaced to the UI.

**6. Input validation on responses from Spotify.** `validate_pairing_url` rejects non-HTTPS, non-`*.spotify.com`, and userinfo-embedded hosts — with tests for `https://spotify.com.evil.test` and `https://spotify.com@evil.test`. `valid_user_code` rejects `<script>`. `valid_hash` requires exactly 64 hex chars. This is the correct paranoia level for parsing a remote service's output.

**7. Zero XSS surface in the webview.** No `@html`, no `innerHTML`, no `eval`, no `new Function` anywhere in `src/`. CSP is restrictive and image sources are pinned to Spotify CDNs.

**8. Documentation preserves reversed decisions.** `CLAUDE.md`'s blockquote convention ("Until 2026-08 this file said X. That was wrong...") is better practice than most engineering orgs manage, and the Mica→Acrylic and opaque→transparent reversals both retain the original reasoning and what invalidated it.

---

## 5. Critical Risks and Issues

| ID | Priority | Area | Issue | Difficulty | Impact | Recommended Action |
|---|---|---|---|---|---|---|
| P0-1 | P0 | Security / Legal | First-party client impersonation in Pathfinder headers; contradicts the project's own stated hard line | Medium | Critical | Remove spoofed UA/Origin/platform headers; accept degraded artist overview |
| P0-2 | P0 | Legal / DevOps | No LICENSE, placeholder bundle identifier, unsigned installer, no updater | Low | Critical | Add LICENSE, fix identifier, decide distribution model before any further feature work |
| P1-1 | P1 | DevOps | No CI at all — every gate is manual | Low | High | Add GitHub Actions running the three existing gates + clippy + fmt |
| P1-2 | P1 | Testing | Zero runtime/integration coverage; no frontend test framework | High | High | Add integration tests for auth state machine; add Vitest for `store.svelte.ts` |
| P1-3 | P1 | Reliability | `ensure_jams` build race → duplicate dealer subscriptions + detached tasks | Low | High | Hold the write lock across build, or use `OnceCell`/`Mutex` guard |
| P1-4 | P1 | Reliability | `JamController::_forward` is detached, never aborted; module doc claims it is aborted | Low | High | Store abortable handle, implement `Drop`, or correct the doc |
| P1-5 | P1 | Security | Refresh tokens persisted as plaintext JSON with default ACLs | Medium | High | Wrap with Windows DPAPI (`CryptProtectData`) or the credential manager |
| P1-6 | P1 | Product / Arch | Feature surface exceeds one maintainer's capacity against a hostile moving API | High | High | Formally tier features; freeze or remove the bottom tier |
| P1-7 | P1 | Architecture | `commands.rs` at 2,128 LOC / 100+ commands; 3-site registration unenforced at compile time | Medium | Medium | Split by domain; add a registration-parity test |
| P2-1 | P2 | Security | `tauri-plugin-mcp-bridge` in the release dependency graph; capability granted unconditionally | Low | Medium | Move to `[target.'cfg(debug_assertions)']`-equivalent gating; scope the capability |
| P2-2 | P2 | Code quality | `NowPlaying.svelte` at 1,166 LOC | Medium | Medium | Extract lyrics pane, sleep-timer, and fullscreen chrome |
| P2-3 | P2 | Dependencies | No `cargo audit`/`cargo-deny`; 1,157-node tree incl. git-pinned librespot | Low | Medium | Add `cargo-deny` to CI |
| P2-4 | P2 | Architecture | Event names and command names duplicated Rust↔TS with nothing enforcing the match | Medium | Medium | Generate the TS surface, or add a parity test |
| P2-5 | P2 | Security | `withGlobalTauri: true` exposes `window.__TAURI__` unnecessarily | Extra small | Low | Set to `false`; the app imports from `@tauri-apps/api` |
| P3-1 | P3 | Docs | README documents *this machine's* tool versions as a prerequisites table | Extra small | Low | Replace with minimum versions |
| P3-2 | P3 | Repo hygiene | `BACKUP/` hidden via `.git/info/exclude` (local-only, unshared) | Extra small | Low | Delete it or move it to `.gitignore` |
| P3-3 | P3 | Docs | Footprint numbers stale by the README's own admission | Small | Low | Re-measure or delete the table |
| P3-4 | P3 | Reliability | `JamSession::events()` panics on second call | Extra small | Low | Return `Option`/`Result` |
| P4-1 | P4 | Performance | WebView2 is ~151 MB of the 157 MB footprint | Extra large | Low | Do not pursue |
| P4-2 | P4 | Feature | DJ narration needs a second audio pipeline | Large | Low | Defer indefinitely |

---

# 6. Detailed Findings by Priority

Findings are enumerated in full under sections 7–11 to avoid duplicating them twice in one document.

---

# 7. P0 — Critical / Hardest Issues

---

## [P0-1] The app impersonates Spotify's first-party desktop client

**Priority:** P0
**Area:** Security / Legal / Product integrity
**Difficulty:** Medium
**Impact:** Critical
**Confidence:** Confirmed (headers), Likely (enforcement consequences)
**Evidence:** `src-tauri/src/spotify/pathfinder.rs:163-200`. Contradicted by `src-tauri/src/telemetry.rs:1-8`.

### Problem

`query_hash` branches on `operation == "queryArtistOverview"` and, when true, sends:

```rust
.header("app-platform", "Win32_x86_64")
.header("Origin", "https://xpui.app.spotify.com")
.header("Referer", "https://xpui.app.spotify.com/")
.header("spotify-app-version", "896000000")
.header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ... Spotify/1.2.88.483 Safari/537.36")
```

`xpui.app.spotify.com` is the official desktop client's internal origin. `Spotify/1.2.88.483` is a specific official client build string. Neither is a User-Agent this application is entitled to send. The only purpose of these headers is to make Spotify's edge believe the request came from its own desktop app.

Meanwhile `telemetry.rs` opens with:

> "Public references only make it accept third-party traffic by impersonating desktop-client context and anti-fraud fields. **Rustify deliberately does not do that.**"

And `README.md:176-179` repeats it: impersonation is "a hard line, not a missing feature."

That statement is false as of the current tree. The project holds the line on Gabo telemetry and crosses it on Pathfinder. Whatever one thinks of the line, a codebase that documents a policy it does not follow is worse than one with no policy — it teaches future readers to trust claims that are not load-bearing.

This is compounded by the rest of the private surface: `spclient.wg.spotify.com` social-connect calls, `clienttoken.spotify.com` minting, scraped `sha256Hash` persisted queries harvested out of Spotify's webpack bundles (`find_operation_hash`, `webpack_chunks`), and dealer websocket subscriptions.

### Current Behavior

Artist overview requests are disguised as official-desktop-client traffic. Everything else on Pathfinder claims `WebPlayer`/`open.spotify.com`, which is a milder but still inaccurate claim.

### Desired Behavior

Every outbound request identifies this application honestly. `webapi.rs` already gets this right — `user_agent(concat!("rustify/", env!("CARGO_PKG_VERSION")))`. The Pathfinder client should do the same, and the feature should degrade or disappear if Spotify rejects an honest client.

### Impact

- **Account risk to the user.** Spotify's ToS prohibit circumventing client identification. The realistic enforcement outcome is a rate-limit or ban on the *account*, not the app — and the account is the user's paid Premium subscription.
- **Legal risk to the maintainer** if the project is ever published (currently it cannot be — see P0-2 — which is the only thing containing this).
- **Correctness risk.** `Spotify/1.2.88.483` is a hardcoded, already-stale version string. When Spotify's edge starts version-gating, this fails in a way that looks like a random 403.
- **Integrity risk.** The documented policy no longer describes the code.

### Recommended Fix

Remove the `desktop_artist` header branch. Send the honest `rustify/{version}` UA for all Pathfinder operations. Keep `client-token` and the OAuth bearer — those are credentials librespot legitimately holds, not identity claims.

If `queryArtistOverview` then 403s (likely), take the existing documented fallback: `README.md:181-185` already describes an ad-hoc top-tracks sample built from recent albums. Ship that, and label the artist page accurately.

If you decide the feature is worth the risk, that is your call to make — but then **delete the impersonation-is-a-hard-line claims** from `telemetry.rs`, `README.md`, and `CLAUDE.md`, and replace them with an accurate statement of what the project does and what it exposes the user's account to. Do not leave both in the tree.

### Implementation Plan

1. Delete the `let desktop_artist = ...` binding and both conditional header blocks in `pathfinder.rs:163-200`; use `WebPlayer`/`open.spotify.com` uniformly, or drop the origin headers entirely.
2. Set the Pathfinder `reqwest::Client` UA to `rustify/{CARGO_PKG_VERSION}`, matching `webapi.rs:29`.
3. Run the app against a live account and record which operations still succeed. **This step cannot be skipped** — it is the only way to know.
4. For each operation that now fails, either wire the documented fallback or remove the feature and its command.
5. Update `README.md` "Known limitations" and the `obsidian/concepts/` note covering Pathfinder with the measured result.
6. Add a one-line policy to `CLAUDE.md`: *no request may claim an identity this application does not have.* Grep for `Spotify/1.` and `xpui` in review.

### Acceptance Criteria

- [ ] No source file sends a `User-Agent`, `Origin`, `Referer`, or version header claiming to be an official Spotify client.
- [ ] Every outbound HTTP client sends `rustify/{version}`.
- [ ] The live behaviour of each Pathfinder operation post-change is recorded in `obsidian/`.
- [ ] Features that no longer work are removed or explicitly degraded — not left silently 403ing.
- [ ] Docs and code agree on the impersonation policy, in whichever direction you choose.

### Estimated Effort

Medium (half day to one day), dominated by live re-testing, not by the edit.

### Dependencies

Live Spotify Premium credentials. A willingness to lose the artist-overview data.

### Risks if Ignored

Account suspension for the user. A stale hardcoded version string that fails opaquely. A codebase whose documented values cannot be trusted, which degrades the value of *all* the other excellent documentation.

---

## [P0-2] The project cannot be legally or safely distributed

**Priority:** P0
**Area:** Legal / DevOps
**Difficulty:** Low (each item), High (the decision behind them)
**Impact:** Critical
**Confidence:** Confirmed
**Evidence:** No `LICENSE`/`COPYING` at repo root (verified). `src-tauri/tauri.conf.json:5` — `"identifier": "dev.local.rustify"`. `tauri.conf.json:35-44` — NSIS bundle with no `signCommand`/certificate config. No updater plugin in `Cargo.toml`. `version: "0.1.0"` in both manifests across 81 commits.

### Problem

Four independent blockers, each individually small, which together mean this cannot be given to anyone:

1. **No license.** Under default copyright, nobody may legally copy, run, or fork it. The repo has a GitHub remote (`origin/main`), so this is already published source that nobody is permitted to use.
2. **Placeholder identifier.** `dev.local.rustify` is scaffold output. `CLAUDE.md` correctly warns that changing the identifier orphans `tokens.json` and forces re-login — so fixing this gets *more* expensive with every user, and there is currently exactly one.
3. **Unsigned NSIS installer.** Windows SmartScreen will block it. Users are trained to click through, which is the exact habit that gets people compromised.
4. **No updater.** If P0-1 or a librespot CVE required a fix, there is no channel to deliver it.

### Current Behavior

`npm run tauri build` produces an unsigned, unlicensed, placeholder-identified installer with no update path.

### Desired Behavior

Either (a) the repository states plainly that it is a personal project not intended for distribution, and stops there — or (b) it has a license, a real reverse-DNS identifier, a signed installer, and an update channel.

### Impact

Legal exposure. A user-hostile install experience. No security-patch delivery mechanism. Combined with P0-1, an unfixable account-risk exposure in anything already handed to someone.

### Recommended Fix

**Decide (a) or (b) first.** Everything else follows from that, and much of this review's P1 work is only justified under (b).

Given "Personal use" in `README.md:8`, one maintainer, and a Premium-only, Windows-only, private-API-dependent app, **(a) is the correct choice.** Recommendation: pick (a), and stop treating distribution readiness as a backlog item.

Under (a):
1. Add `LICENSE` — MIT or Apache-2.0 (permissive is simplest; you retain no meaningful control regardless).
2. Add a prominent README banner: personal project, not distributed, no support, uses private Spotify APIs at the user's own risk.
3. Fix the identifier now, while the migration cost is one manual re-login.
4. Explicitly drop signing and the updater from scope, in writing, so they stop being invisible debt.

Under (b), add to the above: an Authenticode certificate (~$200–400/yr), `tauri-plugin-updater` with signed manifests, a real versioning policy, and — non-negotiably — P0-1 resolved first.

### Implementation Plan

1. Choose (a) or (b). Write the decision into `README.md`.
2. `LICENSE` at repo root; add the SPDX id to `Cargo.toml` and `package.json`.
3. Change `identifier` to something real (`com.<yourdomain>.rustify` or `io.github.<user>.rustify`). Log out first; expect a fresh login.
4. Bump `version` past `0.1.0` and note in `CLAUDE.md` when it gets bumped.
5. Under (b) only: certificate, `signCommand`, updater plugin, release workflow.

### Acceptance Criteria

- [ ] `LICENSE` exists and is referenced from both manifests.
- [ ] `identifier` contains no `dev.local`.
- [ ] README states the distribution posture in its first screen.
- [ ] Signing/updater are either implemented or explicitly out of scope in writing.

### Estimated Effort

Small (under 2 hours) for (a). Extra large for (b).

### Dependencies

None for (a). Certificate purchase and identity verification for (b).

### Risks if Ignored

Continued unlicensed publication. A migration cost that grows with every user. Users trained to bypass SmartScreen. No way to ship a security fix.

---

# 8. P1 — High Difficulty / High Impact Issues

---

## [P1-1] No CI exists

**Priority:** P1 | **Area:** DevOps | **Difficulty:** Low | **Impact:** High | **Confidence:** Confirmed
**Evidence:** `.github/` does not exist (verified). `CLAUDE.md` describes the verification loop as commands a human types.

### Problem

The three gates that do work — `cargo check --no-default-features`, `cargo test --lib`, `npm run check` — are entirely dependent on someone remembering. There is also no `clippy` and no `cargo fmt --check` anywhere in the documented loop, so lint and formatting drift are unpoliced.

### Current / Desired Behavior

Currently: a commit that breaks the build can land and only be discovered on the next manual run. Desired: every push runs all gates; `main` cannot regress silently.

### Impact

Regressions land undetected. Difficulty compounds — three months of unlinted code is a large cleanup; three days is trivial. This is the cheapest high-leverage fix in the review.

### Recommended Fix

One GitHub Actions workflow on `windows-latest` with a Rust + npm cache.

### Implementation Plan

1. `.github/workflows/ci.yml`, triggers `push` + `pull_request`.
2. Steps: `npm ci` → `npm run check` → `cargo fmt --check` → `cargo clippy --no-default-features -- -D warnings` → `cargo check --no-default-features` → `cargo test --no-default-features --lib`.
3. Add `Swatinem/rust-cache` — librespot from git is the long pole; without caching this is a many-minute job.
4. Run clippy locally first and fix or `#[allow]`-with-justification the existing findings before turning on `-D warnings`.
5. Add a `cargo-deny` step once P2-3 lands.

### Acceptance Criteria

- [ ] Workflow runs on every push to `main` and every PR.
- [ ] All five gates green on `main`.
- [ ] Clippy runs with `-D warnings` and no blanket crate-level allows.
- [ ] Cached run completes in under ~10 minutes.

### Estimated Effort

Small (under 2 hours), plus however long the first clippy cleanup takes.

### Dependencies

None. Do this first.

### Risks if Ignored

Slow, invisible quality erosion — precisely the thing this codebase's documentation discipline is otherwise fighting.

---

## [P1-2] The test suite verifies nothing about how the app actually behaves

**Priority:** P1 | **Area:** Testing | **Difficulty:** High | **Impact:** High | **Confidence:** Confirmed
**Evidence:** 106 tests across 29 files, all `#[cfg(test)]` unit tests over fixed payloads. No `tests/` directory. No `vitest`/`@testing-library` in `package.json`. `CLAUDE.md` states this outright.

### Problem

The tests that exist are good — `telemetry::records_only_elapsed_listening_not_seek_distance` and `home::removes_duplicate_cards_and_rejects_unsafe_artwork_urls` test real invariants, not getters. But they are all parsers and pure functions.

Everything genuinely dangerous in this codebase is untested:

- The session-generation watchdog (`state.rs:262-277` + `player::spawn_event_pump`) — the mechanism preventing an ordinary logout from logging the user straight back in.
- `save_stored_tokens` merge behaviour — the fix for the self-sustaining 429 loop. There is no test asserting that writing `webapi_refresh_token: None` preserves the stored value.
- `restore_login`'s degrade-don't-fail path, which exists to prevent permanent lockout after a rotated streaming token.
- Session replacement teardown in `establish` (`commands.rs:702-716`).
- The entire frontend. `store.svelte.ts` holds the auth state machine, the 1 Hz position ticker, optimistic settings rollback, and lyrics request-generation cancellation. None of it is tested.

`CLAUDE.md` says "a green run says nothing about runtime behaviour." Correct, and stated honestly — but it has been true long enough that it is now a decision, not an observation.

### Current / Desired Behavior

Currently the safety-critical recovery machinery is verified only by reading it. Desired: the pure logic in that machinery is tested at the seams, and the frontend store has a test framework.

### Impact

Any refactor of auth or session lifecycle is unguarded. The failure mode is not a crash — it is a user silently logged out, or a credential silently erased, discovered days later. That is the worst kind of bug to have no test for.

### Recommended Fix

Do not attempt to launch Tauri in tests — that is a poor return. Instead, extract the decision logic from the I/O and test the decisions.

Highest value first:

1. **`save_stored_tokens` merge** — pure filesystem logic, testable today with `tempfile`, directly protects the documented 429 loop.
2. **Session generation** — assert `next_session_generation` monotonicity and that a stale generation is rejected. Extract the comparison into a free function if needed.
3. **`is_grant_rejected`** — table test over real Spotify error strings, including near-misses that must *not* clear tokens.
4. **`PlaybackState::set_position`/`refresh_position`** — assert a `VolumeChanged`-shaped update does not reset the clock. This is the documented bug; it deserves a regression test.
5. **Frontend:** add Vitest, test `store.svelte.ts` against a mocked `invoke` — `handleError` clearing auth on `SessionExpired`, the ticker starting/stopping, `#syncLyrics` generation cancellation, `toggleFriendsPanel` rollback.

### Implementation Plan

1. `cargo add --dev tempfile`; write the `save_stored_tokens` merge tests (3 cases: preserve on `None`, overwrite on `Some`, `clear_stored_tokens` removes).
2. Add the `is_grant_rejected` table test.
3. Add the position-anchor regression test.
4. `npm i -D vitest @testing-library/svelte jsdom`; add `"test": "vitest run"`; wire into CI.
5. Write 4–6 store tests with `vi.mock("@tauri-apps/api/core")`.
6. Add the two new commands to `CLAUDE.md`'s verification loop.

### Acceptance Criteria

- [ ] Token merge, grant rejection, and position anchoring each have regression tests naming the bug they prevent.
- [ ] `npm test` exists, runs in CI, and covers `store.svelte.ts`'s auth/ticker/lyrics paths.
- [ ] `CLAUDE.md`'s verification loop lists all four gates.
- [ ] The manual checklist in `README.md` stays — it covers what unit tests cannot, and should be explicitly labelled as such.

### Estimated Effort

Large (multiple days). Steps 1–3 alone are Medium and deliver most of the value.

### Dependencies

P1-1 (CI), so the tests actually run.

### Risks if Ignored

A future auth refactor reintroduces the token-erasure loop. The documentation warning about it will not stop that — it did not stop it the first time.

---

## [P1-3] `ensure_jams` races: two concurrent jam commands build two controllers

**Priority:** P1 | **Area:** Reliability / Concurrency | **Difficulty:** Low | **Impact:** High | **Confidence:** Confirmed
**Evidence:** `src-tauri/src/commands.rs:1972-1992`.

### Problem

```rust
if let Some(ctrl) = state.jams.read().await.as_ref() { return Ok(ctrl.clone()); }
// ... read guard released ...
let ctrl = Arc::new(JamController::build(app, session, &state.tokens).await?);
let mut guard = state.jams.write().await;
if let Some(existing) = guard.as_ref() { return Ok(existing.clone()); }
```

The double-check on the write guard correctly prevents *storing* two controllers. It does not prevent *building* two. Between the read-guard release and the write-guard acquisition sits `JamController::build`, which does network I/O and — critically — has side effects that survive the losing controller being discarded:

- `session.dealer().add_listen_for("social-connect/v2/session_update")` and `..._broadcast_status_update` (`jams_bridge.rs:302-313`) register **two more** subscriptions on the shared librespot dealer.
- `tauri::async_runtime::spawn` (`jams_bridge.rs:317`) starts a forward task that owns those receivers and an `AppHandle`.

The loser's `Arc<JamController>` is dropped, which drops `_forward` — and per this project's own repeatedly-documented rule, **dropping a `JoinHandle` detaches rather than cancels.** The orphan task keeps emitting `jams:changed` to the webview.

The UI can trigger this: `Jams.svelte` calling `getJamStatus()` on mount while the user clicks Create fires two commands that both call `ensure_jams`.

### Current / Desired Behavior

Currently: duplicate dealer subscriptions and a detached emitter per race. Desired: exactly one controller is built per session, ever.

### Impact

Duplicate `jams:changed` events → duplicated or flickering UI state. Leaked dealer subscriptions on a connection shared with Connect and friend presence. An orphan task holding an `AppHandle` past logout. Difficult to diagnose because it is timing-dependent.

### Recommended Fix

Hold the write guard across the build. `JamController::build` is `async`, so this holds a `tokio::RwLock` write guard across an await — acceptable here because `state.jams` is contended only by jam commands, and serialising them is the desired behaviour. Alternatively use `tokio::sync::OnceCell` with `get_or_try_init`, which is the idiomatic fit.

### Implementation Plan

1. Prefer `OnceCell`: change `pub jams: RwLock<Option<Arc<JamController>>>` to `tokio::sync::OnceCell<Arc<JamController>>` in `state.rs`.
2. Rewrite `ensure_jams` as `state.jams.get_or_try_init(|| async { ... }).await.cloned()`.
3. Confirm `logout`'s teardown still works — `OnceCell` has no `take()`, so you will need `RwLock<Option<...>>` with the guard held across the build instead, *or* keep the outer `RwLock` and put a `Mutex` build-lock inside it. Pick whichever keeps logout teardown correct; do not lose it.
4. Add a test spawning two concurrent `ensure_jams` calls against a stub builder, asserting the builder ran once.

### Acceptance Criteria

- [ ] Two concurrent jam commands result in exactly one `JamController::build`.
- [ ] Logout still tears the controller down.
- [ ] A concurrency test covers it.

### Estimated Effort

Small (under 2 hours).

### Dependencies

Resolve alongside P1-4 — same file, same lifecycle.

### Risks if Ignored

Duplicate dealer subscriptions and orphaned emitters, in the one module that already talks to the most fragile private service.

---

## [P1-4] The jam forward task is detached and never aborted — and the docs say otherwise

**Priority:** P1 | **Area:** Reliability | **Difficulty:** Low | **Impact:** High | **Confidence:** Confirmed (the doc/code mismatch), Likely (the leak)
**Evidence:** `src-tauri/src/jams_bridge.rs:8-9`, `:240`, `:317`, `:362`. `src-tauri/src/commands.rs:894-896`. No `.abort()` and no `impl Drop` anywhere in `jams_bridge.rs` (verified by grep).

### Problem

The module doc states:

> "The controller is built lazily on the first jam command and dropped on logout, **which aborts the dealer task and ends the forward loop.**"

`logout` echoes it:

```rust
// Drop the jam controller too: its dealer listener and event forwarder
// must not outlive the session they authenticate against.
state.jams.write().await.take();
```

But the handle is stored as `_forward: tauri::async_runtime::JoinHandle<()>` and nothing calls `.abort()`. Dropping it detaches. This is the *exact* hazard `state.rs:150` and `commands.rs:702-706` warn about for every other background task — and every other task (`refresh_task`, `remote_task`, `connect_state_task`, `friends_task`) is explicitly aborted in `logout`. Jams is the one that was missed.

The forward loop exits only when both `session_updates.next()` and `broadcast_updates.next()` yield `None`. Those receivers are moved into the task, so they are not dropped with the controller. Termination therefore depends on librespot closing the dealer when the last `Session` clone drops — plausible, since the controller holds a `Session` clone that does drop, but **unverified**, and it is not what the comment claims is happening.

The difference matters: as written, correctness depends on a librespot implementation detail rather than on an explicit abort this code controls.

### Current / Desired Behavior

Currently: termination is indirect, undocumented-in-reality, and unverified. Desired: an explicit `.abort()`, matching every sibling task.

### Impact

Best case the docs are wrong and the behaviour is accidentally right. Worst case an orphaned task holds an `AppHandle` and emits `jams:changed` after logout, with the webview receiving jam events for a session that no longer exists. Combined with P1-3, one per race.

### Recommended Fix

Rename `_forward` to `forward`, and add:

```rust
impl Drop for JamController {
    fn drop(&mut self) {
        self.forward.abort();
    }
}
```

`Drop` is preferable to an explicit call in `logout` because it also covers P1-3's discarded loser and any future drop site.

### Implementation Plan

1. Rename the field; add `impl Drop`.
2. Correct the module doc at `jams_bridge.rs:8-9` to describe what the code does.
3. Correct the comment at `commands.rs:894-895`.
4. Verify at runtime: log in, create/refresh a jam, log out, confirm no further `jams:` log lines and that a re-login produces exactly one forwarder.
5. Add the "every spawned task needs an explicit abort path" rule to `CLAUDE.md`'s background-task section — it is already implied but was missed once.

### Acceptance Criteria

- [ ] `JamController` aborts its forward task on drop.
- [ ] Module doc and `logout` comment match the implementation.
- [ ] Manually verified: no `jams:changed` emission after logout.

### Estimated Effort

Extra small (under 30 minutes) for the fix; Small including verification.

### Dependencies

Do with P1-3.

### Risks if Ignored

A background task emitting events with dead credentials, and a documentation claim that will mislead the next person to read it — including you, in six months.

---

## [P1-5] Refresh tokens are stored as plaintext with default file permissions

**Priority:** P1 | **Area:** Security | **Difficulty:** Medium | **Impact:** High | **Confidence:** Confirmed
**Evidence:** `src-tauri/src/auth.rs:446-450` — `std::fs::write(tokens_path(data_dir), raw)` with no ACL adjustment. `StoredTokens` is a plain serde struct holding two long-lived refresh tokens.

### Problem

`tokens.json` in `%APPDATA%\dev.local.rustify\` contains two Spotify refresh tokens in clear text. They are long-lived (Spotify's are ~6 months per the code's own comment at `auth.rs:928`) and carry the full `STREAMING_SCOPES` union — including `user-library-modify`, `playlist-modify-*`, `user-follow-modify`, and `ugc-image-upload`.

Anyone or anything running as that Windows user can read the file and obtain durable, write-capable access to the victim's Spotify account. That includes any malware, any other installed app, and any backup/sync tool pointed at `%APPDATA%`.

To be fair: this is the same posture as most Electron desktop apps, `%APPDATA%` is per-user ACL'd, and there is no cross-process sandbox on Windows to appeal to. It is not a catastrophic defect. But it is below the bar this codebase sets for itself everywhere else — a project that redacts join secrets from debug logs and unit-tests that `device_code` never serialises should not leave the long-lived credential in cleartext on disk.

### Current / Desired Behavior

Currently: plaintext JSON, default inherited ACLs. Desired: encrypted at rest with a user-bound key, so a stolen file is useless on another machine or under another account.

### Impact

Local-attacker or malware access to the file yields persistent Spotify account access, including library and playlist mutation, that survives password changes until the user explicitly revokes app access.

### Recommended Fix

Windows DPAPI (`CryptProtectData` / `CryptUnprotectData` with `CRYPTPROTECT_LOCAL_MACHINE` **unset**, so the key is bound to the user account). The `windows` crate exposes it; the app is already Windows-only.

Keep it contained to `auth.rs`: encrypt in `save_stored_tokens`, decrypt in `load_stored_tokens`. Nothing else needs to know.

### Implementation Plan

1. Add the `windows` crate with the `Win32_Security_Cryptography` feature.
2. Write `protect(&[u8]) -> Vec<u8>` / `unprotect(&[u8]) -> Option<Vec<u8>>` helpers in `auth.rs`.
3. In `save_stored_tokens`, serialise then `protect`, and write the ciphertext. **Preserve the existing merge logic** — it reads the previous file first, so decrypt-merge-encrypt, and do not let the round trip drop the merge (that would reintroduce the documented 429 loop).
4. In `load_stored_tokens`, try `unprotect` first; on failure, fall back to parsing as plaintext and immediately re-save encrypted. This migrates existing installs without a forced re-login.
5. After a release cycle, remove the plaintext fallback.
6. Test: fresh install, upgrade-from-plaintext, and corrupted-file (must return `None`, not panic — `load_stored_tokens` already returns `Option`, keep that).

### Acceptance Criteria

- [ ] `tokens.json` is not human-readable after login.
- [ ] An existing plaintext file is migrated silently, with no re-login.
- [ ] A corrupted/foreign file degrades to "no stored tokens", never a panic.
- [ ] The merge-preserve behaviour of `save_stored_tokens` still holds, with the P1-2 test proving it.

### Estimated Effort

Medium (half day to one day).

### Dependencies

The P1-2 merge test should land first, so the encryption change is guarded.

### Risks if Ignored

Persistent account takeover from any local read. Low likelihood, high consequence, and inconsistent with the project's own standards.

---

## [P1-6] The feature surface exceeds what one maintainer can sustain

**Priority:** P1 | **Area:** Product / Architecture | **Difficulty:** High | **Impact:** High | **Confidence:** Confirmed
**Evidence:** 100+ registered Tauri commands (`lib.rs:107-201`). 26 of 45 Rust files reference private Spotify hosts or internal endpoints. `README.md:157-193` lists nine known limitations, most of them upstream removals. `error.rs` carries four distinct variants — `FeatureUnsupported`, `PublicApiLimitation`, `EndpointNotAvailable`, `PersistedQueryExpired` — that exist purely to describe ways Spotify has taken something away.

### Problem

A "lightweight native Spotify client" (`README.md:3`) has grown to include Jams, DJ with Lexicon fallback, lyrics, track credits, concerts, music videos, friend presence, lossless capability probing, playback telemetry, Listening DNA, Last.fm enrichment, personalised Home, user profiles, and user search.

The maintenance cost is not proportional to feature count — it is proportional to *private-API-dependent* feature count, and it is paid continuously and without warning. The repo already documents four such breakages in a single year:

- `/v1/audio-features` closed to Development Mode apps → DNA rebuilt on genres + popularity.
- `/artists/{id}/top-tracks` restricted (Feb 2026) → `PublicApiLimitation` variant added.
- `product` removed from `/me` for newer apps (Feb 2026) → Premium gate made conditional.
- Lexicon 403s for most accounts → DJ fallback path added.

That is roughly one breakage per quarter, each requiring diagnosis against an undocumented service with no error contract. The `error.rs` variants are, read one way, a scar tissue map.

The code handles each individual breakage well. That is not the same as the portfolio being sustainable. Every added private-API feature raises the standing probability that *something* is broken on any given day, and there is no monitoring — the user finds out.

To be clear: this is not a code problem and it is not fixable by refactoring. It is a scope decision.

### Current / Desired Behavior

Currently: everything is nominally supported, nothing is prioritised, breakage is discovered by use. Desired: an explicit tier list, where the bottom tier is either frozen or deleted.

### Impact

Diffuse maintenance load. Rising baseline broken-feature rate. Attention pulled away from P0/P1 work (this review found no CI and no license on a project with a 111-note documentation vault — that is a prioritisation signal).

### Recommended Fix

Tier the features and write the tiers down.

- **Tier 1 — core, must always work.** Auth, playback, Connect, library, search, queue. Public API + librespot only. Any breakage is a P0.
- **Tier 2 — supported extras.** Lyrics, friend presence, Home, credits. Private-API-backed but with defined degradation. Breakage is a P2.
- **Tier 3 — experimental, unsupported.** Jams, DJ, music videos, lossless probing, telemetry. Explicitly best-effort. Breakage is closed as "expected."

Then act on it: for anything in Tier 3 you have not used in a month, delete it. `README_jams.md` is 15,161 bytes of documentation for a feature that requires captured Pathfinder hashes to fully function and whose config file (`jams.toml`) most installs will never have.

### Implementation Plan

1. Write the tier list into `CLAUDE.md` with each current feature assigned.
2. Surface Tier 3 in the UI as experimental — the code already knows (`JamStatus.pathfinder_hashes` count, DJ's `reason: "lexicon-unavailable-fallback"`); the labelling should follow.
3. Institute a rule: no new Tier 3 features until CI, license, and integration tests exist.
4. Review Tier 3 quarterly against actual use. Delete, do not deprecate — the git history preserves it, and `obsidian/` preserves the reasoning.
5. For each Tier 1/2 feature, verify the documented degradation path actually fires (several are asserted in comments, not tests).

### Acceptance Criteria

- [ ] Every feature is assigned a tier in `CLAUDE.md`.
- [ ] Tier 3 is visibly labelled experimental in the UI.
- [ ] At least one unused Tier 3 feature has been deleted.
- [ ] A written rule gates new Tier 3 work behind the infrastructure items.

### Estimated Effort

Medium for the tiering; Large if deletion is done properly.

### Dependencies

Requires the P0-2 decision — under "personal use," Tier 3 is much more defensible than under "distributed."

### Risks if Ignored

Slow drift toward a project where a rising fraction of features are quietly broken, and the documentation describing them becomes a record of what used to work.

---

## [P1-7] `commands.rs` is a 2,128-line grab bag with unenforced three-site registration

**Priority:** P1 | **Area:** Architecture | **Difficulty:** Medium | **Impact:** Medium | **Confidence:** Confirmed
**Evidence:** `src-tauri/src/commands.rs` — 2,128 lines, largest file by 1,000 lines. ~100 `#[tauri::command]` functions plus non-trivial logic (`establish` at 625-767, `start_dj_fallback` at 356-429, artist-overview caching at 1574-1673). `CLAUDE.md` documents the three-edit requirement and that missing edits 2 or 3 fail at runtime only.

### Problem

Two coupled issues.

**Size.** The file mixes thin dispatchers with substantial logic. `establish` is a 140-line session-lifecycle orchestrator handling token persistence, generation claiming, audio config, the Premium gate, librespot startup, four task spawns, old-session teardown, and error rollback. It belongs in `player.rs` or its own `session.rs`.

**Unenforced registration.** Adding a command requires edits in `commands.rs`, `lib.rs:generate_handler![]`, and `src/lib/api.ts`. Miss the second or third and it compiles cleanly and fails at runtime with "command not found." Same for the event-name constants duplicated between `state::events` and `api.ts:44-49`. With 100+ commands and 6 events this is a real, recurring hazard — `CLAUDE.md` documents it as a known trap rather than fixing it.

### Current / Desired Behavior

Currently a runtime-only failure mode on a routine operation. Desired: compile-time or CI-time detection.

### Impact

Runtime "command not found" surfaces as a broken feature with no compile error. Navigating a 2,128-line file slows every change. `establish`'s complexity is where a lifecycle bug would hide.

### Recommended Fix

Split by domain, then add a parity test. Do these as separate commits.

### Implementation Plan

1. Move `establish`, `login`, `restore_session`, `logout`, and device-authorization commands into `commands/session.rs`. Pure move, no logic change, verify with `cargo check`.
2. Split the rest: `commands/library.rs`, `commands/playback.rs`, `commands/jams.rs`, `commands/discovery.rs` (DJ/Home/DNA/lyrics/profiles). Target under 600 lines each. Keep `commands/mod.rs` re-exporting so `generate_handler![]` needs no change.
3. Add a build-time or test-time parity check: a test that reads `lib.rs` and `src/lib/api.ts` as text, extracts the handler list and the `invoke("...")` call sites, and asserts they match the set of `#[tauri::command]` functions. Crude but effective, and cheap.
4. Do the same for the six event-name constants.
5. Add both to CI.

### Acceptance Criteria

- [ ] No file in `src-tauri/src/commands/` exceeds ~600 lines.
- [ ] `establish` lives outside the command dispatch layer.
- [ ] A test fails when a command is registered in Rust but missing from `api.ts`, or vice versa.
- [ ] A test fails when event-name constants diverge.
- [ ] `CLAUDE.md`'s "three edits" section notes that edits 2 and 3 are now checked.

### Estimated Effort

Medium (half day to one day).

### Dependencies

P1-1, so the parity test runs automatically.

### Risks if Ignored

Recurring runtime-only failures on routine work, and a lifecycle function complex enough to hide a session bug.

---

# 9. P2 — Medium Difficulty / Important Issues

---

## [P2-1] The MCP automation bridge ships in the release dependency graph

**Priority:** P2 | **Area:** Security / Dependencies | **Difficulty:** Low | **Impact:** Medium | **Confidence:** Confirmed
**Evidence:** `Cargo.toml:20` — `tauri-plugin-mcp-bridge = "0.12"` as an unconditional dependency. `lib.rs:92-100` gates *activation* behind `#[cfg(debug_assertions)]`. `capabilities/default.json:16` grants `mcp-bridge:default` unconditionally.

### Problem

The runtime gating is right, and binding to `127.0.0.1` instead of the default `0.0.0.0` for an unauthenticated control channel is a good catch — credit where due. But the crate is still a normal dependency, so its code and its entire transitive tree are compiled into and linked against release builds. And the capability is granted with no `cfg` condition, so the release manifest advertises a permission for a plugin that is not loaded.

`tauri-plugin-mcp-bridge` is a third-party crate at 0.12 whose purpose is remote control of the app. It is the highest-consequence dependency in the tree and it exists purely for development convenience.

### Current / Desired Behavior

Currently: present in release, inactive. Desired: absent from release entirely.

### Impact

Unnecessary attack surface and supply-chain exposure in shipped binaries. A future refactor that loosens the `cfg` gate — or an upstream change that self-registers — turns an inactive dependency into a live unauthenticated control channel with no compile error.

### Recommended Fix

Put it behind a Cargo feature that is off by default, and condition the capability the same way.

### Implementation Plan

1. Add `[features] mcp-bridge = ["dep:tauri-plugin-mcp-bridge"]` and mark the dependency `optional = true`.
2. Change the `lib.rs` gate to `#[cfg(all(debug_assertions, feature = "mcp-bridge"))]`.
3. Move `mcp-bridge:default` into a separate capability file with a platform/feature condition, or accept the unused permission and document why.
4. Document in `CLAUDE.md` that the dev loop needs `--features mcp-bridge`, and update the documented `cargo check` command accordingly.
5. Verify with `cargo tree --no-default-features` that it is gone from the release graph.

### Acceptance Criteria

- [ ] `cargo tree` on a default release build shows no `tauri-plugin-mcp-bridge`.
- [ ] The dev workflow still works with the feature flag, documented in `CLAUDE.md`.

### Estimated Effort

Small (under 2 hours).

### Dependencies

None.

### Risks if Ignored

An unauthenticated automation channel one `cfg` mistake away from shipping.

---

## [P2-2] `NowPlaying.svelte` is a 1,166-line component

**Priority:** P2 | **Area:** Code quality | **Difficulty:** Medium | **Impact:** Medium | **Confidence:** Confirmed
**Evidence:** 1,166 lines — more than double the next-largest frontend file (`PlayerBar.svelte`, 512).

### Problem

The file owns the full-screen now-playing view, the lyrics pane with scroll sync, the sleep-timer clock (`:446`, a 1 Hz interval), and the fullscreen drag-region handling documented in `CLAUDE.md`. Four responsibilities, one file, no tests.

### Current / Desired Behavior

Currently one component. Desired: three or four focused components under ~400 lines each.

### Impact

Hard to test (P1-2), hard to review, and Svelte 5 reactivity bugs in a file this size are hard to localise. The `data-tauri-drag-region={fullscreen ? undefined : true}` subtlety — where `false` renders an attribute Tauri still honours — is exactly the class of bug that hides here.

### Recommended Fix

Extract `LyricsPane.svelte`, `SleepTimerBadge.svelte`, and `FullscreenChrome.svelte`. Keep the parent as layout and state wiring.

### Implementation Plan

1. Extract the lyrics pane first — it is the most self-contained (props: `lyrics`, `positionMs`, `loading`, `error`).
2. Extract the sleep-timer clock with its own interval, so the parent stops owning a timer.
3. Extract fullscreen chrome, preserving the `undefined`-vs-`false` drag-region behaviour verbatim and carrying the explanatory comment with it.
4. Run `npm run check` after each extraction.
5. Manually verify: lyrics scroll sync, sleep-timer countdown, fullscreen enter/exit, window drag in both states.

### Acceptance Criteria

- [ ] No frontend component exceeds ~450 lines.
- [ ] `npm run check` still reports 0 errors.
- [ ] Drag-region behaviour is unchanged in both fullscreen states, verified manually.

### Estimated Effort

Medium (half day to one day) including manual verification.

### Dependencies

Ideally after P1-2 adds Vitest, so the extraction is guarded.

### Risks if Ignored

Growing to 1,500+ lines, at which point extraction becomes a rewrite.

---

## [P2-3] No Rust dependency vulnerability scanning

**Priority:** P2 | **Area:** Dependencies | **Difficulty:** Low | **Impact:** Medium | **Confidence:** Confirmed
**Evidence:** `cargo audit` is not installed (verified). No `deny.toml`. `cargo tree` reports 1,157 nodes from 32 direct dependencies. `npm audit --omit=dev` reports 0 vulnerabilities — the JS side is fine and tiny (2 runtime deps).

### Problem

The Rust tree is ~1,157 crates and includes `librespot` pinned to a **git revision** (`b5f4631f`), not a published version — so it receives no crates.io security advisories at all, and updating it means manually tracking upstream. It also includes TLS stacks (`native-tls`), a WebSocket implementation (`tokio-tungstenite 0.24`), and an HTTP client, all parsing untrusted network input. None of this is scanned.

`Cargo.lock` is committed (good) and `vergen` is deliberately pinned with a documented reason (also good) — so version discipline exists, it just has no security dimension.

### Current / Desired Behavior

Currently: unknown CVE exposure. Desired: advisories checked on every CI run and license compliance verified.

### Impact

A known vulnerability in a network-facing transitive dependency could sit unnoticed indefinitely. `tokio-tungstenite 0.24` handles the dealer websocket — untrusted remote input.

### Recommended Fix

`cargo-deny` (covers advisories, licenses, bans, and sources in one tool) in CI.

### Implementation Plan

1. `cargo install cargo-deny`; `cargo deny init`.
2. Configure: `advisories` deny with explicit, commented allowances for anything unfixable due to the librespot pin; `licenses` allow the standard permissive set; `sources` allow crates.io plus the librespot GitHub org.
3. Run locally; triage findings — a tree this size will produce some.
4. Add `cargo deny check` to CI (P1-1).
5. Set a reminder to re-evaluate the librespot pin quarterly; note in `CLAUDE.md` alongside the existing vergen note.

### Acceptance Criteria

- [ ] `deny.toml` committed; `cargo deny check` runs in CI.
- [ ] Existing findings are either fixed or explicitly allowed with a comment stating why.
- [ ] The librespot pin review cadence is documented.

### Estimated Effort

Small (under 2 hours) plus triage.

### Dependencies

P1-1.

### Risks if Ignored

Silent CVE exposure in TLS/WebSocket/HTTP code paths handling remote input.

---

## [P2-4] The Rust↔TypeScript contract is duplicated by hand in three places

**Priority:** P2 | **Area:** Architecture | **Difficulty:** Medium | **Impact:** Medium | **Confidence:** Confirmed
**Evidence:** `src/lib/types.ts` (656 lines) hand-mirrors the Rust serde structs. Event names duplicated at `state.rs:14-21` and `api.ts:44-49`. `CLAUDE.md` documents the `rename_all` serialize-vs-deserialize hazard that produced `missing field 'isActive'`.

### Problem

656 lines of hand-maintained TypeScript mirroring Rust structs, with nothing checking they agree. The documented `#[serde(rename_all = "camelCase")]` bidirectional trap — where a struct deserialised from Spotify's snake_case and serialised to the webview's camelCase needs `rename_all(serialize = ...)` — is exactly the failure this duplication invites, and it has already fired once, surfacing as an innocuous "No devices found."

`svelte-check` passing proves the TypeScript is internally consistent, not that it matches Rust.

### Current / Desired Behavior

Currently: hand-maintained, drift detected only at runtime by a wrong-looking UI. Desired: generated, or checked.

### Impact

Silent field mismatches that manifest as empty lists or missing data rather than errors — the hardest bug class to notice.

### Recommended Fix

`ts-rs` (`#[derive(TS)]` + `#[ts(export)]`) generates TypeScript from the Rust structs at test time. Adopt incrementally, starting with the types crossing the boundary most often: `PlaybackState`, `AuthState`, `TrackInfo`, `Device`, `AppErrorPayload`.

Do not attempt all 656 lines at once.

### Implementation Plan

1. `cargo add --dev ts-rs`. Derive `TS` on `PlaybackState`, `AuthState`, `TrackInfo`.
2. `cargo test` exports them to `src/lib/generated/`. Commit the output.
3. Re-export from `types.ts` and delete the hand-written versions.
4. Add a CI step that regenerates and fails if `git diff` is non-empty.
5. Extend to the remaining boundary types over time — Spotify-wire-only structs do not need it.
6. Fold the event-name parity check in with P1-7's registration test.

### Acceptance Criteria

- [ ] The core playback/auth types are generated, not hand-written.
- [ ] CI fails on drift between Rust structs and committed TypeScript.
- [ ] Event-name constants are checked for parity.

### Estimated Effort

Medium (half day to one day) for the initial slice.

### Dependencies

P1-1.

### Risks if Ignored

Recurrence of the `missing field 'isActive'` class of bug, which presents as a feature that quietly returns nothing.

---

## [P2-5] `withGlobalTauri: true` widens the webview's API surface for no benefit

**Priority:** P2 | **Area:** Security | **Difficulty:** Extra small | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** `tauri.conf.json:11`. `src/lib/api.ts:1` imports `invoke` from `@tauri-apps/api/core` — the module path, not the global.

### Problem

`withGlobalTauri` injects `window.__TAURI__` into every page context. The frontend does not use it; it imports the API properly. The setting is scaffold default left on.

Actual risk is low — CSP is `default-src 'self'`, there is no `@html`, and no remote content is loaded. But it is a gratuitous widening: any injected script would get the full IPC surface handed to it by name rather than having to find a bundled module.

### Current / Desired Behavior

Currently `true`, unused. Desired `false`.

### Impact

Low. This is defence-in-depth hygiene, not an exploitable finding.

### Recommended Fix

Set `"withGlobalTauri": false`.

### Implementation Plan

1. Flip the flag.
2. `grep -rn "__TAURI__" src/` to confirm zero uses.
3. Run the app; verify login, playback, and window controls (the window buttons use `getCurrentWindow()` via module import — verify explicitly).

### Acceptance Criteria

- [ ] `withGlobalTauri` is `false`.
- [ ] No `window.__TAURI__` references exist.
- [ ] Window controls, login, and playback verified working.

### Estimated Effort

Extra small (under 30 minutes).

### Dependencies

None.

### Risks if Ignored

Minor. A wider IPC surface than the app needs.

---

## [P2-6] No formatting or lint gate is documented or enforced

**Priority:** P2 | **Area:** Code quality / DevEx | **Difficulty:** Low | **Impact:** Medium | **Confidence:** Confirmed
**Evidence:** `CLAUDE.md`'s Commands section lists `check`, `build`, `cargo check`, `cargo test` — no `cargo fmt`, no `cargo clippy`, no Prettier/ESLint. No `.prettierrc`, no `rustfmt.toml`, no eslint config in the repo.

### Problem

Formatting and lint consistency currently depend on editor settings and habit. The code *looks* consistent, which suggests `rust-analyzer` format-on-save is doing the work — but that is a per-machine setting, not a project guarantee.

Clippy in particular would likely find real issues across 18k lines: this is exactly the kind of codebase (heavy `Option`/`Result` chaining, many `async` state machines) where clippy earns its keep.

### Current / Desired Behavior

Currently: nothing enforced. Desired: `cargo fmt --check` and `cargo clippy -- -D warnings` in the loop and in CI.

### Impact

Formatting churn in diffs. Idiomatic issues clippy would catch going unnoticed. Low urgency, compounding cost.

### Recommended Fix

Add both to CI (P1-1) and to `CLAUDE.md`'s documented verification loop. Prettier for the frontend is optional — the Svelte/TS is already consistent and adding a formatter now would produce a large reformatting diff for little gain.

### Implementation Plan

1. Run `cargo clippy --no-default-features` and triage. Expect a meaningful list.
2. Fix what is worth fixing; `#[allow]` the rest **with a one-line justification each** — matching this codebase's existing comment standard, not bare allows.
3. Run `cargo fmt`; commit any reformatting as one isolated commit.
4. Add both as CI steps with `-D warnings`.
5. Update `CLAUDE.md`'s Commands section.

### Acceptance Criteria

- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy -- -D warnings` passes with no blanket crate-level allows.
- [ ] Both are in CI and in `CLAUDE.md`.

### Estimated Effort

Small to Medium, depending on clippy's findings.

### Dependencies

P1-1.

### Risks if Ignored

Gradual style drift, and missed idiomatic bugs in a large async codebase.

---

# 10. P3 — Low Difficulty / Quick Wins

---

## [P3-1] The README prerequisites table documents one specific machine

**Priority:** P3 | **Area:** Documentation | **Difficulty:** Extra small | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** `README.md:15-21` — a "Status on this machine" column reading "✅ present (151.x)", "✅ present (24.x)", "✅ present (1.97.1)".

### Problem

A prerequisites table's job is to tell a *new* reader what they need. This one tells them what the author already has. It is unactionable for anyone else and goes stale on every toolchain update.

### Desired Behavior

Minimum required versions, not observed local versions.

### Recommended Fix / Implementation Plan

1. Replace the "Status on this machine" column with "Minimum version."
2. Fill in: WebView2 (any evergreen), Node 18+, Rust 1.82 (matching `Cargo.toml`'s `rust-version`), MSVC build tools.
3. Add the `rustc --version` / `node --version` commands so a reader can check themselves.

### Acceptance Criteria

- [ ] The table states requirements, not observations.
- [ ] The Rust minimum matches `Cargo.toml`'s `rust-version = "1.82"`.

### Estimated Effort / Dependencies / Risks

Extra small. None. Minor: new-machine setup friction and a table that ages badly.

---

## [P3-2] `BACKUP/` is hidden by a local-only git exclude

**Priority:** P3 | **Area:** Repo hygiene | **Difficulty:** Extra small | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** `git check-ignore -v BACKUP` → `.git/info/exclude:18`. Contains `MANIFEST.txt`, a 27,638-byte `modified.patch`, and `original/`+`worktree/` directories.

### Problem

`.git/info/exclude` is machine-local and not committed. On any fresh clone `BACKUP/` would show as untracked, and the exclusion rationale exists nowhere in the repo. Meanwhile `.gitignore` carefully documents *why* each of its entries is ignored (`Rustify-main/`, `.research/`, `.agent-work/`) — so the convention exists and this one entry sidesteps it.

The directory itself is a stale merge-reconciliation artifact from 2026-08-21.

### Desired Behavior

Either the directory is gone, or its exclusion is in the committed `.gitignore` with a reason.

### Recommended Fix / Implementation Plan

1. Confirm the reconciliation it supported is complete (`modified.patch` dates to the merge, and `main` has 81 commits since).
2. Delete `BACKUP/`.
3. Remove the `.git/info/exclude` line.
4. If it is still needed, move it to `.gitignore` with a comment matching the file's existing style.

### Acceptance Criteria

- [ ] `BACKUP/` is deleted, or ignored via committed `.gitignore` with a stated reason.
- [ ] No project paths remain in `.git/info/exclude`.

### Estimated Effort / Dependencies / Risks

Extra small. Confirm the merge is done first. Risk of deletion: the patch is recoverable from git history if the merge landed; check before deleting.

---

## [P3-3] The footprint table is stale by the README's own admission

**Priority:** P3 | **Area:** Documentation | **Difficulty:** Small | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** `README.md:194-215`. The table is prefaced with "**predates the Jam/DJ/Home/lyrics/friends/profile/telemetry/audio-capability work below**, all of which add dependencies and background tasks" and "Not apples-to-apples" — Rustify measured on the login screen against a fully-loaded official client.

### Problem

The self-correction is admirably honest, and the "157 MB vs 1442 MB" headline still sits there in a table. Numbers get quoted; caveats do not. The comparison is not valid and the README says so.

### Desired Behavior

Either re-measure both clients logged in and playing, or delete the table and keep the one durable insight — the Rust process is ~5.9 MB private and WebView2 is the floor.

### Recommended Fix / Implementation Plan

1. Prefer deletion: keep the "the native side is effectively free; the webview is the floor" paragraph, which is the actually useful finding and does not go stale.
2. If keeping it: build release, log in on both clients, play a track on each, let them settle, measure private commit and working set, and note the date and build.

### Acceptance Criteria

- [ ] No headline number is present that the surrounding text disclaims.
- [ ] Any retained measurement states date, build, and that both clients were in the same state.

### Estimated Effort / Dependencies / Risks

Small. Requires a release build if re-measuring. Risk: a misleading number gets quoted somewhere it cannot be corrected.

---

## [P3-4] `JamSession::events()` panics if called twice

**Priority:** P3 | **Area:** Reliability | **Difficulty:** Extra small | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** `src-tauri/src/jams/session.rs:356-362` — `.expect("jam events mutex poisoned")` then `.expect("events() may only be called once; pass the returned receiver around")`.

### Problem

Two of the four non-test panic sites in the whole codebase are in one function. The `expect` messages are good — they explain the invariant — but a `std::Mutex` poisoning or a second call aborts the process. In a Tauri app that means the window vanishes with no message.

The invariant is documented and currently respected by the one caller, so this is latent, not live. It is called out because a "call this exactly once" contract enforced by panic is the weakest form of enforcement available, in a module (`jams`) that is experimental and most likely to be re-wired.

### Desired Behavior

Return `Option<mpsc::Receiver<JamEvent>>`; let the caller decide.

### Recommended Fix / Implementation Plan

1. Change the signature to `pub fn events(&self) -> Option<mpsc::Receiver<JamEvent>>`.
2. Replace the mutex `expect` with `.lock().ok()?` — a poisoned mutex becomes `None`, not a crash.
3. Update the single caller in `jams_bridge.rs` to map `None` to an `AppError`.
4. Keep the explanatory comment; it is the valuable part.

### Acceptance Criteria

- [ ] `events()` returns `Option` and cannot panic.
- [ ] The caller surfaces a typed error.
- [ ] Non-test panic sites drop from 4 to 2.

### Estimated Effort / Dependencies / Risks

Extra small. Best done with P1-3/P1-4. Risk if ignored: a hard process abort from a code path in the most-likely-to-be-rewired module.

---

## [P3-5] Version has been `0.1.0` for 81 commits

**Priority:** P3 | **Area:** Release hygiene | **Difficulty:** Extra small | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** `0.1.0` in `package.json:4`, `Cargo.toml:3`, `tauri.conf.json:4`, across the full history. `webapi.rs:29` embeds it in the User-Agent — so every request identifies as `rustify/0.1.0`.

### Problem

The version is meaningless, three files must be kept in sync by hand, and it is the only self-identification the app sends to Spotify.

### Desired Behavior

A version that changes, with a documented policy.

### Recommended Fix / Implementation Plan

1. Bump to something honest (`0.9.0` reflects the feature surface better than `0.1.0`).
2. Add a `CLAUDE.md` note on when it gets bumped and that three files must move together.
3. Optionally have `tauri.conf.json` read `"version"` from `package.json` (Tauri supports pointing at it), reducing three files to two.

### Acceptance Criteria

- [ ] Version is not `0.1.0`.
- [ ] All three manifests agree.
- [ ] The bump policy is documented.

### Estimated Effort / Dependencies / Risks

Extra small. Do with P0-2. Risk: inability to tell builds apart in logs or bug reports.

---

# 11. P4 — Optional / Future Improvements

---

## [P4-1] WebView2 memory floor

**Priority:** P4 | **Area:** Performance | **Difficulty:** Very High | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** `README.md:210-215` — Rust process 5.9 MB private, WebView2 ~151 MB of a 157 MB total.

**Problem / Recommendation.** The README's own analysis is correct: "further memory work means shrinking or replacing the webview, not optimising Rust." Replacing the webview means a native UI rewrite — months of work to reclaim memory the OS shares across WebView2 processes anyway.

**Recommendation: do not do this.** It is listed only so it is explicitly rejected rather than sitting as ambient temptation. The 157 MB figure is already ~9× better than the official client.

**Effort:** Extra large. **Risks if ignored:** none.

---

## [P4-2] DJ narration audio pipeline

**Priority:** P4 | **Area:** Feature | **Difficulty:** High | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** `README.md:165-170` — a valid signed TTS URL resolves; nothing plays it. Needs a second audio pipeline alongside librespot's Sink.

**Problem / Recommendation.** A second pipeline means mixing, ducking, and sequencing against a Sink this app does not own, requiring live device testing to get right — and it sits on a private endpoint that may vanish (Lexicon already 403s for most accounts per `CLAUDE.md`).

**Recommendation: defer indefinitely.** Under P1-6 this is Tier 3. Costs days of live-testing work for a feature most accounts cannot fully reach.

**Effort:** Large. **Dependencies:** P1-6 tiering, P0-1. **Risks if ignored:** none; DJ music playback already works.

---

## [P4-3] Cross-platform support

**Priority:** P4 | **Area:** Portability | **Difficulty:** Very High | **Impact:** Low | **Confidence:** Confirmed
**Evidence:** Windows-only throughout — Acrylic `windowEffects`, NSIS-only bundle target, `os_version: "10"` in the client-token identity, DPAPI proposed in P1-5, `CLAUDE.md`'s undecorated-window notes are all Win32-specific.

**Problem / Recommendation.** Portability would touch the window chrome, the bundle config, the audio device layer, and credential storage. **Recommendation: no.** Windows-only is a legitimate, coherent choice, and P1-5's DPAPI work deliberately deepens it — correctly.

**Effort:** Extra large. **Risks if ignored:** none.

---

## [P4-4] Client-side router

**Priority:** P4 | **Area:** Frontend architecture | **Difficulty:** Medium | **Impact:** Low | **Confidence:** Likely
**Evidence:** No router dependency in `package.json`; 13 views under `src/lib/views/` with navigation presumably state-driven from `App.svelte` (511 lines). [ASSUMPTION] — I read `App.svelte`'s size and imports but did not trace its navigation logic in full.

**Problem / Recommendation.** At 13 views, hand-rolled state-driven navigation is near the point where deep-linking, back-button behaviour, and per-view state retention get awkward. But a desktop app with no URL bar has weak motivation for a router, and adding one is a broad refactor with no user-visible benefit.

**Recommendation: revisit only if navigation logic in `App.svelte` becomes a source of bugs.** Not now.

**Effort:** Medium. **Dependencies:** P1-2 (Vitest) first. **Risks if ignored:** low.

---

# 12. Recommended Execution Plan

## Immediate Actions

**Goal:** Stop the bleeding on account risk and legal exposure; make the existing quality gates automatic.

**Tasks**

1. **Decide P0-2's question: personal project, or distributed?** Everything below branches on this. Recommendation: personal. Write it in the README.
2. Add `LICENSE`; fix the bundle identifier; bump the version (P0-2, P3-5).
3. Remove the impersonation headers in `pathfinder.rs`; re-test live; record what broke; reconcile docs and code on the policy (P0-1).
4. Add the CI workflow with all five gates (P1-1).
5. Gate `tauri-plugin-mcp-bridge` behind a Cargo feature (P2-1).
6. Set `withGlobalTauri: false` (P2-5).

**Reason.** P0-1 risks the user's Spotify account today. P0-2 is legally unsound today. P1-1 is two hours of work that protects everything after it. The rest are cheap and adjacent.

**Expected outcome.** No request claims a false identity; the repo is legally usable; every push is verified.

**Definition of done.** `LICENSE` present; no `Spotify/1.` or `xpui` strings in source; CI green on `main`; `cargo tree` shows no mcp-bridge in a default build; docs match code on the impersonation question.

---

## Short-Term Plan (1–2 weeks)

**Goal:** Fix the concurrency defects and establish real verification.

**Tasks**

1. Fix `ensure_jams` racing and `JamController` task detachment; correct the two misleading comments (P1-3, P1-4).
2. Convert `JamSession::events()` to return `Option` (P3-4).
3. Add the highest-value Rust tests: token merge, `is_grant_rejected`, position anchoring (P1-2 steps 1–3).
4. Add Vitest and 4–6 `store.svelte.ts` tests (P1-2 steps 4–5).
5. Add `cargo-deny` and triage (P2-3).
6. Run clippy, triage, enable `-D warnings` (P2-6).
7. README fixes: prerequisites table, footprint table, `BACKUP/` (P3-1, P3-2, P3-3).

**Reason.** P1-3/P1-4 are confirmed defects in the same subsystem, cheap together. The tests target the three documented bugs most likely to recur. The doc fixes are minutes each and protect the credibility of documentation that is otherwise this project's best asset.

**Expected outcome.** Jam lifecycle correct and documented accurately; the recovery machinery guarded by tests; dependency and lint gates live.

**Definition of done.** Concurrent `ensure_jams` builds one controller; no `jams:changed` after logout (manually verified); `npm test` and `cargo deny check` in CI; clippy clean at `-D warnings`.

---

## Medium-Term Plan (2–6 weeks)

**Goal:** Reduce structural and scope debt.

**Tasks**

1. Split `commands.rs` into domain modules; move `establish` out of the dispatch layer (P1-7 steps 1–2).
2. Add the command/event registration parity test (P1-7 steps 3–5).
3. Encrypt `tokens.json` with DPAPI, with silent plaintext migration (P1-5).
4. Adopt `ts-rs` for the core boundary types (P2-4).
5. Extract components from `NowPlaying.svelte` (P2-2).
6. Write the feature tier list; label Tier 3 in the UI; delete at least one unused Tier 3 feature (P1-6).

**Reason.** These are all "the code is fine but the structure will bite later" items. They need the CI and tests from earlier phases to be done safely — which is exactly why they are not first.

**Expected outcome.** No file over ~600 lines; boundary drift caught in CI; credentials encrypted at rest; scope explicitly bounded.

**Definition of done.** Registration parity test fails on an intentionally-omitted `api.ts` wrapper; `tokens.json` unreadable; existing installs migrate without re-login; tier list in `CLAUDE.md` with every feature assigned.

---

## Long-Term Plan (after stability)

**Goal:** Keep the project maintainable and honest.

**Tasks**

1. Quarterly Tier 3 review — delete, do not deprecate.
2. Quarterly librespot pin review against upstream (alongside the existing vergen note).
3. Extend `ts-rs` coverage to remaining boundary types.
4. Keep `obsidian/` current — especially the auth, rate-limiting, and playback notes `CLAUDE.md` flags as staling fastest.
5. Explicitly reject P4-1 (webview replacement) and P4-3 (cross-platform) in writing so they stop consuming attention.

**Reason.** This project's real long-term risk is not decay in the code — it is scope accretion against a hostile upstream, plus documentation drifting out of sync with reality (which P0-1 and P1-4 both demonstrate has already happened twice).

**Expected outcome.** A stable, bounded, well-documented personal client whose docs can be trusted.

**Definition of done.** Every quarter produces either a deletion or a written decision not to delete. No documented claim contradicts the code.

---

# 13. Quick Wins

| Win | Effort | Benefit | Where |
|---|---|---|---|
| Add `LICENSE` | 10 min | Makes an already-published repo legally usable | Repo root |
| `withGlobalTauri: false` | 20 min | Removes an unused IPC surface | `tauri.conf.json:11` |
| Fix the bundle identifier | 20 min | Cost grows with every user; currently one | `tauri.conf.json:5` |
| CI workflow | 2 hrs | Automates three gates that already exist and work | `.github/workflows/ci.yml` |
| Abort the jam forward task | 30 min | Fixes a confirmed detached-task leak | `jams_bridge.rs:240,362` |
| `ensure_jams` build lock | 1 hr | Fixes a confirmed race with side effects | `commands.rs:1972` |
| `events()` → `Option` | 20 min | Removes 2 of 4 non-test panic sites | `jams/session.rs:356` |
| Prerequisites table | 15 min | Makes the README useful to a second person | `README.md:15` |
| Delete `BACKUP/` | 10 min | Removes a stale artifact and a local-only exclude | Repo root |
| `cargo-deny` | 1 hr + triage | CVE and license coverage on 1,157 crates | New `deny.toml` |
| Bump version off `0.1.0` | 10 min | Builds become distinguishable in logs and UA | 3 manifests |

Roughly one focused day for the whole table, and it closes one P0, three P1s, and four P3s.

---

# 14. What Not to Change Yet

**Do not rewrite the auth module.** `auth.rs` is 1,146 lines and looks like it wants simplifying. It does not. Nearly every branch encodes a specific production failure — the merge-preserve in `save_stored_tokens`, the degrade-don't-propagate in `restore_login`, the narrow `is_grant_rejected`, the empty-refresh-token guard. A cleanup would delete the ugliness *and* the reasons for it. Add tests (P1-2) first; only then consider touching it, and only with those tests green.

**Do not replace the webview.** (P4-1.) The README's own analysis already settles it. Months of work for memory the OS is sharing anyway.

**Do not add features until CI, license, and the P1-3/P1-4 fixes land.** The gap between engineering quality (~8/10) and infrastructure (~3/10) is this project's defining problem. More features widen it.

**Do not narrow `STREAMING_SCOPES`.** It looks over-broad. `auth.rs:29-36` explains why it is not: that token is the fallback, and narrowing it made the fallback silently under-privileged — search kept working while library and player calls returned bare 403s. Leave it.

**Do not "fix" the DJ fallback to look like a resolved session.** `CLAUDE.md` is explicit: `spawn_dj_refill` keys off those flags being false. It looks like an inconsistency; it is load-bearing.

**Do not add Prettier/ESLint right now.** The frontend is already consistent under strict `svelte-check`. Adding a formatter produces a large reformatting diff that obscures the substantive changes in the plan above. Revisit after the medium-term phase.

**Do not touch the window-chrome CSS.** `CLAUDE.md`'s Acrylic section documents a hard-won configuration where transparency, `windowEffects`, and body background must move together, and where `windowEffects.effects` is a priority list rather than a stack. This is not a place to experiment casually.

---

# 15. Testing Plan

## Critical flows to cover first

- [ ] Token persistence merge — a session with no Web API refresh token must not erase the stored one
- [ ] Grant rejection — only `invalid_grant`/`invalid_client` may clear credentials
- [ ] Session generation — a stale pump generation must not trip the watchdog
- [ ] Position anchoring — a volume/shuffle event must not reset the UI clock
- [ ] Auth state machine in `store.svelte.ts` — `SessionExpired`/`NotLoggedIn` clear auth; intentional logout shows no "expired" banner

## Unit tests to add (Rust)

- [ ] `save_stored_tokens`: preserve on `None`, overwrite on `Some`, `clear_stored_tokens` removes the file
- [ ] `is_grant_rejected`: table over real error strings plus near-misses that must return `false`
- [ ] `PlaybackState::set_position`/`refresh_position`: elapsed accumulation, `duration_ms` clamping, no advance while paused
- [ ] `next_session_generation`: monotonic; concurrent callers get distinct values
- [ ] `ensure_jams`: two concurrent calls build exactly one controller (P1-3)
- [ ] `webapi::send` status mapping: 400→`BadRequest`, 401→`SessionExpired`, 403→`Forbidden`, 404→`Unavailable`, 5xx→`ServiceUnavailable`, 429→`RateLimited` with `Retry-After` parsed
- [ ] `WebApi::get` retry: absorbs `Retry-After` ≤ 8 s, returns beyond it, stops at `MAX_RETRIES`
- [ ] Empty-200 handling: an empty body deserialises as `null` rather than erroring

## Unit tests to add (frontend, new)

- [ ] `handleError` clears auth on `SessionExpired` and on `NotLoggedIn`
- [ ] Ticker starts on `isPlaying: true`, stops on `false`, clamps at `durationMs`
- [ ] `#syncLyrics` — a stale response for a previous track is discarded
- [ ] `toggleFriendsPanel` rolls back on rejection
- [ ] `destroy()` clears the interval and all listeners

## Integration tests

- [ ] Command/handler/`api.ts` registration parity (P1-7)
- [ ] Event-name constant parity between `state::events` and `api.ts`
- [ ] `ts-rs` generated types match committed output (P2-4)

## End-to-end

Not recommended. Driving a Tauri webview in CI is high-maintenance for this project's size. The `README.md` 15-step manual checklist is the right tool — keep it, and label it explicitly as the E2E layer rather than as a stopgap.

## Manual test cases (add to the existing checklist)

- [ ] Log out during active jam → no `jams:changed` in the log afterward (P1-4)
- [ ] Open the Jams view and click Create simultaneously → exactly one dealer subscription (P1-3)
- [ ] Upgrade an install with plaintext `tokens.json` → migrates silently, no re-login (P1-5)
- [ ] Disconnect the network mid-session → session survives; reconnect recovers without forcing login
- [ ] Kill librespot's player → the watchdog rebuilds on the 2/6/15/45 s backoff
- [ ] Log in and out three times → exactly one refresher task; no token-endpoint duplication in the log

## Edge cases

- [ ] Corrupted `tokens.json` → treated as absent, no panic
- [ ] Corrupted `settings.json` → defaults applied, no panic
- [ ] Port 8898 held by another process → falls back to an ephemeral port
- [ ] Port 8899 held → Web API login must fail with a clear message (the port is fixed by design)
- [ ] Refresh token expired after six months → clean logout, not a retry loop
- [ ] Clock moved backwards mid-track → telemetry adds zero, never negative

## Failure scenarios

- [ ] Spotify returns 429 on `/me` during `establish` → tokens already persisted; retry via `restore_session` without reopening the browser
- [ ] Web API refresh fails while streaming refresh succeeded → degrade to shared token; rotated streaming token persists
- [ ] Dealer websocket drops → Connect state falls back to the 5 s poller
- [ ] Pathfinder returns `PersistedQueryNotFound` → hash re-scrape or clean degradation, never a silent empty view

---

# 16. Security Checklist

Project-specific. Generic items are excluded.

**Authentication**
- [x] OAuth PKCE with no client secret — correct for a desktop app
- [x] Password login correctly ruled out (server-side disabled July 2024)
- [x] Redirect URIs restricted to `127.0.0.1` loopback
- [x] Device-authorization flow validates `user_code` charset and pairing URL host/scheme, with tests
- [x] Device code never crosses the IPC boundary to the webview (test-asserted)
- [ ] **Streaming scope union is broader than any single flow needs** — justified and documented as the fallback token; accepted, not a defect

**Authorization**
- [x] Premium gate before playback, degrading correctly when Spotify omits `product`
- [x] Tauri capabilities are narrowly scoped — no `fs`, `shell`, or `http` permissions granted
- [ ] **`mcp-bridge:default` granted unconditionally** (P2-1)

**Secret management**
- [ ] **Refresh tokens stored in plaintext** (P1-5)
- [x] Client ID correctly treated as non-secret; `.env` gitignored for tidiness with the reasoning documented
- [x] Client token never persisted — minted per session
- [x] `.env` not tracked in git (verified)

**Logging**
- [x] `safe_url()` redacts jam join secrets, with a test
- [x] Device-token poll failures log the error, never the token
- [x] `establish` logs token *presence* booleans, never values
- [x] `safe_excerpt` caps Pathfinder error bodies at 300 chars
- [ ] Full URLs including query strings are logged on Web API failures (`webapi.rs:84`) — low risk since Spotify query params are IDs, not credentials; worth a second look if that changes

**Input validation**
- [x] Every Spotify response field that becomes a URL, code, or hash is validated
- [x] Playlist IDs bounds-checked; search limit capped at Spotify's real maximum
- [x] Image upload re-encoded and capped at 256 KB base64 in the webview

**Client identity**
- [ ] **Impersonates the official desktop client on Pathfinder** (P0-1) — the single most serious item on this list

**Webview**
- [x] CSP restricts `default-src` to `'self'`; images pinned to Spotify CDNs
- [x] No `@html`, `innerHTML`, `eval`, or `new Function` anywhere
- [ ] `withGlobalTauri: true` unnecessarily (P2-5)

**Dependencies**
- [x] `npm audit --omit=dev`: 0 vulnerabilities (2 runtime deps)
- [ ] **Rust tree unscanned** — 1,157 crates, `cargo audit` not installed (P2-3)
- [ ] librespot pinned to a git rev, so no crates.io advisories apply

**Distribution**
- [ ] **Installer unsigned** (P0-2)
- [ ] **No update channel for security fixes** (P0-2)

*Not applicable:* CORS, CSRF, IDOR, SSRF, rate limiting as a server concern, file-upload handling as a server concern. This is a single-user desktop client with no server component and no multi-tenant data.

---

# 17. Performance Checklist

**Network**
- [x] One pooled `reqwest::Client` on `AppState` — the per-request-client regression is fixed and documented
- [x] Separate pooled client for Last.fm (different host, carries no Spotify credential)
- [x] Rate-limit absorption on idempotent GETs only, capped at 8 s
- [x] `/me/following` cursor pagination handled correctly (the one endpoint that differs)
- [x] Search limit capped at Spotify's real maximum of 10
- [x] Exactly one recurring network timer (`spawn_remote_poller`, 5 s), gated on not being the active device
- [ ] Verify no additional polling has crept in — `audio-devices.svelte.ts:38` polls every 5 s and `FriendsPanel.svelte:32` every 30 s, both local/UI-only, but `CLAUDE.md`'s "read `rate-limiting.md` before adding another timer" rule should be applied to them too

**Caching**
- [x] Artist overview: 10-minute TTL, 64-entry bound, successes only
- [x] Lyrics: 64-entry bound with per-URI request gates
- [x] Saved tracks snapshot with TTL, invalidated on any save/unsave
- [x] Audio capability cache stores booleans/counts only, never storage URLs
- [x] Every cache is explicitly bounded — no unbounded `HashMap` growth found

**Request coalescing**
- [x] Per-key `Arc<Mutex<()>>` gates prevent duplicate in-flight requests for lyrics and artist overviews

**Rendering**
- [x] 1 Hz `setInterval` for the progress bar instead of `requestAnimationFrame` — correct for a Pentium-class target
- [x] `PlaybackState` deliberately flat and cheap to clone
- [x] Position recomputed from an anchor rather than re-emitted

**Bundle / build**
- [x] `target: "chrome110"` — no unnecessary transpilation for evergreen WebView2
- [x] `sourcemap: false`, esbuild minification
- [x] Release profile: `opt-level = "s"`, `lto = true`, `codegen-units = 1`, `strip = true`
- [x] librespot `with-libmdns` disabled with a documented reason

**Audio**
- [x] `AudioRuntime` kept synchronous and off the Tokio state so the sink never blocks on an async lock
- [ ] EQ filter cost per sample not profiled [UNVERIFIED] — six biquad bands per channel; likely negligible, unmeasured

**Not applicable:** database queries, N+1, server-side pagination, image optimisation (all images are remote CDN URLs), background job queues.

---

# 18. Documentation Recommendations

The documentation is this project's strongest asset. These are targeted repairs, not a rewrite.

**Must add**
- `LICENSE` (P0-2) — the single largest documentation gap.
- A README banner stating the distribution posture, the Premium requirement, the Windows-only scope, and that the app uses private Spotify APIs at the user's own risk.

**Must fix (accuracy — these are the ones that matter)**
- Reconcile the impersonation policy across `telemetry.rs:1-8`, `README.md:176-179`, and `CLAUDE.md` with what `pathfinder.rs:199` actually does (P0-1). A false claim in otherwise-excellent documentation devalues all of it.
- Correct `jams_bridge.rs:8-9` and `commands.rs:894-895`, which state the forward task is aborted when nothing aborts it (P1-4).
- Prerequisites table: requirements, not this machine's versions (P3-1).
- Footprint table: re-measure or delete (P3-3).

**Should add**
- The feature tier list in `CLAUDE.md` (P1-6).
- `cargo fmt`, `cargo clippy`, and `npm test` in `CLAUDE.md`'s Commands section (P1-1, P2-6).
- The version-bump policy and the three-file sync requirement (P3-5).
- A short troubleshooting section: 429s and what they mean, "command not found" → missing `generate_handler!`/`api.ts` edit, the `cargo clean` fix after moving the project directory, and `$env:RUST_LOG` usage. Several of these already exist scattered through `CLAUDE.md`; collecting them under one README heading serves a different reader.

**Explicitly not needed**
- A `CONTRIBUTING.md`, unless P0-2 resolves toward distribution. Under "personal project," it is ceremony.
- Architecture docs. `obsidian/` (111 notes, `concepts/` + `files/` + a `MOC.md` entry point) already exceeds what most teams produce. Keep it current; do not add to it.

**Keep exactly as is**
- The blockquote convention for reversed decisions. This is the best thing in the repo's process and should be defended against tidying.
- The 15-step manual checklist in `README.md`. Label it as the E2E test layer rather than treating it as a placeholder for automation that is not coming.

---

# 19. Open Questions

**1. Is this a personal tool or something you intend to distribute?**
*Why it matters:* It determines whether P0-2's signing/updater work, P2-4's type generation, and much of the testing investment are justified at all. Under "personal," roughly a third of this review is optional.
*How the answer changes things:* "Personal" → do the immediate + short-term phases and stop; skip signing, the updater, and E2E entirely. "Distributed" → P0-1 becomes non-negotiable before any release, and P0-2 grows into a multi-week workstream.

**2. Was the impersonation in `pathfinder.rs` a deliberate exception, or did it land without the policy in `telemetry.rs` being reconsidered?**
*Why it matters:* It changes the fix from "remove it" to "restate the policy honestly."
*How the answer changes things:* Deliberate → P0-1 becomes a documentation fix plus an explicit account-risk warning to the user, and drops to P1. Unnoticed → remove the headers as recommended.

**3. Which Tier 3 features do you actually use?**
*Why it matters:* P1-6's deletion step needs this input and I cannot supply it. `README_jams.md` is 15 KB documenting a feature requiring captured Pathfinder hashes most installs will never have.
*How the answer changes things:* Anything unused should be deleted, which removes both maintenance load and private-API surface — partially addressing P0-1 for free.

**4. Has the `BACKUP/` merge reconciliation completed?**
*Why it matters:* Determines whether P3-2 is "delete" or "move the ignore rule."
*How the answer changes things:* Trivially, but I will not recommend deleting a 27 KB patch without confirmation.

**5. How often do you actually hit private-API breakage in practice?**
*Why it matters:* I inferred a roughly quarterly cadence from `error.rs` variants and README notes. If it is monthly, P1-6 escalates to P0. If it is annual, Tier 3 is far more defensible than I have assumed.
*How the answer changes things:* Directly sets the priority of the scope-reduction work.

**6. Is there a reason `cargo audit`/`cargo-deny` was never added?**
*Why it matters:* If it was tried and produced unfixable noise from the librespot git pin, P2-3's plan needs an allowlist-first approach rather than a clean-slate one.
*How the answer changes things:* Changes the triage step from "fix findings" to "document accepted risk."

---

# 20. Final Verdict

**1. Is this project safe to continue in its current state?**
Yes, with one exception. Continue — but fix P0-1 first. The impersonation headers put the user's paid Spotify account at risk, and the project's own documentation says it does not do the thing it does. Everything else can proceed in parallel.

**2. Is it safe to deploy to production?**
No. There is no production and no path to one: no license, a placeholder identifier, an unsigned installer, no updater, no CI, and no verification that the app works beyond a human running through a checklist. As a personal tool on the author's machine it is fine. As something handed to another person it is not ready, and the licensing gap makes that a legal statement, not just an engineering one.

**3. What is the biggest risk?**
The private-API surface — legally, operationally, and in maintenance terms, all at once. P0-1 is its sharpest edge; P1-6 is its long-term shape.

**4. What is the highest-value improvement?**
CI (P1-1). Two hours of work, and it makes three already-working quality gates automatic and permanent. Nothing else in this review has that ratio.

**5. What should be done first?**
Answer Open Question 1 — personal or distributed. Then, in one day: `LICENSE`, fix the identifier, strip the impersonation headers, add the CI workflow.

**6. What should be done second?**
The two confirmed jam concurrency defects (P1-3, P1-4) with their misleading comments corrected, then the first three Rust regression tests (P1-2 steps 1–3). All small, all guarding things this codebase has already been bitten by once.

**7. What should be avoided for now?**
Any new feature. Any refactor of `auth.rs` before it has tests. The webview replacement (P4-1). Cross-platform (P4-3). Prettier/ESLint. And do not narrow `STREAMING_SCOPES` or "fix" the DJ fallback — both look wrong and are load-bearing.

---

The uncomfortable summary: you have written approximately 8/10 code inside a 3/10 project. The Rust is careful, the concurrency reasoning is sound, the documentation is better than most funded teams produce, and the failure-mode archaeology in `auth.rs` is the kind of thing that takes real discipline to maintain. None of that is in question.

What is missing is everything that is not fun — a license file, a CI workflow, a decision about scope, and tests for the parts that would silently log a user out. There is a 111-note documentation vault and no `.github/` directory. That imbalance is the actual finding, and it is a prioritisation problem, not a capability one.

**Next step: answer Open Question 1, then spend one focused day on the Quick Wins table in section 13.** It closes one P0, three P1s, and four P3s, and it converts the discipline already present in the code into discipline the project enforces on itself.