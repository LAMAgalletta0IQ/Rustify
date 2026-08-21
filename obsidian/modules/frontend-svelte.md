---
tags: [module, frontend]
---
# Module — Frontend (Svelte 5)

**Path:** `src/` · **Svelte 5 + TypeScript + Vite**

Chosen over React for runtime footprint: Svelte compiles away, so the shipped
bundle is ~64 KB (23 KB gzipped) with no framework runtime — meaningful on the
low-end hardware this project targets.

## Structure

```
src/
├── main.ts             mount entry
├── App.svelte          shell + router
├── app.css             global styles / design tokens
├── vite-env.d.ts       ambient types
└── lib/
    ├── api.ts          typed wrappers for all 43 commands
    ├── types.ts        mirrors the Rust serde shapes
    ├── store.svelte.ts runes store, event subscriptions
    ├── views/          full-screen views
    └── components/     reusable UI
```

## Files

### Shell
| File | Role |
| --- | --- |
| [[main.ts]] | Mounts [[App.svelte]] into `#app` |
| [[App.svelte]] | Boot gate → login vs. main UI; tab switching; error banner |
| [[app.css]] | CSS custom properties, resets, shared utility classes |
| [[vite-env.d.ts]] | Svelte + Vite ambient type references |

### Shared library
| File | Role |
| --- | --- |
| [[api.ts]] | One function per Tauri command. No component calls `invoke` directly |
| [[types.ts]] | TS interfaces mirroring Rust; `formatMs`, `volumeToPercent` |
| [[store.svelte.ts]] | `store.playback`, `store.auth`, `store.error`; event listeners; position ticker |

### Views — [[frontend-views]]
[[Login.svelte]] · [[Home.svelte]] · [[Search.svelte]] · [[NowPlaying.svelte]] ·
[[AlbumView.svelte]] · [[ArtistView.svelte]]

### Components — [[frontend-components]]
[[PlayerBar.svelte]] · [[SpotifyConnectMenu.svelte]] · [[TrackList.svelte]]

## Architecture

**One-way data flow for playback.** Components never mutate playback state
locally; they call a command and wait for the backend event. See [[data-flow]].

**Views own their own data.** Library and search results are fetched by the
view that displays them and discarded on unmount. Only playback and auth live
in the store.

**No router library.** Navigation is boolean/enum state in [[App.svelte]]
(`tab`, `nowPlayingOpen`) and inside [[Search.svelte]] (`openAlbum`,
`openArtist`). Adequate for six views; there is no URL or history.

## Svelte 5 runes used

| Rune | Use |
| --- | --- |
| `$state` | Reactive local and store state |
| `$derived` | Computed values (`store.playback` aliases, progress %) |
| `$props` | Component inputs, including callback props |
| `$effect` | Data loading on dependency change |
| `untrack` | Prevent an effect re-running on state it writes |

> ### The `$effect` gotcha
> Svelte 5 tracks only reads that happen **synchronously, before the first
> `await`**. An async loader reading the same state it later assigns therefore
> does not loop — but it is fragile. [[Home.svelte]] genuinely re-ran on every
> appended page until its effect was wrapped in `untrack`.
>
> The rule used here: **any `$effect` calling an async loader wraps the call in
> `untrack`**, with the tracked dependency named explicitly on its own line.
> Applied in [[Home.svelte]] and [[NowPlaying.svelte]].

## Styling

Plain scoped CSS in each component — no Tailwind, no UI library, consistent
with the low-footprint goal. Shared tokens (`--bg`, `--accent`, `--fg-dim`,
`--radius`) are defined in [[app.css]]; the palette is dark-only.

## Error handling

`store.run(fn)` wraps a backend call, clearing `error` first and setting it on
failure. [[App.svelte]] renders `store.error` as a dismissible banner. Views
that need custom presentation — notably [[Login.svelte]] for
`PremiumRequired` — catch directly and branch on `kind`.

## See also

[[architecture]] · [[data-flow]] · [[state-and-events]] · [[backend-rust]] ·
[[frontend-views]] · [[frontend-components]] · [[project-root]] · [[MOC]]
