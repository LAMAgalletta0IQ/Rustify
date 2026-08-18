---
tags: [file, frontend, ui, auth]
---
# `src/lib/views/Setup.svelte`

First-run form for a user-owned Spotify Web API Client ID. It shows the exact
redirect URI from `LoginInfo`, validates non-empty input, persists through
`set_client_id`, and calls `onDone` only after success. Client IDs are public
identifiers; no client secret is requested or stored.

[[App.svelte]] owns a full-window `.preauth` flex column. The 46 px drag strip
is fixed and Setup's `.wrap` receives exactly the remaining viewport. The card
uses `margin:auto`, centering in both axes when it fits; at 780×520 the 474 px
usable wrapper scrolls and preserves 24 px safe padding so the form is fully
reachable. At 1100×720 the card and usable viewport centers both measure 383 px.

Login's explicit "Use a different Client ID" can reopen this view. From the
logged-in Settings view, replacing the integration first signs out because a
new client ID requires a new OAuth grant.

## See also

[[Login.svelte]] · [[Settings.svelte]] · [[auth-and-tokens]] · [[App.svelte]] · [[MOC]]
