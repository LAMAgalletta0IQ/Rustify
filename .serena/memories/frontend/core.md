# Frontend core
- `src/App.svelte` owns boot/setup/login, a flex-column custom titlebar shell, four tabs (Home/Search/Library/Settings), Profile account area, Now Playing overlay, and persistent PlayerBar.
- Setup/Login are children of absolute `.preauth`: a fixed 46 px drag strip plus a flexing/scrolling usable viewport. Cards use `margin:auto`; never use `height:100%` below the separate titlebar.
- `src/lib/store.svelte.ts` is the runes singleton for auth/playback/settings/error/events. It checks live Rust auth before restore, distinguishes intentional logout, and centralizes all command/section failures through `handleError`; SessionExpired/NotLoggedIn always moves the UI to logged-out even when the error remains local to a section.
- `api.ts` is the sole invoke surface; `types.ts` mirrors serde camelCase.
- Home sources are independent: context-aware recent activity, /me/top/tracks listening mix, and releases from /me/top/artists. Copy must never claim Spotify editorial/recommendation authorship.
- Profile reuses the authenticated session; account click must not log out. Sign-out is explicit inside Profile.
- PlayerBar native ranges use zero margin, 4 px track, 11 px centered thumb, 16 px hit area, local drag drafts, focus-visible ring, and native keyboard/ARIA.
- Track/album saved actions are optimistic with per-item pending guards and rollback.
- NowPlaying requests LRCLIB lyrics per track with cancellation; synced active line follows playback position, plain/instrumental/unavailable/error are distinct states.