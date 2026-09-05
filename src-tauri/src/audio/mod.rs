use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use biquad::{Biquad, Coefficients, DirectForm1, ToHertz, Type};
use cpal::traits::{DeviceTrait, HostTrait};
use librespot::playback::audio_backend::{Sink, SinkBuilder, SinkResult};
use librespot::playback::config::{AudioFormat, Bitrate};
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;
use librespot::playback::{NUM_CHANNELS, SAMPLE_RATE};
use serde::{Deserialize, Serialize};
#[cfg(feature = "ts-rs-export")]
use ts_rs::TS;

use crate::error::{AppError, AppResult};

pub const EQ_FREQUENCIES: [f64; 6] = [60.0, 150.0, 400.0, 1_000.0, 2_400.0, 15_000.0];
const EQ_BANDS: usize = EQ_FREQUENCIES.len();
const MIN_GAIN_DB: f32 = -12.0;
const MAX_GAIN_DB: f32 = 12.0;
const MIN_PREAMP_DB: f32 = -12.0;
const MAX_PREAMP_DB: f32 = 0.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs-export", derive(TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts-rs-export",
    ts(export, export_to = "../../src/lib/generated/")
)]
pub enum StreamQuality {
    /// Uses librespot's stable default. The installed version does not expose
    /// a bandwidth-adaptive selector, so this currently resolves to 160 kbps.
    #[default]
    Automatic,
    Low,
    Normal,
    VeryHigh,
}

impl StreamQuality {
    pub fn bitrate(self) -> Bitrate {
        match self {
            Self::Low => Bitrate::Bitrate96,
            Self::Automatic | Self::Normal => Bitrate::Bitrate160,
            Self::VeryHigh => Bitrate::Bitrate320,
        }
    }

    pub fn effective_label(self) -> &'static str {
        match self.bitrate() {
            Bitrate::Bitrate96 => "96 kbps",
            Bitrate::Bitrate160 => "160 kbps",
            Bitrate::Bitrate320 => "320 kbps",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqualizerPreset {
    pub id: String,
    pub name: String,
    pub bands_db: [f32; EQ_BANDS],
    pub preamp_db: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqualizerSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bands_db: [f32; EQ_BANDS],
    #[serde(default)]
    pub preamp_db: f32,
    #[serde(default = "default_auto_headroom")]
    pub auto_headroom: bool,
    #[serde(default)]
    pub active_preset_id: Option<String>,
    #[serde(default)]
    pub custom_presets: Vec<EqualizerPreset>,
}

const fn default_auto_headroom() -> bool {
    true
}

impl Default for EqualizerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            bands_db: [0.0; EQ_BANDS],
            preamp_db: 0.0,
            auto_headroom: true,
            active_preset_id: Some("flat".into()),
            custom_presets: Vec::new(),
        }
    }
}

/// Track/album loudness normalization toward Spotify's own -14 LUFS target
/// (ITU-R BS.1770), applied by the pinned librespot fork itself
/// (`PlayerConfig.normalisation*`) rather than anything Rustify computes —
/// see `player::playback_config`. `pregain_db` is the one knob exposed here;
/// librespot's own normalisation type/method/compressor tuning
/// (`NormalisationType::Auto`, `NormalisationMethod::Dynamic`, and its
/// threshold/attack/release/knee defaults) are left as-is rather than turned
/// into more settings than this app needs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoudnessSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub pregain_db: f32,
}

impl Default for LoudnessSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            pregain_db: 0.0,
        }
    }
}

pub fn validate_loudness(settings: &LoudnessSettings) -> AppResult<()> {
    if !settings.pregain_db.is_finite()
        || !(MIN_GAIN_DB..=MAX_GAIN_DB).contains(&settings.pregain_db)
    {
        return Err(AppError::BadRequest(
            "Loudness pregain must be between -12 dB and +12 dB.".into(),
        ));
    }
    Ok(())
}

