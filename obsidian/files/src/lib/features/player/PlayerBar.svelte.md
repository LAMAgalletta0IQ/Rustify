---
tags: [file, frontend, ui, playback]
---
# `src/lib/features/player/PlayerBar.svelte`

Persistent playback surface. Artwork has its own album/content navigation,
metadata opens Now Playing, the center owns transport and the single standard
timeline, and the right side exposes a labeled Lyrics action, Spotify Connect,
the native output selector, and player volume.

`cycleRepeat` maps off → context → track → off. Seek converts percentage to
milliseconds; volume passes 0–100 to Rust for librespot-scale conversion.
Both sliders keep native keyboard/assistive behavior and use local draft values
during drag before committing on change.

The range wrapper paints the fill with the browser's thumb-travel formula.
The input explicitly has zero margin and a centered 16 px hit area, its WebKit
track is transparent and 4 px tall, and the 11 px thumb uses the matching
negative half-difference margin. Hover, active, and focus-visible reveal the
thumb; focus adds a visible ring. Disabled seek is dimmed/non-interactive.
Live computed geometry at 1100×720 placed both volume input and rail centers at
y=663, correcting the previous 2 px user-agent-margin offset.

## See also

[[player.rs]] · [[NowPlaying.svelte]] · [[SpotifyConnectMenu.svelte]] ·
[[playback-and-connect]] · [[2026-08-capability-audit]] · [[MOC]]
