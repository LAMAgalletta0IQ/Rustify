---
tags: [module, frontend, ui]
---
# Module — Components

**Path:** `src/lib/components/` · Parent: [[frontend-svelte]]

Reusable UI shared across views.

## The components

| Component | Used by | Role |
| --- | --- | --- |
| [[PlayerBar.svelte]] | [[App.svelte]] | Persistent transport bar |
| [[DevicePicker.svelte]] | [[PlayerBar.svelte]] | Connect device popover |
| [[TrackList.svelte]] | [[Home.svelte]], [[Search.svelte]], [[AlbumView.svelte]], [[ArtistView.svelte]] | Track rows |

## `TrackList.svelte` — the most reused

Takes `tracks` plus an optional `contextUri`, and that optional prop encodes an
important rule:

| `contextUri` | Behaviour | Callers |
| --- | --- | --- |
| Provided | `loadContext(contextUri, track.uri)` — plays the container starting at the clicked track | Playlist, album, liked songs |
| `null` | `loadTracks(allUris, track.uri)` — ad-hoc list | Search results, release-derived artist track sample |

Getting this wrong means playback stops after one song. See
[[playback-and-connect]].

It also owns the ♥ save state, looked up in 50-id chunks (the Web API cap) and
updated optimistically with rollback on failure.

## Composition

```
App.svelte
└── PlayerBar.svelte
    └── DevicePicker.svelte

Home / Search / AlbumView / ArtistView
└── TrackList.svelte
```

`PlayerBar` is rendered once by the shell and never unmounts, so playback
controls survive navigation.

## Conventions

**Callback props, not events.** Svelte 5 style — `onOpenNowPlaying`, `onBack`,
`onOpenAlbum` are passed as functions via `$props()`.

**Read the store directly.** Components import `store` rather than receiving
playback state through props, so no prop-drilling through the shell.

**`store.run()` for actions.** Fire-and-forget commands route errors to the
banner in one line.

**Scoped CSS with shared tokens** from [[app.css]].

## See also

[[frontend-svelte]] · [[frontend-views]] · [[state-and-events]] ·
[[playback-and-connect]] · [[MOC]]
