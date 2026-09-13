//! Fetches, decodes and plays DJ narration clips.
//!
//! Spotify's `client-tts/v1/fulfill` endpoint (see `spotify/dj.rs`) resolves a
//! narration script to a short-lived, unauthenticated CDN URL for a small MP3
//! clip — a spoken intro or outro around a DJ track. This module turns that
//! URL into audible output.
//!
//! Unlike a crossfade, narration is never mixed with the track: the official
//! client reports `ms_narration_overlapping = 0`, meaning intro, track and
//! outro play back to back. Rustify's playback pipeline hands Spotify track
//! URIs to librespot's own `Player`/`Spirc`, which owns the only path to the
//! real output device — there is no seam for handing it an arbitrary
//! non-Spotify audio clip to interleave with a track. So narration clips play
//! through a second, independent, short-lived `cpal` stream on the same
//! output device instead, sequenced by pausing/loading around it (see
//! `player::spawn_dj_advance`) rather than by mixing PCM inside librespot's
//! Sink chain.

use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use librespot::playback::{NUM_CHANNELS, SAMPLE_RATE};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::Sample;

use crate::audio::AudioRuntime;
use crate::error::{AppError, AppResult};

/// Mirrors go-librespot's own cap: a narration clip is a couple of seconds of
/// speech and weighs a few hundred kilobytes. Anything wildly larger than
/// this means something other than a narration clip is on the other end.
const MAX_NARRATION_BYTES: usize = 8 << 20;

/// Spotify's own loudness normalization target (ITU-R BS.1770 LUFS). Matches
/// the target librespot's own track normalisation aims for; see CLAUDE.md's
/// note on `-14 LUFS` being Spotify's standard.
const TARGET_LUFS: f32 = -14.0;

/// Fetches a narration clip from its signed CDN URL.
///
/// Plain unauthenticated GET, exactly like the real client: the URL from
/// `client-tts/v1/fulfill`'s `Location` header is already signed, and sending
/// Spotify's own Authorization header to a plain CDN would be nonsensical.
pub async fn fetch_clip(http: &reqwest::Client, url: &str) -> AppResult<Vec<u8>> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|error| AppError::Playback(format!("narration fetch failed: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::Playback(format!(
            "narration fetch returned HTTP {}",
            response.status()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::Playback(format!("narration fetch failed: {error}")))?;
    if bytes.is_empty() {
        return Err(AppError::Playback("narration audio is empty".into()));
    }
    if bytes.len() > MAX_NARRATION_BYTES {
        return Err(AppError::Playback(format!(
            "narration audio is too large (over {MAX_NARRATION_BYTES} bytes)"
        )));
    }
    Ok(bytes.to_vec())
}

/// Gain to apply to a narration clip so it lands at Spotify's loudness
/// target, mirroring the standard LUFS-normalization formula (this is the
/// same math as ReplayGain/EBU R128 track-gain calculation, not anything
/// specific to any one player's implementation): the gap between the
/// measured integrated loudness and the target, plus a configurable pregain,
/// converted from dB to a linear factor, then capped against the clip's true
/// peak so the normalized result cannot exceed full scale.
///
/// `loudness_db`/`true_peak_db` come from the track's own
/// `narration.<kind>.loudness`/`.true_peak` metadata. Absence must not be
/// read as `0.0`: that would normalize against 0 LUFS and attenuate real
/// speech to nothing, so callers pass `None` through to `gain = 1.0`
/// (unity, i.e. don't touch it) instead.
pub fn narration_gain(loudness_db: Option<f32>, true_peak_db: Option<f32>, pregain_db: f32) -> f32 {
    let Some(loudness_db) = loudness_db else {
        return 1.0;
    };
    let gain_db = TARGET_LUFS - loudness_db + pregain_db;
    let mut gain = 10f32.powf(gain_db / 20.0);

    if let Some(true_peak_db) = true_peak_db {
        let true_peak_linear = 10f32.powf(true_peak_db / 20.0);
        if true_peak_linear > 0.0 && gain * true_peak_linear > 1.0 {
            gain = 1.0 / true_peak_linear;
        }
    }
    gain
}

