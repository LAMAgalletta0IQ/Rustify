---
tags: [file, frontend, ui, account]
---
# `src/lib/views/Profile.svelte`

Account/profile destination opened by the signed-in account button. It reuses
the current authenticated session and shows the exposed profile identity plus
independent top-artist, top-track, and playlist summaries. Partial API failure
does not erase successful sections.

Sign out is an explicit action here, not the account button's default behavior.
It clears OAuth tokens and live playback/auth state while retaining app
settings and the configured client ID.

## See also

[[App.svelte]] · [[store.svelte.ts]] · [[auth-and-tokens]] · [[MOC]]
