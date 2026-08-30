---
tags: [concept, verification, ui, theming, playback, auth, jams, dj]
---
# 2026-08 correctness and polish pass

Fifteen reported defects, grouped by the domain that actually owned each one.
The theming, layout and Connect-icon items were single-cause; the playback and
Spotify-integration items each needed a new mechanism.

## Theming — one variable, two symptoms

`--accent` in [[app.css]] was Spotify's brand green `#1db954`. It is the only
saturated cool hue in a palette that is otherwise entirely warm (`#f5f0e6` ink,
`rgba(255,241,224,…)` glass, tobacco/umber ambient blobs), so every accented
thing read as pasted in from another app. It is now a warm caramel `#d4a373`,
with `--accent-hover` as its lighter step. Nothing else changed: the token was
already the single source, so the fix reached the progress fill, EQ nodes,
eyebrow labels, the Connect dot and the focus ring at once.

The "green square around the search bar" was the same variable seen through a
second bug. `:focus-visible` used `outline: 2px solid var(--accent)`, and **an
outline is always a rectangle — it ignores `border-radius` entirely**, so on a
999px pill it drew a hard square box. `--focus-ring` is now a two-step
`box-shadow` (dark inner ring reproducing `outline-offset`, accent outer ring),
which does follow the radius. `.find` takes the ring on the *pill* via
`:focus-within` and suppresses it on the inner `<input>`, because a ring drawn
on the bare input landed inside the frosted container and was clipped on three
sides.

## The friends panel no longer moves the app

> Until 2026-08 [[FriendsPanel.svelte]] was a flex rail beside `<main>` above
> 1050px and an overlay drawer only below it. As a rail it claimed 280px the
> instant it opened, which reflowed every grid behind it — cards re-wrapped,
> rows changed count, and whatever the user was reading jumped sideways.

It is now `position: fixed` at every width. `<main>`'s geometry is constant, so
opening and closing the drawer costs nothing behind it. The old rail existed so
the panel would not cover content; that concern is answered instead by keeping
it translucent at the standard raised elevation and dismissible three ways
(Escape, the titlebar toggle, a click on the backdrop).

The `.grid`'s `auto-fit` comment in [[app.css]] previously cited the friends
panel as the reflow trigger. That is no longer true — the rationale now cites
window resize, which is the remaining case.

## Fullscreen: no drag region, and the artwork tints it

Two fixes in [[NowPlaying.svelte]]:

- A borderless Tauri window still honours `data-tauri-drag-region` while
  fullscreen, so the lyrics view could be dragged out of fullscreen by its
  header — the window moved, the content stayed sized for the display, and Esc
  was the only way back. The attribute is now bound to
  `{fullscreen ? undefined : true}`. **`undefined`, not `false`:** Svelte omits
  an attribute set to `undefined`, whereas `false` on a non-boolean attribute
  still renders `data-tauri-drag-region="false"`, which Tauri matches on
  presence and keeps honouring.
- The fullscreen background is now tinted from the track. Source order is
  Spotify's own `lyrics.colors.background` first (it is designed for contrast
  against the lyric text), falling back to a colour sampled from the cover art
  with an offscreen 12×12 canvas. The sampler weights pixels by saturation and
  drops near-black/near-white, so a mostly-grey sleeve with one coloured
  element still reads as that colour. It needs `crossOrigin="anonymous"`;
  `getImageData` is wrapped in a `try` because a CDN that answers without
  permissive CORS taints the canvas, and the correct response to that is a flat
  background, not an error.

Only fullscreen is tinted. In the windowed view the panel sits inside the app
chrome, where a per-track tint fights the ambient wash rather than extending it.

## Connect icon: inverted meaning

[[SpotifyConnectMenu.svelte]] accented the trigger when
`playback.isActiveDevice` was true — i.e. during ordinary *local* listening,
which reads as "you are casting". It now accents when playback is happening
somewhere else and there is something to point at:
`!isActiveDevice && track !== null`. The tooltip names the remote device in
that state.

## The playback watchdog

See [[playback-and-connect]] for the mechanism. Summary: when librespot's
`PlayerEvent` channel closed, `spawn_event_pump` logged and exited, leaving the
dead `Spirc` installed in `AppState::spotify`. Every subsequent transport
command failed with `Internal error { channel closed }` until the process was
restarted. The pump now hands off to `recover_closed_session`, which rebuilds
the session from the stored refresh tokens with backoff and **never signs the
user out** — the OAuth grant is still valid; what died is the audio session on
top of it.

## Jam links

