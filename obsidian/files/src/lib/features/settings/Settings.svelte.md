---
tags: [file, frontend, ui, settings, audio]
---
# `src/lib/features/settings/Settings.svelte`

Settings covers quality, native output, equalizer, appearance, cache, and
Spotify integration. The redundant default/global volume control is
intentionally absent; player volume remains in [[PlayerBar.svelte]] and
persists as private session state.

Equalizer input calls `configure_audio` immediately and debounces
`update_audio_settings` for persistence. Presets, custom curves, bypass,
preamp, automatic headroom, graph points, and the Rust DSP share one settings
shape. [[AudioOutputSelector.svelte]] is reused here and in the player.

General settings save separately from audio changes. Replacing the Spotify
integration signs out because a different client ID requires a new OAuth grant.
The merge in [[auth.rs]] preserves credentials and player-volume state.

## See also

[[audio-mod]] · [[SelectMenu.svelte]] · [[store.svelte.ts]] · [[auth.rs]] · [[MOC]]
