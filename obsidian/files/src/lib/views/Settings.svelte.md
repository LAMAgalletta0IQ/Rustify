---
tags: [file, frontend, ui, settings]
---
# `src/lib/views/Settings.svelte`

Functional Settings destination in the title-bar navigation. It edits the
typed persisted `AppSettings`: default local-session volume (0–100), reduced
ambient/lyrics motion, and librespot audio-cache cap (128–8192 MB). Volume and
cache apply on the next session; reduced motion applies immediately after save.

The Spotify integration section reads `LoginInfo`. "Replace integration" is
explicitly described as a sign-out because a different client ID requires a
new OAuth grant. Saving merges into [[auth.rs]]'s existing settings document so
the client ID is never overwritten.

## See also

[[App.svelte]] · [[store.svelte.ts]] · [[auth.rs]] · [[player.rs]] · [[MOC]]
