---
tags: [file, backend, audio, dsp]
aliases: [audio-mod]
---
# `src-tauri/src/audio/mod.rs`

Native audio domain: stream quality, equalizer settings/presets/validation,
CPAL output discovery, disconnected-output fallback/retry, and the librespot
`ProcessingSink`.

The sink refreshes requested output and EQ state on decoded packets. Six
peaking filters are coefficient-smoothed over roughly 35 ms; wet/dry bypass is
smoothed and becomes sample-exact once settled. Preamp and optional automatic
headroom precede a final finite sample clamp.

Unit tests cover exact bypass, real spectral gain at 1 kHz, rapid curve changes,
range validation, and installed-bitrate mapping.

## See also

[[Settings.svelte]] · [[AudioOutputSelector.svelte]] · [[player.rs]] ·
[[2026-08-ui-audio-stability-pass]]