/// Decodes an MP3 clip to interleaved `f32` PCM at librespot's fixed
/// `SAMPLE_RATE`/`NUM_CHANNELS`, with `gain` applied and clamped to
/// `[-1.0, 1.0]` so it can never clip the output.
///
/// Rustify always requests `SAMPLE_RATE` explicitly in the TTS request body
/// (see `spotify/dj.rs`), so no resample is needed for rate; channel count is
/// adapted here since synthesized speech commonly comes back mono while the
/// rest of the pipeline is stereo.
pub fn decode_clip(bytes: Vec<u8>, gain: f32) -> AppResult<Vec<f32>> {
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| {
            AppError::Playback(format!("narration audio is not valid MP3: {error}"))
        })?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| AppError::Playback("narration audio has no decodable track".into()))?;
    let track_id = track.id;
    let source_channels = track
        .codec_params
        .channels
        .map(|channels| channels.count())
        .unwrap_or(1)
        .max(1);
    let source_rate = track.codec_params.sample_rate.unwrap_or(SAMPLE_RATE);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| AppError::Playback(format!("no MP3 decoder available: {error}")))?;

    let mut mono_or_native: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(error) => {
                return Err(AppError::Playback(format!(
                    "narration audio decode failed: {error}"
                )))
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => append_interleaved(&mut mono_or_native, &decoded),
            // A single bad frame in a couple of seconds of speech is not worth
            // losing the whole clip over; keep decoding the rest.
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(error) => {
                return Err(AppError::Playback(format!(
                    "narration audio decode failed: {error}"
                )))
            }
        }
    }

    if mono_or_native.is_empty() {
        return Err(AppError::Playback(
            "narration audio decoded to silence".into(),
        ));
    }

    let resampled = resample_linear(&mono_or_native, source_rate, SAMPLE_RATE, source_channels);
    let mut stereo = adapt_channels(resampled, source_channels, NUM_CHANNELS as usize);

    for sample in &mut stereo {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }
    Ok(stereo)
}

fn append_interleaved(out: &mut Vec<f32>, buffer: &AudioBufferRef) {
    match buffer {
        AudioBufferRef::F32(buf) => append_planar(out, buf),
        AudioBufferRef::S32(buf) => {
            append_planar_converted(out, buf, |s| s as f32 / i32::MAX as f32)
        }
        AudioBufferRef::S16(buf) => {
            append_planar_converted(out, buf, |s| s as f32 / i16::MAX as f32)
        }
        AudioBufferRef::U8(buf) => {
            append_planar_converted(out, buf, |s| (s as f32 - 128.0) / 128.0)
        }
        // Other formats are not something Spotify's TTS CDN has been observed
        // to return; skip rather than guess at a conversion.
        _ => {}
    }
}

fn append_planar(out: &mut Vec<f32>, buf: &symphonia::core::audio::AudioBuffer<f32>) {
    let channels = buf.spec().channels.count().max(1);
    let frames = buf.frames();
    let base = out.len();
    out.resize(base + frames * channels, 0.0);
    for ch in 0..channels {
        let plane = buf.chan(ch);
        for (frame, sample) in plane.iter().enumerate() {
            out[base + frame * channels + ch] = *sample;
        }
    }
}

fn append_planar_converted<S: Sample>(
    out: &mut Vec<f32>,
    buf: &symphonia::core::audio::AudioBuffer<S>,
    convert: impl Fn(S) -> f32,
) {
    let channels = buf.spec().channels.count().max(1);
    let frames = buf.frames();
    let base = out.len();
    out.resize(base + frames * channels, 0.0);
    for ch in 0..channels {
        let plane = buf.chan(ch);
        for (frame, sample) in plane.iter().enumerate() {
            out[base + frame * channels + ch] = convert(*sample);
        }
    }
}

