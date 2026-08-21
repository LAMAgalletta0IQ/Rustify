//! Spotify podcast progress through the Herodotus resumption platform.
//!
//! Bodies are ordinary protobuf messages (not gRPC-framed). This module keeps
//! a deliberately small wire implementation for the nine fields it consumes,
//! avoiding a second generated protocol tree while still preserving unknown
//! fields for forward compatibility by skipping them safely.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use http::{header::CONTENT_TYPE, HeaderMap, HeaderValue, Method};
use librespot::core::{session::Session, SpotifyUri};
use serde::Serialize;

use crate::error::{AppError, AppResult};

const LIST_PATH: &str = "/herodotus/spotify.resumption.v1.CurrentStateService/ListCurrentStates";
const CREATE_PATH: &str =
    "/herodotus/spotify.resumption.v1.ResumePointRevisionService/CreateResumePointRevision";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeResume {
    pub uri: String,
    pub position_ms: u64,
    pub completed: bool,
    pub has_state: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Point {
    Position(u64),
    Finished,
    Other,
}

#[derive(Debug, Clone, Copy)]
enum Wire<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

fn episode_uri(value: &str) -> AppResult<String> {
    let uri = SpotifyUri::from_uri(value)
        .map_err(|error| AppError::BadRequest(format!("invalid Spotify URI: {error}")))?;
    if !matches!(uri, SpotifyUri::Episode { .. }) {
        return Err(AppError::BadRequest(
            "podcast progress requires a Spotify episode URI".to_string(),
        ));
    }
    Ok(uri.to_uri())
}

fn push_varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn protobuf_varint(output: &mut Vec<u8>, field: u32, value: u64) {
    push_varint(output, u64::from(field) << 3);
    push_varint(output, value);
}

fn protobuf_bytes(output: &mut Vec<u8>, field: u32, value: &[u8]) {
    push_varint(output, u64::from(field) << 3 | 2);
    push_varint(output, value.len() as u64);
    output.extend_from_slice(value);
}

fn protobuf_string(output: &mut Vec<u8>, field: u32, value: &str) {
    protobuf_bytes(output, field, value.as_bytes());
}

fn take_varint(input: &mut &[u8]) -> AppResult<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let Some((&byte, rest)) = input.split_first() else {
            return Err(AppError::Other("truncated resumption protobuf".to_string()));
        };
        *input = rest;
        if shift == 63 && byte > 1 {
            return Err(AppError::Other("invalid resumption varint".to_string()));
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(AppError::Other("invalid resumption varint".to_string()))
}

fn fields(mut input: &[u8]) -> AppResult<Vec<(u32, Wire<'_>)>> {
    let mut fields = Vec::new();
    while !input.is_empty() {
        let key = take_varint(&mut input)?;
        let number = u32::try_from(key >> 3)
            .map_err(|_| AppError::Other("invalid resumption field number".to_string()))?;
        if number == 0 {
            return Err(AppError::Other("zero resumption field number".to_string()));
        }
        let value = match key & 7 {
            0 => Wire::Varint(take_varint(&mut input)?),
            1 => {
                if input.len() < 8 {
                    return Err(AppError::Other("truncated fixed64 field".to_string()));
                }
                input = &input[8..];
                Wire::Fixed
            }
            2 => {
                let length = usize::try_from(take_varint(&mut input)?)
                    .map_err(|_| AppError::Other("oversized protobuf field".to_string()))?;
                if input.len() < length {
                    return Err(AppError::Other("truncated protobuf field".to_string()));
                }
                let (value, rest) = input.split_at(length);
                input = rest;
                Wire::Bytes(value)
            }
            5 => {
                if input.len() < 4 {
                    return Err(AppError::Other("truncated fixed32 field".to_string()));
                }
                input = &input[4..];
                Wire::Fixed
            }
            _ => {
                return Err(AppError::Other(
                    "unsupported protobuf wire type".to_string(),
                ))
            }
        };
        fields.push((number, value));
    }
    Ok(fields)
}

fn duration_ms(input: &[u8]) -> AppResult<u64> {
    let mut seconds = 0u64;
    let mut nanos = 0u64;
    for (number, value) in fields(input)? {
        match (number, value) {
            (1, Wire::Varint(value)) => seconds = value,
            (2, Wire::Varint(value)) => nanos = value,
            _ => {}
        }
    }
    Ok(seconds
        .saturating_mul(1_000)
        .saturating_add(nanos / 1_000_000))
}

fn timestamp(input: &[u8]) -> AppResult<(u64, u32)> {
    let mut seconds = 0;
    let mut nanos = 0;
    for (number, value) in fields(input)? {
        match (number, value) {
            (1, Wire::Varint(value)) => seconds = value,
            (2, Wire::Varint(value)) => nanos = value.min(u64::from(u32::MAX)) as u32,
            _ => {}
        }
    }
    Ok((seconds, nanos))
}

fn snapshot(input: &[u8]) -> AppResult<Option<Point>> {
    let mut point = None;
    for (number, value) in fields(input)? {
        match (number, value) {
            (2, Wire::Bytes(value)) => point = Some(Point::Position(duration_ms(value)?)),
            (4 | 8, Wire::Bytes(_)) => point = Some(Point::Finished),
            (3 | 5 | 6 | 7 | 9, _) => point = Some(Point::Other),
            (10, Wire::Bytes(value)) => {
                point = fields(value)?.into_iter().find_map(|(field, wire)| {
                    (field == 1).then_some(wire).and_then(|wire| match wire {
                        Wire::Bytes(duration) => duration_ms(duration).ok().map(Point::Position),
                        _ => None,
                    })
                });
            }
            _ => {}
        }
    }
    Ok(point)
}

fn revision(input: &[u8]) -> AppResult<Option<((u64, u32), Point)>> {
    let mut point = None;
    let mut created = (0, 0);
    let mut updated = None;
    for (number, value) in fields(input)? {
        match (number, value) {
            (2, Wire::Bytes(value)) => point = snapshot(value)?,
            (3, Wire::Bytes(value)) => created = timestamp(value)?,
            (4, Wire::Bytes(value)) => updated = Some(timestamp(value)?),
            _ => {}
        }
    }
    Ok(point.map(|point| (updated.unwrap_or(created), point)))
}

fn parse_current_state(input: &[u8], expected_uri: &str) -> AppResult<Option<Point>> {
    let mut uri = None;
    let mut revisions = Vec::new();
    for (number, value) in fields(input)? {
        match (number, value) {
            (1, Wire::Bytes(value)) => uri = std::str::from_utf8(value).ok(),
            (2, Wire::Bytes(value)) => {
                if let Some(revision) = revision(value)? {
                    revisions.push(revision);
                }
            }
            _ => {}
        }
    }
    if uri != Some(expected_uri) {
        return Ok(None);
    }
    Ok(revisions
        .into_iter()
        .max_by_key(|(updated_at, _)| *updated_at)
        .map(|(_, point)| point))
}

fn parse_list_response(input: &[u8], expected_uri: &str) -> AppResult<Option<Point>> {
    for (number, value) in fields(input)? {
        if let (1, Wire::Bytes(value)) = (number, value) {
            if let Some(point) = parse_current_state(value, expected_uri)? {
                return Ok(Some(point));
            }
        }
    }
    Ok(None)
}

fn protobuf_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/x-protobuf"),
    );
    headers
}

