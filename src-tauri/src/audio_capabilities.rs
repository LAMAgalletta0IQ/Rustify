//! Modern Spotify audio-format and storage-resolution capability inspection.
//!
//! This is deliberately separate from key acquisition. Metadata, format
//! selection, v2 storage resolution, and FLAC decoding are ordinary plumbing;
//! Spotify-native FLAC key derivation currently depends on the unavailable
//! PlayPlay protected component and is never attempted here.

use http::Method;
use librespot::{
    core::{Session, SpotifyUri},
    metadata::{
        audio::{AudioFileFormat, AudioFiles},
        Metadata, Track,
    },
};
use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AudioFormatCapability {
    pub id: i32,
    pub name: String,
    pub codec: String,
    pub nominal_kbps: Option<u32>,
    pub lossless: bool,
    pub bit_depth: Option<u8>,
    pub decoder_supported: bool,
    pub current_player_selectable: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StorageCapability {
    pub endpoint_version: String,
    pub attempted: bool,
    pub available: bool,
    pub result: Option<String>,
    pub cdn_url_count: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AudioCapability {
    pub track_uri: String,
    pub formats: Vec<AudioFormatCapability>,
    pub preferred_format: Option<AudioFormatCapability>,
    pub storage: StorageCapability,
    pub lossless_metadata_available: bool,
    pub lossless_decoder_available: bool,
    pub lossless_playback_available: bool,
    pub lossless_blocker: Option<String>,
}

pub async fn inspect(session: &Session, track_uri: &str) -> AppResult<AudioCapability> {
    let uri = validate_track_uri(track_uri)?;
    let track = Track::get(session, &uri)
        .await
        .map_err(|error| AppError::Playback(format!("audio metadata lookup failed: {error}")))?;

    let mut available = track
        .files
        .iter()
        .map(|(format, file_id)| (*format, *file_id))
        .collect::<Vec<_>>();
    available.sort_by_key(|(format, _)| std::cmp::Reverse(format_rank(*format)));

    let formats = available
        .iter()
        .map(|(format, _)| describe(*format))
        .collect::<Vec<_>>();
    let preferred = available.first().copied();
    let storage = match preferred {
        Some((format, file_id)) => probe_storage(session, format, &file_id.to_base16()).await,
        None => StorageCapability {
            endpoint_version: "v2-format-aware".to_string(),
            attempted: false,
            available: false,
            result: None,
            cdn_url_count: 0,
            error: Some("track metadata exposed no audio files".to_string()),
        },
    };

    let lossless_metadata_available = formats.iter().any(|format| format.lossless);
    let lossless_decoder_available = formats
        .iter()
        .any(|format| format.lossless && format.decoder_supported);

    Ok(AudioCapability {
        track_uri: track_uri.to_string(),
        formats,
        preferred_format: preferred.map(|(format, _)| describe(format)),
        storage,
        lossless_metadata_available,
        lossless_decoder_available,
        lossless_playback_available: false,
        lossless_blocker: lossless_metadata_available.then(|| {
            "Spotify FLAC requires a PlayPlay protected-key implementation. Rustify has format-aware storage and FLAC decoding but will not use leaked tokens, static secrets, or DRM-circumvention code."
                .to_string()
        }),
    })
}

async fn probe_storage(
    session: &Session,
    format: AudioFileFormat,
    file_id: &str,
) -> StorageCapability {
    let endpoint = format!(
        "/storage-resolve/v2/files/audio/interactive/{}/{file_id}",
        format_id(format)
    );
    match session
        .spclient()
        .request(&Method::GET, &endpoint, None, None)
        .await
    {
        Ok(bytes) => match parse_storage_response(&bytes) {
            Ok((result, cdn_url_count)) => StorageCapability {
                endpoint_version: "v2-format-aware".to_string(),
                attempted: true,
                available: result == 0 && cdn_url_count > 0,
                result: Some(storage_result(result).to_string()),
                cdn_url_count,
                error: None,
            },
            Err(error) => StorageCapability {
                endpoint_version: "v2-format-aware".to_string(),
                attempted: true,
                available: false,
                result: None,
                cdn_url_count: 0,
                error: Some(error.to_string()),
            },
        },
        Err(error) => StorageCapability {
            endpoint_version: "v2-format-aware".to_string(),
            attempted: true,
            available: false,
            result: None,
            cdn_url_count: 0,
            error: Some(format!("storage resolution unavailable: {error}")),
        },
    }
}

fn validate_track_uri(value: &str) -> AppResult<SpotifyUri> {
    let uri = SpotifyUri::from_uri(value)
        .map_err(|error| AppError::BadRequest(format!("invalid Spotify URI: {error}")))?;
    if uri.item_type() != "track" || uri.to_uri() != value {
        return Err(AppError::BadRequest(
            "audio capability requires a canonical Spotify track URI".to_string(),
        ));
    }
    Ok(uri)
}

fn describe(format: AudioFileFormat) -> AudioFormatCapability {
    use AudioFileFormat::*;
    let (name, codec, nominal_kbps, bit_depth) = match format {
        OGG_VORBIS_96 => ("Ogg Vorbis 96", "vorbis", Some(96), None),
        OGG_VORBIS_160 => ("Ogg Vorbis 160", "vorbis", Some(160), None),
        OGG_VORBIS_320 => ("Ogg Vorbis 320", "vorbis", Some(320), None),
        MP3_96 => ("MP3 96", "mp3", Some(96), None),
        MP3_160 | MP3_160_ENC => ("MP3 160", "mp3", Some(160), None),
        MP3_256 => ("MP3 256", "mp3", Some(256), None),
        MP3_320 => ("MP3 320", "mp3", Some(320), None),
        AAC_24 => ("AAC 24", "aac", Some(24), None),
        AAC_48 => ("AAC 48", "aac", Some(48), None),
        AAC_160 => ("AAC 160", "aac", Some(160), None),
        AAC_320 => ("AAC 320", "aac", Some(320), None),
        MP4_128 => ("MP4 128", "mp4", Some(128), None),
        XHE_AAC_12 => ("xHE-AAC 12", "xhe-aac", Some(12), None),
        XHE_AAC_16 => ("xHE-AAC 16", "xhe-aac", Some(16), None),
        XHE_AAC_24 => ("xHE-AAC 24", "xhe-aac", Some(24), None),
        FLAC_FLAC => ("FLAC", "flac", None, Some(16)),
        FLAC_FLAC_24BIT => ("FLAC 24-bit", "flac", None, Some(24)),
        OTHER5 => ("Unknown format 13", "unknown", None, None),
    };
    let lossless = matches!(format, FLAC_FLAC | FLAC_FLAC_24BIT);
    AudioFormatCapability {
        id: format_id(format),
        name: name.to_string(),
        codec: codec.to_string(),
        nominal_kbps,
        lossless,
        bit_depth,
        // Pinned librespot includes Symphonia's FLAC reader, but its helper
        // currently recognizes only the 16-bit enum as FLAC.
        decoder_supported: AudioFiles::is_ogg_vorbis(format)
            || AudioFiles::is_mp3(format)
            || matches!(format, FLAC_FLAC),
        // The pinned selector's preference arrays contain Ogg and MP3 only.
        current_player_selectable: AudioFiles::is_ogg_vorbis(format) || AudioFiles::is_mp3(format),
    }
}

fn format_id(format: AudioFileFormat) -> i32 {
    use AudioFileFormat::*;
    match format {
        OGG_VORBIS_96 => 0,
        OGG_VORBIS_160 => 1,
        OGG_VORBIS_320 => 2,
        MP3_256 => 3,
        MP3_320 => 4,
        MP3_160 => 5,
        MP3_96 => 6,
        MP3_160_ENC => 7,
        AAC_24 => 8,
        AAC_48 => 9,
        AAC_160 => 10,
        AAC_320 => 11,
        MP4_128 => 12,
        OTHER5 => 13,
        FLAC_FLAC => 16,
        XHE_AAC_24 => 18,
        XHE_AAC_16 => 19,
        XHE_AAC_12 => 20,
        FLAC_FLAC_24BIT => 22,
    }
}

fn format_rank(format: AudioFileFormat) -> u8 {
    use AudioFileFormat::*;
    match format {
        FLAC_FLAC_24BIT => 100,
        FLAC_FLAC => 90,
        OGG_VORBIS_320 | MP3_320 | AAC_320 => 80,
        MP3_256 => 70,
        OGG_VORBIS_160 | MP3_160 | MP3_160_ENC | AAC_160 => 60,
        MP4_128 => 50,
        OGG_VORBIS_96 | MP3_96 => 40,
        AAC_48 => 30,
        AAC_24 | XHE_AAC_24 => 20,
        XHE_AAC_16 => 15,
        XHE_AAC_12 => 10,
        OTHER5 => 0,
    }
}

fn parse_storage_response(bytes: &[u8]) -> AppResult<(u64, usize)> {
    let mut input = bytes;
    let mut result = 0;
    let mut cdn_count = 0;
    while !input.is_empty() {
        let key = varint(&mut input)?;
        let field = key >> 3;
        match (field, key & 7) {
            (1, 0) => result = varint(&mut input)?,
            (2, 2) => {
                let value = take_bytes(&mut input)?;
                let url = std::str::from_utf8(value)
                    .map_err(|_| AppError::Other("storage URL was not UTF-8".to_string()))?;
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    return Err(AppError::Other(
                        "storage response contained an invalid CDN URL".to_string(),
                    ));
                }
                cdn_count += 1;
            }
            (_, 0) => {
                let _ = varint(&mut input)?;
            }
            (_, 1) => skip(&mut input, 8)?,
            (_, 2) => {
                let _ = take_bytes(&mut input)?;
            }
            (_, 5) => skip(&mut input, 4)?,
            (_, wire) => {
                return Err(AppError::Other(format!(
                    "unsupported storage protobuf wire type {wire}"
                )))
            }
        }
    }
    Ok((result, cdn_count))
}

fn storage_result(value: u64) -> &'static str {
    match value {
        0 => "cdn",
        1 => "storage",
        3 => "restricted",
        _ => "unknown",
    }
}