/// Simple linear-interpolation resampler. Narration clips are a couple of
/// seconds of speech, not music under critical listening, so a lightweight
/// resampler is the right tradeoff against pulling in a dedicated resampling
/// crate for one small, infrequent clip.
fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32, channels: usize) -> Vec<f32> {
    if from_rate == to_rate || channels == 0 || input.is_empty() {
        return input.to_vec();
    }
    let in_frames = input.len() / channels;
    let ratio = from_rate as f64 / to_rate as f64;
    let out_frames = ((in_frames as f64) / ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_frames * channels);
    for out_frame in 0..out_frames {
        let src_pos = out_frame as f64 * ratio;
        let src_index = src_pos.floor() as usize;
        let frac = (src_pos - src_index as f64) as f32;
        let a = src_index.min(in_frames - 1);
        let b = (src_index + 1).min(in_frames - 1);
        for ch in 0..channels {
            let sample_a = input[a * channels + ch];
            let sample_b = input[b * channels + ch];
            out.push(sample_a + (sample_b - sample_a) * frac);
        }
    }
    out
}

/// Adapts interleaved PCM between channel counts. Narration is commonly mono
/// (synthesized speech) while the rest of the pipeline is stereo.
fn adapt_channels(input: Vec<f32>, from_channels: usize, to_channels: usize) -> Vec<f32> {
    if from_channels == to_channels || from_channels == 0 || to_channels == 0 {
        return input;
    }
    let frames = input.len() / from_channels;
    let mut out = Vec::with_capacity(frames * to_channels);
    for frame in 0..frames {
        match (from_channels, to_channels) {
            (1, _) => {
                let sample = input[frame];
                out.extend(std::iter::repeat_n(sample, to_channels));
            }
            (_, 1) => {
                let sum: f32 = (0..from_channels)
                    .map(|ch| input[frame * from_channels + ch])
                    .sum();
                out.push(sum / from_channels as f32);
            }
            _ => {
                for ch in 0..to_channels {
                    out.push(input[frame * from_channels + ch.min(from_channels - 1)]);
                }
            }
        }
    }
    out
}

/// How often the playback thread checks `should_stop` while waiting out the
/// clip's duration. Short enough that a cancelled session goes quiet almost
/// immediately, long enough not to burn the thread spinning.
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Blocks the calling thread for up to `total`, in [`STOP_POLL_INTERVAL`]
/// chunks, returning early the first time `should_stop` reports `true`.
/// Extracted from [`play_clip`]'s stream-lifetime loop so the polling/timing
/// behavior can be unit-tested without opening a real `cpal` device.
fn wait_or_stop(total: Duration, should_stop: &impl Fn() -> bool) {
    let mut waited = Duration::ZERO;
    while waited < total {
        if should_stop() {
            return;
        }
        let chunk = STOP_POLL_INTERVAL.min(total - waited);
        std::thread::sleep(chunk);
        waited += chunk;
    }
}

