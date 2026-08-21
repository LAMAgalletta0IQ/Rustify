//! Spotify-native music-video discovery and protected-playback boundary.
//!
//! Discovery is ordinary authenticated metadata: an audio track's
//! VIDEO_ASSOCIATIONS extension points to a video catalog track, whose
//! `original_video.gid` is the v9 manifest id. Actual playback is PlayReady
//! protected and is intentionally not attempted by Rustify's audio decoder.

use librespot::{core::SpotifyUri, protocol::extension_kind::ExtensionKind};
use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MusicVideoImage {
    pub url: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub size: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MusicVideoCapability {
    pub audio_uri: String,
    pub available: bool,
    pub video_uri: Option<String>,
    pub manifest_id: Option<String>,
    pub images: Vec<MusicVideoImage>,
    pub open_url: Option<String>,
    pub playback_supported: bool,
    pub playback_blocker: Option<String>,
}

pub async fn capability(
    session: &librespot::core::Session,
    audio_uri: &str,
) -> AppResult<MusicVideoCapability> {
    let uri = validate_track_uri(audio_uri)?;
    let response = match session
        .spclient()
        .get_metadata(ExtensionKind::VIDEO_ASSOCIATIONS, &uri)
        .await
    {
        Ok(bytes) => bytes,
        // An absent extension is a normal, negative capability result.
        Err(error) => {
            log::debug!(target: "spotify.video", "video association unavailable for {audio_uri}: {error}");
            return Ok(unavailable(audio_uri));
        }
    };

    let associations = parse_associations(&response)?;
    let Some(association) = associations
        .into_iter()
        .find(|item| validate_track_uri(&item.video_uri).is_ok() && item.video_uri != audio_uri)
    else {
        return Ok(unavailable(audio_uri));
    };

    let video_uri = validate_track_uri(&association.video_uri)?;
    let manifest_id = match session.spclient().get_track_metadata(&video_uri).await {
        Ok(bytes) => parse_original_video_gid(&bytes)?,
        Err(error) => {
            log::debug!(target: "spotify.video", "video track metadata unavailable for {}: {error}", association.video_uri);
            None
        }
    };
    let id = association
        .video_uri
        .strip_prefix("spotify:track:")
        .ok_or_else(|| AppError::BadRequest("invalid associated video URI".into()))?;
    let open_url = format!("https://open.spotify.com/track/{id}");

    Ok(MusicVideoCapability {
        audio_uri: audio_uri.to_string(),
        available: true,
        video_uri: Some(association.video_uri),
        manifest_id,
        images: association.images,
        open_url: Some(open_url),
        playback_supported: false,
        playback_blocker: Some(
            "Spotify music-video manifests use a PlayReady-protected EME pipeline; Rustify exposes metadata and capability only and does not bypass protected media."
                .to_string(),
        ),
    })
}

fn unavailable(audio_uri: &str) -> MusicVideoCapability {
    MusicVideoCapability {
        audio_uri: audio_uri.to_string(),
        available: false,
        video_uri: None,
        manifest_id: None,
        images: Vec::new(),
        open_url: None,
        playback_supported: false,
        playback_blocker: None,
    }
}

fn validate_track_uri(value: &str) -> AppResult<SpotifyUri> {
    let uri = SpotifyUri::from_uri(value)
        .map_err(|_| AppError::BadRequest("expected a Spotify track URI".into()))?;
    if uri.item_type() != "track" || uri.to_uri() != value {
        return Err(AppError::BadRequest(
            "expected a canonical Spotify track URI".into(),
        ));
    }
    Ok(uri)
}

#[derive(Debug)]
struct Association {
    video_uri: String,
    images: Vec<MusicVideoImage>,
}

fn parse_associations(bytes: &[u8]) -> AppResult<Vec<Association>> {
    let mut result = Vec::new();
    for field in fields(bytes)? {
        let Field::Bytes(1, association) = field else {
            continue;
        };
        let mut video_uri = None;
        let mut images = Vec::new();
        for field in fields(association)? {
            match field {
                Field::Bytes(1, value) => {
                    video_uri = std::str::from_utf8(value).ok().map(str::to_string)
                }
                Field::Bytes(2, group) => images.extend(parse_images(group)?),
                _ => {}
            }
        }
        if let Some(video_uri) = video_uri {
            result.push(Association { video_uri, images });
        }
    }
    Ok(result)
}

fn parse_images(bytes: &[u8]) -> AppResult<Vec<MusicVideoImage>> {
    let mut images = Vec::new();
    for field in fields(bytes)? {
        let Field::Bytes(1, image) = field else {
            continue;
        };
        let mut file_id = None;
        let mut size = 0;
        let mut width = None;
        let mut height = None;
        for field in fields(image)? {
            match field {
                Field::Bytes(1, value) if !value.is_empty() => file_id = Some(hex(value)),
                Field::Varint(2, value) => size = value,
                Field::Varint(3, value) => width = Some(zigzag32(value)),
                Field::Varint(4, value) => height = Some(zigzag32(value)),
                _ => {}
            }
        }
        if let Some(file_id) = file_id {
            images.push(MusicVideoImage {
                url: format!("https://i.scdn.co/image/{file_id}"),
                width,
                height,
                size: match size {
                    1 => "small",
                    2 => "large",
                    3 => "xlarge",
                    4 => "xxlarge",
                    _ => "default",
                }
                .to_string(),
            });
        }
    }
    Ok(images)
}

fn parse_original_video_gid(bytes: &[u8]) -> AppResult<Option<String>> {
    for field in fields(bytes)? {
        let Field::Bytes(38, video) = field else {
            continue;
        };
        for field in fields(video)? {
            if let Field::Bytes(1, gid) = field {
                if !gid.is_empty() {
                    return Ok(Some(hex(gid)));
                }
            }
        }
    }
    Ok(None)
}

#[derive(Debug, PartialEq, Eq)]
enum Field<'a> {
    Varint(u32, u64),
    Bytes(u32, &'a [u8]),
}

fn fields(mut bytes: &[u8]) -> AppResult<Vec<Field<'_>>> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        let key = varint(&mut bytes)?;
        let number = u32::try_from(key >> 3)
            .map_err(|_| AppError::Other("protobuf field number overflow".into()))?;
        if number == 0 {
            return Err(AppError::Other("invalid protobuf field zero".into()));
        }
        match key & 7 {
            0 => result.push(Field::Varint(number, varint(&mut bytes)?)),
            1 => skip(&mut bytes, 8)?,
            2 => {
                let len = usize::try_from(varint(&mut bytes)?)
                    .map_err(|_| AppError::Other("protobuf length overflow".into()))?;
                if bytes.len() < len {
                    return Err(AppError::Other("truncated protobuf field".into()));
                }
                let (value, rest) = bytes.split_at(len);
                bytes = rest;
                result.push(Field::Bytes(number, value));
            }
            5 => skip(&mut bytes, 4)?,
            wire => {
                return Err(AppError::Other(format!(
                    "unsupported protobuf wire type {wire}"
                )))
            }
        }
    }
    Ok(result)
}

