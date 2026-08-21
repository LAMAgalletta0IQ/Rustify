# UI/audio stability pass — 2026-08-21

## Protection boundary
The HEAD baseline was b24183c (experimental Spotify Jams). Treat every file added/touched by that commit as protected, especially src-tauri/src/jams/**, jams_bridge.rs, examples/jam_demo.rs, README_jams.md, and the Jam regions of commands.rs/lib.rs/player.rs/state.rs/App.svelte/api.ts/types.ts/AlbumView.svelte. This pass intentionally left Jam symbols and modules in place and used only targeted non-Jam hunks in mixed files.

## Current ownership
- src/lib/features/audio: shared CPAL device singleton and AudioOutputSelector.
- src/lib/features/player: persistent PlayerBar and SpotifyConnectMenu.
- src/lib/features/lyrics: NowPlaying, native fullscreen, queue, and lyric interaction.
- src/lib/features/settings: Settings and live equalizer UI.
- src/lib/ui/SelectMenu.svelte: shared portalled/collision-bounded accessible listbox.
- src-tauri/src/audio/mod.rs: quality, device resolution/fallback/retry, six-band DSP and tests.
- src-tauri/src/lyrics/mod.rs: LRCLIB lookup/parser.
- Existing route/detail views stay in src/lib/views; TrackList remains a shared component.

## Behavioral contracts
- configure_audio validates/applies output + EQ without persistence. update_audio_settings persists only those fields and applies the same snapshot. Settings previews immediately and debounces persistence.
- Saved disconnected CPAL names are allowed; list_output_devices returns them with is_available=false. ProcessingSink falls back to default and retries requested output every 5s.
- Settings default/global volume was removed. auth::Settings now keeps private last_volume_percent with a serde alias for legacy default_volume_percent; set_volume persists it.
- TrackInfo carries album_id/album_uri so artwork can navigate to AlbumView. Standalone content falls back to NowPlaying.
- Explicit labeled Lyrics control opens native fullscreen. Standard mode uses PlayerBar timeline; fullscreen hides PlayerBar and owns exactly one page timeline.
- Lyrics are fetched centrally in AppStore with a request generation. NowPlaying uses local scrollTo, following/manual states, explicit resume, lyric seek, and rounded hover/focus styles.
- NowPlaying queue refresh must be guarded by track URI; playback snapshots replace the object and would otherwise consume API quota repeatedly.
- SpotifyConnectMenu is account/session transfer; AudioOutputSelector is the CPAL sink. Do not merge their semantics.

## Verification evidence
- svelte-check: 0 errors, 0 warnings.
- Vite production build succeeds.
- cargo test --all-targets --all-features: 14 passed.
- New DSP tests prove 1 kHz band changes sample RMS, settled bypass is exact, rapid changes remain finite/bounded.
- Live Tauri verified presets/custom presets, disconnected output, headset↔Realtek↔default switching without track/position/flags reset, album artwork navigation, one fullscreen timeline, repeated 820x620↔1920x1080 fullscreen, lyric seek, manual follow suspension/resume, and clean console.
- cargo fmt --check and clippy -D warnings retain only pre-existing protected Jam formatting/lints; ordinary cargo clippy succeeds with those warnings.
- /screenshots/ is ignored and no screenshots are tracked.

See Obsidian note [[2026-08-ui-audio-stability-pass]].