`session_from_value` in [[jams/session.rs|session.rs]] accepted the first
non-empty string it found across a list of candidate keys, several of which are
speculative. Social-connect responses have been seen carrying Spotify's own
internal transport URIs (`hs://`, `hm://`) under generic names like `link`;
pasted anywhere they resolve to nothing. Parsed URLs are now filtered through
`is_shareable_web_url` (http/https only) before use, so anything else falls
through to the constructed `https://open.spotify.com/socialsession/<token>`
form — which is the shape independently confirmed to work.

## Spotify integration is managed, not just replaced

`LoginInfo` now carries the configured `client_id` itself plus
`client_id_from_env`. A Client ID is not a secret — it travels in the clear in
every OAuth redirect and this flow is PKCE with no client secret — so handing
it to the webview is safe. [[Settings.svelte]] edits it in place and can clear
it via the new `clear_client_id` command; the env override disables the field
and says why, because `auth::webapi_client_id` gives the env var priority and
editing the saved value would otherwise silently do nothing.

Saving a *different* ID signs out as part of the same action: the stored
refresh token belongs to the previous app and cannot survive the change.

Sign-out moved from [[Profile.svelte]] to a danger zone at the bottom of
Settings. A profile is a thing you browse — including other people's — so a
destructive session action under one you were only reading was both easy to
miss and easy to hit by accident.

## Playlist editing

`update_playlist_details` (`PUT /playlists/{id}`) and `update_playlist_image`
(`PUT /playlists/{id}/images`) in [[library.rs]]. Two things make the image half
non-obvious:

- It needs **`ugc-image-upload`**, a grant separate from `playlist-modify-*`.
  It was added to both scope lists here, so tokens minted earlier lack it — the
  403 is rewritten to say so instead of surfacing bare "Forbidden".
- The body is **base64 JPEG with `Content-Type: image/jpeg`**, not JSON, hence
  `WebApi::put_raw`. `src/lib/images.ts` centre-crops, downscales to 640px and
  walks a quality ladder until the encoding fits Spotify's 256 KB *base64* cap,
  so the Rust side never needs an image codec.

`PlaylistSummary` gained `owner_id`, `description` and `collaborative`. The Edit
button is gated on `owner_id === auth.userId` — ids, not display names, which
are not unique. That gate is convenience only; Spotify's 403 is the real check.

## DJ falls back instead of failing

Lexicon (`/lexicon-session-provider/context-resolve/v2/session`) is the endpoint
Spotify gates hardest against non-official clients, and a 403/404 there used to
leave the DJ button permanently disabled behind an error the user could do
nothing about. `start_dj` now catches `LexiconUnavailable` and loads the public
DJ playlist as an ordinary context; `get_dj_status` reports
`dj::fallback_session()` rather than erroring. The session carries
`reason: "lexicon-unavailable-fallback"` — a constant mirrored in
[[Home.svelte]] — and every dynamic capability flag stays false, so nothing
downstream mistakes it for a resolved Lexicon session. The card explains that
the personalized mix and voice intros are restricted to Spotify's own clients.

## Listening DNA

New [[dna.rs]] and a radar chart in [[Profile.svelte]]. **This is not Spotify's
DNA and does not claim to be.** `/v1/audio-features` (danceability, energy,
valence) is closed to apps in Development Mode, which is what every
self-registered Client ID is — and Rustify's whole auth design is that each user
registers their own app. Scraping the values out of the web player is against
the Developer Terms and a reliable way to get IP-banned, so it is not attempted.

What remains available is enough for an honest profile: `genres` and
`popularity` on `/me/top/artists`, and album `release_date` on
`/me/top/tracks`. Six axes are derived from those, and each carries a `basis`
string naming exactly what was measured. The UI states the limitation in place.

The chart is one series across six axes, so it is deliberately single-hue —
`--accent` for the shape, hairline neutrals for the web, and text tokens (never
the series colour) for the labels. A radar cannot be measured accurately by
eye, so the numbers are always present as a labelled list beside it rather than
only on hover.

## Tour cards

[[ArtistView.svelte]]'s concert list rendered one long localised datetime
string, which reads as a paragraph at 12px. The date is now a stacked
month-over-day badge — the field people actually scan a tour list by — with the
venue promoted to the headline and a primary "Get tickets" action. The full
datetime survives on the third line and as the `<time>` element's `aria-label`.

## Verification

- `cargo check --no-default-features`: clean, no warnings.
- `cargo test --lib`: 104 passed, 0 failed (includes new coverage for
  `is_shareable_web_url` and the DNA helpers).
- `svelte-check` and the production Vite build: 153 files, 0 errors, 0 warnings.
- **Not verified at runtime.** None of the above exercises the Tauri app. The
  watchdog in particular can only be confirmed by killing a live librespot
  session and watching the log — see the manual checklist in `README.md`.
