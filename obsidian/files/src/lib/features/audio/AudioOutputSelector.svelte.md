---
tags: [file, frontend, ui, audio]
---
# `src/lib/features/audio/AudioOutputSelector.svelte`

Controlled native-output selector shared by Settings and PlayerBar. It maps the
singleton device state to [[SelectMenu.svelte]], distinguishes system default,
active, available, and disconnected outputs, and refreshes after a change.

## See also

[[audio-devices.svelte.ts]] · [[audio-mod]] · [[Settings.svelte]] · [[PlayerBar.svelte]]
