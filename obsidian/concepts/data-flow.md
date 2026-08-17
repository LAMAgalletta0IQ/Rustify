---
tags: [concept, dataflow]
---
# Data flow

Three traced paths. Together they cover almost everything the app does.

## Path A — Playing a track (command out, event back)

The important thing: **the command does not update the UI.** It returns
`void`. The UI changes only when librespot pushes an event back.

```
User clicks a row in TrackList.svelte
  │
  ▼ playTrack(t)  — picks context vs. ad-hoc list
api.ts  loadContext(contextUri, trackUri)
  │
  ▼ Tauri IPC
commands.rs  load_context
  │
  ▼ with_spirc(...)
librespot Spirc.load(LoadRequest::from_context_uri(..., start_playing: true))
  │
  ▼ audio decoded → rodio → WASAPI → speakers
  │
  └─► PlayerEvent::Loading / Playing  ─────┐
                                            ▼
                          player.rs  spawn_event_pump
                            · mutates AppState.playback
                            · resolves metadata via Web API (cached)
                            · app.emit("playback:changed", snapshot)
                                            │
                                            ▼ Tauri event
                          store.svelte.ts  store.playback = payload
                                            │
                                            ▼ Svelte 5 runes reactivity
                          PlayerBar / TrackList / NowPlaying re-render
```

**Why the round trip?** Because a phone can also start playback. If the UI
updated optimistically from its own commands, remote changes would desync it.
One inbound path keeps local and remote identical. See
[[playback-and-connect]].

**Context vs. tracks.** `load_context` plays a *container* (playlist, album,
artist, Liked Songs) starting at a chosen track, so playback continues past it.
`load_tracks` handles results with no container — search hits and queue
entries. Loading a bare track URI would play one song and stop.

## Path B — Loading the library (plain request/response)

No events involved; the view owns the data.

```
Home.svelte  $effect on `section`  (wrapped in untrack)
  │
  ▼ api.ts  getPlaylists(limit, offset)
commands.rs  get_playlists
  │  ├─ token(&state)  → TokenStore, or NotLoggedIn
  ▼
library.rs  playlists()
  │
  ▼ webapi.rs  get()  → GET /v1/me/playlists  + Bearer
Spotify Web API
  │
  ▼ Page<WirePlaylist>  →  Vec<PlaylistSummary>   (flattened for the UI)
  ▼ Tauri IPC (serde camelCase)
Home.svelte  playlists = [...]
```

The `Wire*` → `*Summary` conversion in [[library.rs]] is deliberate: Spotify's
envelopes are deeply nested and full of nulls. Flattening in Rust keeps the
Svelte components trivial and reduces what crosses the IPC boundary.

**Pagination** appends: `exhausted` is set when a page returns fewer items than
requested, which hides the "Load more" button.

## Path C — Token refresh (background, invisible)

```
auth.rs  spawn_refresher   (spawned once at login)
  │
  ▼ sleep until (expires_at − 5 min), floor 30 s
  ▼ refresh_login()  → librespot_oauth refresh_token_async
  ▼ tokens.set(new_access_token)      ← TokenStore, shared
  ▼ if refresh token rotated → save_stored_tokens()
  └─► loop
```

Everything reading the token — [[commands.rs]] and the metadata lookup inside
[[player.rs]]'s event pump — reads through `TokenStore`, so they all observe the
new value with no plumbing.

This is why `TokenStore` exists rather than a plain `String`: an earlier design
copied the token into the event pump at startup, where a refresh could never
reach it, and metadata lookups would begin failing after an hour.

On failure the refresher retries instead of ending the session, since the
likeliest cause is a transient network blip.

## State ownership

Who owns what, and therefore where to look when something is stale:

| Data | Owner | Lifetime |
| --- | --- | --- |
| Playback + auth state | `AppState` in Rust ([[state.rs]]) | Session; mirrored into the store |
| Web API bearer token | `TokenStore` ([[state.rs]]) | Refreshed continuously |
| Refresh token | `tokens.json` on disk ([[auth.rs]]) | Across restarts |
| Library / search results | The Svelte view that fetched them | Until unmounted |
| Track metadata cache | `HashMap` in the event pump ([[player.rs]]) | Session |
| Saved (♥) flags | `TrackList.svelte` local state | Component lifetime |
| Audio + credential cache | librespot `Cache` on disk, 2 GB cap | Across restarts |

Playback state is *not* owned by the frontend — the store is a replica. Library
data is the opposite: Rust never caches it.

## See also

[[architecture]] · [[state-and-events]] · [[auth-and-tokens]] ·
[[playback-and-connect]] · [[player.rs]] · [[commands.rs]] ·
[[store.svelte.ts]] · [[MOC]]