/// Plays already-decoded interleaved stereo PCM through a short-lived `cpal`
/// stream on the currently-configured output device, and waits for it to
/// finish (or a generous timeout, so a device error can never hang DJ
/// playback indefinitely).
///
/// Deliberately independent of librespot's own Sink: that instance lives on
/// librespot's dedicated audio thread and is not reachable from here, and
/// narration is sequential with the track rather than mixed into it (see this
/// module's doc comment), so a second, short-lived stream is the simplest
/// correct design rather than a workaround.
///
/// `should_stop` is polled every [`STOP_POLL_INTERVAL`] while the clip plays
/// out and, if it ever returns `true`, the stream is dropped (silencing the
/// device) instead of waiting for the clip to finish naturally. Without this,
/// a logout or session replacement mid-narration left the underlying OS
/// thread sleeping for up to the clip's remaining duration — a few seconds of
/// audio outliving the session it belonged to — because nothing outside this
/// function's own async caller knew the thread existed. Callers with nothing
/// to cancel on can pass `|| false`. Generic over a plain closure rather than
/// this module depending on `AppState`/`AppHandle` directly, so it stays
/// testable and decoupled from Tauri state the way the rest of this file is.
pub async fn play_clip(
    runtime: &AudioRuntime,
    samples: Vec<f32>,
    should_stop: impl Fn() -> bool + Send + 'static,
) -> AppResult<()> {
    if samples.is_empty() {
        return Ok(());
    }
    let requested = runtime.requested_device();
    let clip_frames = samples.len() / NUM_CHANNELS as usize;
    let clip_duration =
        Duration::from_secs_f64(clip_frames as f64 / SAMPLE_RATE as f64) + Duration::from_secs(2);

    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<AppResult<()>>();
    // Wrapped before any fallible setup runs, so every exit path below —
    // including a device/stream-open failure — sends a definitive result
    // rather than silently dropping the sender. A dropped-without-sending
    // sender is deliberately not treated as success (see the match below):
    // an earlier version of this function did exactly that, and a smoke test
    // playing a real tone through it (`tests::plays_an_audible_tone`) caught
    // a broken device open reporting back as `Ok(())`.
    let done_tx = Arc::new(std::sync::Mutex::new(Some(done_tx)));
    let send_result = {
        let done_tx = done_tx.clone();
        move |result: AppResult<()>| {
            if let Some(tx) = done_tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = tx.send(result);
            }
        }
    };

    // cpal streams are not `Send` on every backend, so the stream itself must
    // live and die on one dedicated thread rather than crossing an await
    // point; only the completion signal crosses back to the async caller.
    std::thread::spawn(move || {
        let outcome = (|| -> AppResult<cpal::Stream> {
            let host = cpal::default_host();
            let device = match requested.as_deref() {
                Some(name) => host
                    .output_devices()
                    .map_err(|e| AppError::Playback(format!("no audio outputs: {e}")))?
                    .find(|device| device.name().map(|n| n == name).unwrap_or(false))
                    .or_else(|| host.default_output_device()),
                None => host.default_output_device(),
            }
            .ok_or_else(|| AppError::Playback("no audio output device available".into()))?;

            // `samples` decodes to a fixed 44.1kHz/stereo (librespot's own
            // format), but the output device is under no obligation to accept
            // that directly — forcing it produced exactly this failure on a
            // real machine during development ("requested stream
            // configuration is not supported by the device"). Adapt to
            // whatever the device's own default config actually is instead of
            // assuming ours is accepted.
            let default_config = device.default_output_config().map_err(|e| {
                AppError::Playback(format!("no supported narration stream config: {e}"))
            })?;
            let device_channels = default_config.channels() as usize;
            let device_rate = default_config.sample_rate().0;
            let samples =
                resample_linear(&samples, SAMPLE_RATE, device_rate, NUM_CHANNELS as usize);
            let samples = adapt_channels(samples, NUM_CHANNELS as usize, device_channels);

            let config = cpal::StreamConfig {
                channels: device_channels as cpal::ChannelCount,
                sample_rate: cpal::SampleRate(device_rate),
                buffer_size: cpal::BufferSize::Default,
            };

            let position = Arc::new(AtomicUsize::new(0));
            let read_position = position.clone();
            let total = samples.len();
            let done_tx_data = send_result.clone();
            let done_tx_error = send_result.clone();

            let stream = device
                .build_output_stream(
                    &config,
                    move |output: &mut [f32], _| {
                        let start = read_position.load(Ordering::Acquire);
                        let remaining = total.saturating_sub(start);
                        let take = remaining.min(output.len());
                        output[..take].copy_from_slice(&samples[start..start + take]);
                        for sample in &mut output[take..] {
                            *sample = 0.0;
                        }
                        read_position.store(start + take, Ordering::Release);
                        if take < output.len() {
                            done_tx_data(Ok(()));
                        }
                    },
                    move |error| {
                        done_tx_error(Err(AppError::Playback(format!(
                            "narration playback failed: {error}"
                        ))));
                    },
                    None,
                )
                .map_err(|e| AppError::Playback(format!("could not open narration stream: {e}")))?;
            stream.play().map_err(|e| {
                AppError::Playback(format!("could not start narration stream: {e}"))
            })?;
            Ok(stream)
        })();

        match outcome {
            Ok(stream) => {
                // Keep the stream alive until playback reports completion or
                // `should_stop` fires, whichever comes first — the only way a
                // cancelled session can make the device go quiet before the
                // clip finishes on its own. The callback above still signals
                // completion through `done_rx` on the async side
                // independently of this loop.
                wait_or_stop(clip_duration, &should_stop);
                drop(stream);
            }
            Err(error) => {
                log::debug!(target: "spotify.dj", "narration stream setup failed: {error}");
                send_result(Err(error));
            }
        }
    });

    match tokio::time::timeout(clip_duration, done_rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(AppError::Playback(
            "narration stream ended without confirming playback".into(),
        )),
        Err(_) => Err(AppError::Playback("narration playback timed out".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual-only smoke test for the actual `cpal` playback path: opens a
    /// real output stream and plays an audible tone through it. Ignored by
    /// default — like the rest of this project's manual checklist (see
    /// README.md), this exercises real hardware, which `cargo test --lib`
    /// deliberately never does. Run with:
    /// `cargo test --no-default-features --lib narration::tests::plays_an_audible_tone -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn plays_an_audible_tone() {
        let runtime = AudioRuntime::default();
        let frames = SAMPLE_RATE as usize; // ~1s
        let mut samples = Vec::with_capacity(frames * NUM_CHANNELS as usize);
        for i in 0..frames {
            let t = i as f32 / SAMPLE_RATE as f32;
            let s = (t * 440.0 * std::f32::consts::TAU).sin() * 0.2;
            samples.push(s);
            samples.push(s);
        }
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(play_clip(&runtime, samples, || false));
        println!("play_clip result: {result:?}");
        assert!(result.is_ok());
    }

    /// Regression test for the bug this session's fix addresses: without
    /// `should_stop`, a cancelled session's narration thread kept sleeping
    /// (and the device kept outputting audio) for the clip's full remaining
    /// duration. Runs no real audio hardware — just the timing/polling loop.
    #[test]
    fn wait_or_stop_returns_immediately_when_already_stopped() {
        let start = std::time::Instant::now();
        wait_or_stop(Duration::from_secs(5), &|| true);
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "should_stop=true must not wait out anywhere near the full duration"
        );
    }

    #[test]
    fn wait_or_stop_waits_the_full_duration_when_never_stopped() {
        let total = Duration::from_millis(50);
        let start = std::time::Instant::now();
        wait_or_stop(total, &|| false);
        assert!(
            start.elapsed() >= total,
            "should_stop=false must wait out the full duration"
        );
    }

    #[test]
    fn wait_or_stop_stops_partway_through_once_the_flag_flips() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let start = std::time::Instant::now();
        // Flips true on the second poll — before a 5s duration could ever
        // elapse on its own, so a passing test proves the early exit fired.
        wait_or_stop(Duration::from_secs(5), &|| {
            calls.fetch_add(1, Ordering::Relaxed) >= 1
        });
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "must stop shortly after the flag flips, not wait out the full 5s"
        );
    }

    #[test]
    fn gain_is_unity_without_loudness_metadata() {
        assert_eq!(narration_gain(None, None, 0.0), 1.0);
    }

    #[test]
    fn gain_boosts_quiet_clips_toward_the_target() {
        // -20 LUFS clip should be boosted by 6 dB towards -14 LUFS.
        let gain = narration_gain(Some(-20.0), None, 0.0);
        assert!((gain - 10f32.powf(6.0 / 20.0)).abs() < 0.001);
    }

    #[test]
    fn gain_is_clamped_by_true_peak_to_avoid_clipping() {
        // A clip already close to full scale must not be boosted past it.
        let gain = narration_gain(Some(-6.0), Some(-0.5), 0.0);
        let true_peak_linear = 10f32.powf(-0.5f32 / 20.0);
        assert!(gain * true_peak_linear <= 1.0 + 1e-4);
    }

    #[test]
    fn mono_upmixes_to_stereo() {
        let out = adapt_channels(vec![0.5, -0.5], 1, 2);
        assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5]);
    }

    #[test]
    fn stereo_downmixes_to_mono() {
        let out = adapt_channels(vec![1.0, 0.0, -1.0, 0.0], 2, 1);
        assert_eq!(out, vec![0.5, -0.5]);
    }

    #[test]
    fn resample_is_a_no_op_at_matching_rates() {
        let input = vec![0.1, 0.2, 0.3, 0.4];
        assert_eq!(resample_linear(&input, 44_100, 44_100, 2), input);
    }

    #[test]
    fn resample_changes_frame_count_proportionally() {
        let input: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect(); // mono, 100 frames
        let out = resample_linear(&input, 44_100, 22_050, 1);
        assert!((out.len() as i64 - 50).abs() <= 1);
    }
}