async fn post(session: &Session, path: &str, body: &[u8]) -> AppResult<Vec<u8>> {
    let request =
        session
            .spclient()
            .request(&Method::POST, path, Some(protobuf_headers()), Some(body));
    let bytes = tokio::time::timeout(REQUEST_TIMEOUT, request)
        .await
        .map_err(|_| AppError::ServiceUnavailable { status: 408 })?
        .map_err(|error| AppError::WebApi(format!("podcast resumption request failed: {error}")))?;
    Ok(bytes.to_vec())
}

pub async fn get(session: &Session, value: &str) -> AppResult<EpisodeResume> {
    let uri = episode_uri(value)?;
    let mut body = Vec::new();
    protobuf_varint(&mut body, 2, 1);
    protobuf_string(&mut body, 4, &format!("cs.uri == \"{uri}\""));
    let response = post(session, LIST_PATH, &body).await?;
    let point = parse_list_response(&response, &uri)?;
    Ok(EpisodeResume {
        uri,
        position_ms: match point {
            Some(Point::Position(position)) => position,
            _ => 0,
        },
        completed: point == Some(Point::Finished),
        has_state: point.is_some(),
    })
}

fn create_body(uri: &str, point_field: u32, position_ms: u64) -> Vec<u8> {
    let mut point = Vec::new();
    if point_field == 2 {
        let mut duration = Vec::new();
        protobuf_varint(&mut duration, 1, position_ms / 1_000);
        protobuf_bytes(&mut point, 2, &duration);
    } else {
        protobuf_bytes(&mut point, point_field, &[]);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let mut timestamp = Vec::new();
    protobuf_varint(&mut timestamp, 1, now.as_secs());
    protobuf_varint(&mut timestamp, 2, u64::from(now.subsec_nanos()));
    let mut revision = Vec::new();
    protobuf_bytes(&mut revision, 2, &point);
    protobuf_bytes(&mut revision, 3, &timestamp);
    let mut request = Vec::new();
    protobuf_string(&mut request, 2, uri);
    protobuf_bytes(&mut request, 4, &revision);
    request
}

pub async fn set_position(session: &Session, value: &str, position_ms: u64) -> AppResult<()> {
    let uri = episode_uri(value)?;
    if position_ms == 0 {
        return Ok(());
    }
    // Spotify's own clients write whole-second revisions.
    let body = create_body(&uri, 2, position_ms / 1_000 * 1_000);
    post(session, CREATE_PATH, &body).await?;
    Ok(())
}

pub async fn set_completed(session: &Session, value: &str, completed: bool) -> AppResult<()> {
    let uri = episode_uri(value)?;
    // `finished` is field 4; explicit user unmark is field 9.
    let body = create_body(&uri, if completed { 4 } else { 9 }, 0);
    post(session, CREATE_PATH, &body).await?;
    Ok(())
}

pub fn spawn_report(session: Session, uri: String, position_ms: u64, completed: bool) {
    tauri::async_runtime::spawn(async move {
        let result = if completed {
            set_completed(&session, &uri, true).await
        } else {
            set_position(&session, &uri, position_ms).await
        };
        if let Err(error) = result {
            log::debug!(target: "spotify.podcasts", "resume report failed for {uri}: {error}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested(field: u32, body: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        protobuf_bytes(&mut result, field, body);
        result
    }

    #[test]
    fn validates_episode_uris() {
        let uri = "spotify:episode:0Jv8TUEkzMplSPfX3ynBXu";
        assert_eq!(episode_uri(uri).unwrap(), uri);
        assert!(episode_uri("spotify:track:0Jv8TUEkzMplSPfX3ynBXu").is_err());
    }

    #[test]
    fn newest_revision_wins_and_finished_clears_position() {
        let uri = "spotify:episode:0Jv8TUEkzMplSPfX3ynBXu";
        let revision_at = |seconds: u64, field: u32, value_ms: u64| {
            let point = create_body(uri, field, value_ms);
            // Pull the already-correct revision out of request field 4, then
            // replace its create timestamp to make ordering deterministic.
            let mut revision = match fields(&point).unwrap()[1].1 {
                Wire::Bytes(value) => value.to_vec(),
                _ => unreachable!(),
            };
            let mut time = Vec::new();
            protobuf_varint(&mut time, 1, seconds);
            protobuf_bytes(&mut revision, 4, &time);
            revision
        };
        let mut state = Vec::new();
        protobuf_string(&mut state, 1, uri);
        protobuf_bytes(&mut state, 2, &revision_at(10, 2, 325_000));
        protobuf_bytes(&mut state, 2, &revision_at(20, 4, 0));
        let response = nested(1, &state);
        assert_eq!(
            parse_list_response(&response, uri).unwrap(),
            Some(Point::Finished)
        );
    }

    #[test]
    fn parses_position_and_relative_position() {
        let mut duration = Vec::new();
        protobuf_varint(&mut duration, 1, 90);
        protobuf_varint(&mut duration, 2, 500_000_000);
        assert_eq!(duration_ms(&duration).unwrap(), 90_500);
        let relative = nested(1, &duration);
        let snapshot = nested(10, &relative);
        assert_eq!(
            super::snapshot(&snapshot).unwrap(),
            Some(Point::Position(90_500))
        );
    }

    #[test]
    fn create_request_has_uri_revision_and_expected_point() {
        let uri = "spotify:episode:0Jv8TUEkzMplSPfX3ynBXu";
        let body = create_body(uri, 2, 125_999);
        let parsed = fields(&body).unwrap();
        assert!(matches!(parsed[0], (2, Wire::Bytes(value)) if value == uri.as_bytes()));
        let revision = match parsed[1].1 {
            Wire::Bytes(value) => fields(value).unwrap(),
            _ => unreachable!(),
        };
        let snapshot = match revision[0].1 {
            Wire::Bytes(value) => fields(value).unwrap(),
            _ => unreachable!(),
        };
        assert!(
            matches!(snapshot[0], (2, Wire::Bytes(value)) if duration_ms(value).unwrap() == 125_000)
        );
    }

    #[test]
    fn rejects_truncated_and_unsupported_wire_data() {
        assert!(fields(&[0x0a, 0x05, 0x01]).is_err());
        assert!(fields(&[0x0b]).is_err());
    }
}
