---
tags: [file, frontend, state, audio]
---
# `src/lib/features/audio/audio-devices.svelte.ts`

Reference-counted Svelte singleton for `list_audio_devices` and
`get_audio_status`. It deduplicates in-flight refreshes and polls every five
seconds while at least one selector is mounted, covering hot-plug and fallback
status without duplicating state between Home/player and Settings.

## See also

[[AudioOutputSelector.svelte]] · [[store.svelte.ts]] · [[audio-mod]]