fn varint(bytes: &mut &[u8]) -> AppResult<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let Some((&byte, rest)) = bytes.split_first() else {
            return Err(AppError::Other("truncated storage protobuf".to_string()));
        };
        *bytes = rest;
        if shift == 63 && byte > 1 {
            return Err(AppError::Other("storage protobuf overflow".to_string()));
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(AppError::Other("storage protobuf overflow".to_string()))
}

fn take_bytes<'a>(bytes: &mut &'a [u8]) -> AppResult<&'a [u8]> {
    let len = usize::try_from(varint(bytes)?)
        .map_err(|_| AppError::Other("oversized storage protobuf".to_string()))?;
    if bytes.len() < len {
        return Err(AppError::Other("truncated storage protobuf".to_string()));
    }
    let (value, rest) = bytes.split_at(len);
    *bytes = rest;
    Ok(value)
}

fn skip(bytes: &mut &[u8], len: usize) -> AppResult<()> {
    if bytes.len() < len {
        return Err(AppError::Other("truncated storage protobuf".to_string()));
    }
    *bytes = &bytes[len..];
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_mapping_matches_current_storage_route_ids() {
        assert_eq!(format_id(AudioFileFormat::OGG_VORBIS_320), 2);
        assert_eq!(format_id(AudioFileFormat::FLAC_FLAC), 16);
        assert_eq!(format_id(AudioFileFormat::FLAC_FLAC_24BIT), 22);
        let flac = describe(AudioFileFormat::FLAC_FLAC);
        assert!(flac.lossless && flac.decoder_supported);
        assert!(!flac.current_player_selectable);
        assert_eq!(flac.bit_depth, Some(16));
    }

    #[test]
    fn ranks_lossless_before_lossy_without_claiming_playability() {
        assert!(
            format_rank(AudioFileFormat::FLAC_FLAC_24BIT) > format_rank(AudioFileFormat::FLAC_FLAC)
        );
        assert!(
            format_rank(AudioFileFormat::FLAC_FLAC) > format_rank(AudioFileFormat::OGG_VORBIS_320)
        );
        assert!(!describe(AudioFileFormat::FLAC_FLAC_24BIT).decoder_supported);
    }

    #[test]
    fn parses_storage_result_without_exposing_urls() {
        let url = b"https://audio-fa.scdn.co/file";
        let mut payload = vec![0x08, 0x00, 0x12, url.len() as u8];
        payload.extend_from_slice(url);
        let fallback = b"http://fallback";
        payload.extend_from_slice(&[0x12, fallback.len() as u8]);
        payload.extend_from_slice(fallback);
        let (result, urls) = parse_storage_response(&payload).unwrap();
        assert_eq!(storage_result(result), "cdn");
        assert_eq!(urls, 2);
        assert!(parse_storage_response(&[0x12, 0xff]).is_err());
    }

    #[test]
    fn validates_track_uri_for_capability_queries() {
        assert!(validate_track_uri("spotify:track:4uLU6hMCjMI75M1A2tKUQC").is_ok());
        assert!(validate_track_uri("spotify:album:4uLU6hMCjMI75M1A2tKUQC").is_err());
    }
}