pub fn builtin_presets() -> Vec<EqualizerPreset> {
    [
        ("flat", "Flat", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 0.0),
        ("bass", "Bass lift", [5.0, 3.5, 1.0, 0.0, -1.0, -1.5], 0.0),
        (
            "vocal",
            "Vocal clarity",
            [-1.5, -1.0, 0.5, 2.5, 3.0, 1.0],
            0.0,
        ),
        ("bright", "Bright", [-1.5, -1.0, 0.0, 1.5, 3.0, 4.0], 0.0),
        (
            "late",
            "Late night",
            [1.5, 1.0, 0.0, -1.0, -2.0, -3.0],
            -2.0,
        ),
    ]
    .into_iter()
    .map(|(id, name, bands_db, preamp_db)| EqualizerPreset {
        id: id.into(),
        name: name.into(),
        bands_db,
        preamp_db,
    })
    .collect()
}

pub fn validate_equalizer(eq: &EqualizerSettings) -> AppResult<()> {
    if eq
        .bands_db
        .iter()
        .any(|gain| !gain.is_finite() || !(MIN_GAIN_DB..=MAX_GAIN_DB).contains(gain))
    {
        return Err(AppError::BadRequest(
            "Equalizer bands must be finite values between -12 dB and +12 dB.".into(),
        ));
    }
    if !eq.preamp_db.is_finite() || !(MIN_PREAMP_DB..=MAX_PREAMP_DB).contains(&eq.preamp_db) {
        return Err(AppError::BadRequest(
            "Equalizer preamp must be between -12 dB and 0 dB.".into(),
        ));
    }
    if eq.custom_presets.len() > 32 {
        return Err(AppError::BadRequest(
            "At most 32 custom equalizer presets can be stored.".into(),
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for preset in &eq.custom_presets {
        if preset.id.trim().is_empty() || preset.name.trim().is_empty() {
            return Err(AppError::BadRequest(
                "Custom presets need a name and identifier.".into(),
            ));
        }
        if !ids.insert(preset.id.as_str()) {
            return Err(AppError::BadRequest(
                "Custom preset identifiers must be unique.".into(),
            ));
        }
        if preset
            .bands_db
            .iter()
            .any(|gain| !gain.is_finite() || !(MIN_GAIN_DB..=MAX_GAIN_DB).contains(gain))
            || !preset.preamp_db.is_finite()
            || !(MIN_PREAMP_DB..=MAX_PREAMP_DB).contains(&preset.preamp_db)
        {
            return Err(AppError::BadRequest(
                "Custom preset values are outside the supported range.".into(),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_selected: bool,
    pub is_active: bool,
    pub is_available: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    pub active_device: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct RuntimeState {
    requested_device: Option<String>,
    active_device: Option<String>,
    equalizer: EqualizerSettings,
    last_error: Option<String>,
}

/// Small synchronous snapshot shared with librespot's dedicated audio thread.
/// No async/Tauri lock is ever acquired from the latency-sensitive sink path.
#[derive(Clone, Default)]
pub struct AudioRuntime(Arc<RwLock<RuntimeState>>);

impl AudioRuntime {
    pub fn configure(&self, device: Option<String>, equalizer: EqualizerSettings) {
        let mut state = self.0.write().unwrap_or_else(|e| e.into_inner());
        state.requested_device = clean_device(device);
        state.equalizer = equalizer;
        state.last_error = None;
    }

    pub fn requested_device(&self) -> Option<String> {
        self.0
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .requested_device
            .clone()
    }

    fn equalizer(&self) -> EqualizerSettings {
        self.0
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .equalizer
            .clone()
    }

    fn report_output(&self, active_device: Option<String>, error: Option<String>) {
        let mut state = self.0.write().unwrap_or_else(|e| e.into_inner());
        state.active_device = active_device;
        state.last_error = error;
    }

    pub fn status(&self) -> AudioStatus {
        let state = self.0.read().unwrap_or_else(|e| e.into_inner());
        AudioStatus {
            active_device: state.active_device.clone(),
            last_error: state.last_error.clone(),
        }
    }
}

fn clean_device(device: Option<String>) -> Option<String> {
    device.and_then(|name| {
        let trimmed = name.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn output_names() -> AppResult<(Option<String>, Vec<String>)> {
    let host = cpal::default_host();
    let default = host.default_output_device().and_then(|d| d.name().ok());
    let mut names = host
        .output_devices()
        .map_err(|e| AppError::Playback(format!("could not enumerate audio outputs: {e}")))?
        .filter_map(|device| device.name().ok())
        .collect::<Vec<_>>();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    Ok((default, names))
}

pub fn list_output_devices(runtime: &AudioRuntime) -> AppResult<Vec<AudioDevice>> {
    let selected = runtime.requested_device();
    let status = runtime.status();
    let (default, names) = output_names()?;
    let mut devices = names
        .into_iter()
        .map(|name| AudioDevice {
            id: name.clone(),
            is_default: default.as_deref() == Some(name.as_str()),
            is_selected: selected.as_deref() == Some(name.as_str())
                || (selected.is_none() && default.as_deref() == Some(name.as_str())),
            is_active: status.active_device.as_deref() == Some(name.as_str()),
            is_available: true,
            name,
        })
        .collect::<Vec<_>>();
    if let Some(missing) =
        selected.filter(|requested| !devices.iter().any(|device| device.id == *requested))
    {
        devices.insert(
            0,
            AudioDevice {
                id: missing.clone(),
                name: missing,
                is_default: false,
                is_selected: true,
                is_active: false,
                is_available: false,
            },
        );
    }
    Ok(devices)
}

fn available_device(requested: Option<&str>) -> (Option<String>, Option<String>) {
    match output_names() {
        Ok((_default, names)) => match requested {
            Some(name) if names.iter().any(|candidate| candidate == name) => {
                (Some(name.to_string()), None)
            }
            Some(name) => (
                None,
                Some(format!(
                    "Audio output ‘{name}’ disconnected; using the system default."
                )),
            ),
            None => (None, None),
        },
        Err(error) => (None, Some(error.to_string())),
    }
}

pub fn processing_sink(
    inner_builder: SinkBuilder,
    format: AudioFormat,
    runtime: AudioRuntime,
) -> Box<dyn Sink> {
    Box::new(ProcessingSink::new(inner_builder, format, runtime))
}

struct ProcessingSink {
    builder: SinkBuilder,
    format: AudioFormat,
    runtime: AudioRuntime,
    inner: Box<dyn Sink>,
    requested_at_open: Option<String>,
    retry_missing_after: Instant,
    started: bool,
    equalizer: EqualizerProcessor,
}

impl ProcessingSink {
    fn new(builder: SinkBuilder, format: AudioFormat, runtime: AudioRuntime) -> Self {
        let requested = runtime.requested_device();
        let (resolved, warning) = available_device(requested.as_deref());
        let (inner, active, open_error) = open_sink(builder, format, resolved.clone());
        runtime.report_output(active, warning.or(open_error));
        Self {
            builder,
            format,
            runtime,
            inner,
            requested_at_open: requested,
            retry_missing_after: Instant::now() + Duration::from_secs(5),
            started: false,
            equalizer: EqualizerProcessor::new(),
        }
    }

    fn refresh_device(&mut self) {
        let requested = self.runtime.requested_device();
        let changed = requested != self.requested_at_open;
        let retry_missing = Instant::now() >= self.retry_missing_after
            && requested.is_some()
            && self.runtime.status().active_device != requested;
        if !changed && !retry_missing {
            return;
        }

        let (resolved, warning) = available_device(requested.as_deref());
        let (mut next, active, open_error) = open_sink(self.builder, self.format, resolved);
        if self.started {
            if let Err(error) = next.start() {
                self.runtime.report_output(
                    self.runtime.status().active_device,
                    Some(format!(
                        "could not start the selected audio output: {error}"
                    )),
                );
                self.retry_missing_after = Instant::now() + Duration::from_secs(5);
                return;
            }
        }
        self.inner = next;
        self.requested_at_open = requested;
        self.retry_missing_after = Instant::now() + Duration::from_secs(5);
        self.runtime.report_output(active, warning.or(open_error));
    }
}

fn open_sink(
    builder: SinkBuilder,
    format: AudioFormat,
    device: Option<String>,
) -> (Box<dyn Sink>, Option<String>, Option<String>) {
    let chosen = device.clone();
    match catch_unwind(AssertUnwindSafe(|| builder(device, format))) {
        Ok(sink) => {
            let active = chosen.or_else(|| {
                cpal::default_host()
                    .default_output_device()
                    .and_then(|d| d.name().ok())
            });
            (sink, active, None)
        }
        Err(_) => {
            let fallback = builder(None, format);
            let active = cpal::default_host()
                .default_output_device()
                .and_then(|d| d.name().ok());
            (
                fallback,
                active,
                Some(
                    "The selected audio output could not be opened; using the system default."
                        .into(),
                ),
            )
        }
    }
}

impl Sink for ProcessingSink {
    fn start(&mut self) -> SinkResult<()> {
        self.started = true;
        self.inner.start()
    }

    fn stop(&mut self) -> SinkResult<()> {
        self.started = false;
        self.inner.stop()
    }

    fn write(&mut self, mut packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        self.refresh_device();
        if let AudioPacket::Samples(samples) = &mut packet {
            self.equalizer.process(samples, &self.runtime.equalizer());
        }
        match self.inner.write(packet, converter) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.runtime.report_output(
                    self.runtime.status().active_device,
                    Some(format!("audio output failed: {error}")),
                );
                Err(error)
            }
        }
    }
}

struct EqualizerProcessor {
    filters: [[DirectForm1<f64>; EQ_BANDS]; NUM_CHANNELS as usize],
    current_gains: [f64; EQ_BANDS],
    current_preamp_db: f64,
    wet: f64,
}

impl EqualizerProcessor {
    fn new() -> Self {
        let filters = std::array::from_fn(|_| {
            std::array::from_fn(|index| {
                DirectForm1::new(eq_coefficients(EQ_FREQUENCIES[index], 0.0))
            })
        });
        Self {
            filters,
            current_gains: [0.0; EQ_BANDS],
            current_preamp_db: 0.0,
            wet: 0.0,
        }
    }

    fn process(&mut self, samples: &mut [f64], settings: &EqualizerSettings) {
        let frames = samples.len() / NUM_CHANNELS as usize;
        if frames == 0 {
            return;
        }
        if !settings.enabled && self.wet <= 0.0001 {
            self.wet = 0.0;
            return;
        }

        // One coefficient retune per decoded packet. Direct Form 1 is used
        // specifically because the crate documents it as the low-artifact
        // topology for filters that are adjusted online.
        let smoothing = 1.0 - (-(frames as f64) / (SAMPLE_RATE as f64 * 0.035)).exp();
        for (index, gain) in settings.bands_db.iter().enumerate() {
            self.current_gains[index] += (*gain as f64 - self.current_gains[index]) * smoothing;
            let coefficients = eq_coefficients(EQ_FREQUENCIES[index], self.current_gains[index]);
            for channel in &mut self.filters {
                channel[index].update_coefficients(coefficients);
            }
        }
        let max_boost = settings.bands_db.iter().copied().fold(0.0_f32, f32::max);
        let target_preamp = settings.preamp_db as f64
            - if settings.auto_headroom {
                max_boost as f64
            } else {
                0.0
            };
        self.current_preamp_db += (target_preamp - self.current_preamp_db) * smoothing;

        let target_wet = if settings.enabled { 1.0 } else { 0.0 };
        let wet_step = (target_wet - self.wet) / frames as f64;
        let amplitude = 10.0_f64.powf(self.current_preamp_db / 20.0);

        for frame in samples.chunks_exact_mut(NUM_CHANNELS as usize) {
            self.wet = (self.wet + wet_step).clamp(0.0, 1.0);
            for (channel_index, sample) in frame.iter_mut().enumerate() {
                let dry = *sample;
                let mut processed = dry;
                for filter in &mut self.filters[channel_index] {
                    processed = filter.run(processed);
                }
                *sample =
                    (dry * (1.0 - self.wet) + processed * amplitude * self.wet).clamp(-1.0, 1.0);
            }
        }
    }
}

fn eq_coefficients(frequency: f64, gain_db: f64) -> Coefficients<f64> {
    Coefficients::from_params(
        Type::PeakingEQ(gain_db),
        (SAMPLE_RATE as f64).hz(),
        frequency.hz(),
        1.0,
    )
    .expect("fixed equalizer frequencies are below Nyquist with a positive Q")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_maps_only_to_formats_the_installed_player_can_request() {
        assert_eq!(StreamQuality::Low.bitrate(), Bitrate::Bitrate96);
        assert_eq!(StreamQuality::Normal.bitrate(), Bitrate::Bitrate160);
        assert_eq!(StreamQuality::Automatic.bitrate(), Bitrate::Bitrate160);
        assert_eq!(StreamQuality::VeryHigh.bitrate(), Bitrate::Bitrate320);
    }

    #[test]
    fn bypass_is_sample_exact() {
        let mut processor = EqualizerProcessor::new();
        let mut samples = vec![0.25, -0.25, 0.5, -0.5];
        let expected = samples.clone();
        processor.process(&mut samples, &EqualizerSettings::default());
        assert_eq!(samples, expected);
    }

    #[test]
    fn invalid_equalizer_values_are_rejected() {
        let mut settings = EqualizerSettings::default();
        settings.bands_db[2] = 13.0;
        assert!(validate_equalizer(&settings).is_err());
        settings.bands_db[2] = 0.0;
        settings.preamp_db = -13.0;
        assert!(validate_equalizer(&settings).is_err());
    }

    fn stereo_sine(frequency: f64, frames: usize, amplitude: f64) -> Vec<f64> {
        (0..frames)
            .flat_map(|frame| {
                let phase = std::f64::consts::TAU * frequency * frame as f64 / SAMPLE_RATE as f64;
                let sample = phase.sin() * amplitude;
                [sample, sample]
            })
            .collect()
    }

    fn process_in_decoder_packets(
        processor: &mut EqualizerProcessor,
        samples: &mut [f64],
        settings: &EqualizerSettings,
    ) {
        for packet in samples.chunks_mut(2_048) {
            processor.process(packet, settings);
        }
    }

    fn rms(samples: &[f64]) -> f64 {
        (samples.iter().map(|sample| sample * sample).sum::<f64>() / samples.len() as f64).sqrt()
    }

    #[test]
    fn one_kilohertz_band_changes_the_real_sample_stream() {
        let mut flat_samples = stereo_sine(1_000.0, SAMPLE_RATE as usize, 0.05);
        let mut boosted_samples = flat_samples.clone();
        let flat = EqualizerSettings {
            enabled: true,
            auto_headroom: false,
            ..EqualizerSettings::default()
        };
        let mut boosted = flat.clone();
        boosted.bands_db[3] = 12.0;

        process_in_decoder_packets(&mut EqualizerProcessor::new(), &mut flat_samples, &flat);
        process_in_decoder_packets(
            &mut EqualizerProcessor::new(),
            &mut boosted_samples,
            &boosted,
        );
        let settled = flat_samples.len() / 2;
        let gain_ratio = rms(&boosted_samples[settled..]) / rms(&flat_samples[settled..]);

        assert!(
            gain_ratio > 2.5,
            "expected an audible boost, got {gain_ratio:.2}x"
        );
    }

    #[test]
    fn bypass_settles_back_to_sample_exact_dry_audio() {
        let mut processor = EqualizerProcessor::new();
        let mut enabled = EqualizerSettings {
            enabled: true,
            bands_db: [8.0, -4.0, 3.0, 0.0, 2.0, -5.0],
            ..EqualizerSettings::default()
        };
        let mut warmup = stereo_sine(400.0, 4_096, 0.1);
        processor.process(&mut warmup, &enabled);

        enabled.enabled = false;
        let mut fade = stereo_sine(400.0, 2_048, 0.1);
        processor.process(&mut fade, &enabled);
        let mut dry = stereo_sine(400.0, 2_048, 0.1);
        let expected = dry.clone();
        processor.process(&mut dry, &enabled);

        assert_eq!(dry, expected);
    }

    #[test]
    fn rapid_valid_curve_changes_remain_finite_and_bounded() {
        let mut processor = EqualizerProcessor::new();
        let mut settings = EqualizerSettings {
            enabled: true,
            auto_headroom: false,
            ..EqualizerSettings::default()
        };
        for step in 0..80 {
            for (index, gain) in settings.bands_db.iter_mut().enumerate() {
                *gain = if (step + index) % 2 == 0 { 12.0 } else { -12.0 };
            }
            settings.preamp_db = if step % 2 == 0 { 0.0 } else { -12.0 };
            let mut samples = stereo_sine(1_000.0, 256, 0.8);
            processor.process(&mut samples, &settings);
            assert!(samples.iter().all(|sample| sample.is_finite()));
            assert!(samples.iter().all(|sample| (-1.0..=1.0).contains(sample)));
        }
    }
}