fn varint(bytes: &mut &[u8]) -> AppResult<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let Some((&byte, rest)) = bytes.split_first() else {
            return Err(AppError::Other("truncated protobuf varint".into()));
        };
        *bytes = rest;
        if shift == 63 && byte > 1 {
            return Err(AppError::Other("protobuf varint overflow".into()));
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(AppError::Other("protobuf varint overflow".into()))
}

fn skip(bytes: &mut &[u8], len: usize) -> AppResult<()> {
    if bytes.len() < len {
        return Err(AppError::Other("truncated protobuf field".into()));
    }
    *bytes = &bytes[len..];
    Ok(())
}

fn zigzag32(value: u64) -> i32 {
    let value = value as u32;
    ((value >> 1) as i32) ^ -((value & 1) as i32)
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0xf)]));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(field: u32, wire: u8, out: &mut Vec<u8>) {
        encode_varint(u64::from(field) << 3 | u64::from(wire), out);
    }

    fn encode_varint(mut value: u64, out: &mut Vec<u8>) {
        while value >= 0x80 {
            out.push((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    fn bytes_field(field: u32, value: &[u8], out: &mut Vec<u8>) {
        key(field, 2, out);
        encode_varint(value.len() as u64, out);
        out.extend_from_slice(value);
    }

    #[test]
    fn validates_only_canonical_track_uris() {
        assert!(validate_track_uri("spotify:track:4uLU6hMCjMI75M1A2tKUQC").is_ok());
        assert!(validate_track_uri("spotify:episode:4uLU6hMCjMI75M1A2tKUQC").is_err());
        assert!(validate_track_uri("spotify:track:bad/path").is_err());
    }

    #[test]
    fn parses_association_images_and_dimensions() {
        let mut image = Vec::new();
        bytes_field(1, &[0xab, 0xcd], &mut image);
        key(2, 0, &mut image);
        encode_varint(2, &mut image);
        key(3, 0, &mut image);
        encode_varint(1280 << 1, &mut image);
        key(4, 0, &mut image);
        encode_varint(720 << 1, &mut image);
        let mut group = Vec::new();
        bytes_field(1, &image, &mut group);
        let mut association = Vec::new();
        bytes_field(1, b"spotify:track:4uLU6hMCjMI75M1A2tKUQC", &mut association);
        bytes_field(2, &group, &mut association);
        let mut root = Vec::new();
        bytes_field(1, &association, &mut root);

        let parsed = parse_associations(&root).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].images[0].url, "https://i.scdn.co/image/abcd");
        assert_eq!(parsed[0].images[0].width, Some(1280));
        assert_eq!(parsed[0].images[0].height, Some(720));
        assert_eq!(parsed[0].images[0].size, "large");
    }

    #[test]
    fn extracts_original_video_manifest_and_rejects_truncation() {
        let mut video = Vec::new();
        bytes_field(1, &[1, 2, 0xfe], &mut video);
        let mut track = Vec::new();
        bytes_field(38, &video, &mut track);
        assert_eq!(
            parse_original_video_gid(&track).unwrap(),
            Some("0102fe".into())
        );
        assert!(parse_original_video_gid(&[0xf2, 0x02, 0xff]).is_err());
    }
}
