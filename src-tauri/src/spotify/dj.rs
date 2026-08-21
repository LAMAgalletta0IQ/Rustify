use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::error::{AppError, AppResult};

const DEFAULT_DJ_CONTEXT: &str = "spotify:playlist:37i9dQZF1EYkqdzj48dyYq";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DjSession {
    pub available: bool,
    pub context_uri: String,
    pub context_url: Option<String>,
    pub tracks: Vec<DjTrack>,
    pub metadata: BTreeMap<String, String>,
    pub interactivity_enabled: bool,
    pub jump_button_label: Option<String>,
    pub volatile_context_id: Option<String>,
    pub lexicon_current_time: Option<String>,
    pub lexicon_expiration_time: Option<String>,
    pub reason: String,
    pub active: bool,
    pub narration_resolved: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DjTrack {
    pub uri: String,
    pub canonical_uri: Option<String>,
    pub source: Option<String>,
    pub station_uri: Option<String>,
    pub narration_kinds: Vec<String>,
    pub narration_image_url: Option<String>,
    #[serde(skip_serializing)]
    pub metadata: BTreeMap<String, String>,
}

pub struct DjClient {
    http: Client,
    tts_http: Client,
    cached: RwLock<Option<DjSession>>,
}

impl Default for DjClient {
    fn default() -> Self {
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .unwrap_or_else(|_| Client::new()),
            tts_http: Client::builder()
                .timeout(Duration::from_secs(12))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_else(|_| Client::new()),
            cached: RwLock::new(None),
        }
    }
}

impl DjClient {
    pub async fn cached(&self) -> Option<DjSession> {
        self.cached.read().await.clone()
    }

    pub async fn resolve(
        &self,
        base_url: String,
        reason: &str,
        access_token: &str,
        client_token: &str,
        connection_id: &str,
    ) -> AppResult<DjSession> {
        let context_uri = std::env::var("RUSTIFY_DJ_CONTEXT_URI")
            .ok()
            .filter(|uri| uri.starts_with("spotify:"))
            .unwrap_or_else(|| DEFAULT_DJ_CONTEXT.to_owned());
        let endpoint = format!(
            "{}/lexicon-session-provider/context-resolve/v2/session",
            base_url.trim_end_matches('/')
        );
        let mut request = self
            .http
            .get(endpoint)
            .query(&[("contextUri", context_uri.as_str()), ("reason", reason)])
            .bearer_auth(access_token)
            .header("client-token", client_token)
            .header("app-platform", "WebPlayer")
            .header("Accept", "application/json");
        if !connection_id.is_empty() {
            request = request.header("Spotify-Connection-Id", connection_id);
        }
        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await?;
        log::debug!(target: "spotify.lexicon", "DJ resolve ({reason}) -> {status} ({} bytes)", body.len());
        if status == StatusCode::UNAUTHORIZED {
            return Err(AppError::SessionExpired);
        }
        if status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND {
            return Err(AppError::LexiconUnavailable(
                "Spotify DJ is not enabled for this account or region".into(),
            ));
        }
        if !status.is_success() {
            return Err(AppError::ServiceUnavailable {
                status: status.as_u16(),
            });
        }
        let value: Value = serde_json::from_str(&body)
            .map_err(|error| AppError::WebApi(format!("invalid Lexicon JSON: {error}")))?;
        let session = parse_session(&value, &context_uri, reason)?;
        *self.cached.write().await = Some(session.clone());
        Ok(session)
    }

    pub async fn update_status(&self, active: bool, narration_resolved: bool) {
        if let Some(session) = self.cached.write().await.as_mut() {
            session.active = active;
            session.narration_resolved = narration_resolved;
        }
    }

    /// Resolves the first narration clip to its short-lived signed audio URL.
    /// The URL never leaves this backend method and is not cached or logged.
    /// Audio injection consumes this same seam in the hardening pass.
    pub async fn prepare_first_narration(
        &self,
        base_url: String,
        session: &DjSession,
        access_token: &str,
        client_token: &str,
        connection_id: &str,
    ) -> AppResult<bool> {
        let Some((track, kind)) = session.tracks.iter().find_map(|track| {
            track
                .narration_kinds
                .first()
                .map(|kind| (track, kind.as_str()))
        }) else {
            return Ok(false);
        };
        let prefix = format!("narration.{kind}");
        let Some(ssml) = track.metadata.get(&format!("{prefix}.ssml")) else {
            return Ok(false);
        };
        let body = tts_request(
            ssml,
            track.metadata.get(&format!("{prefix}.language")),
            track.metadata.get(&format!("{prefix}.voice")),
            track.metadata.get(&format!("{prefix}.tts_provider")),
            track.metadata.get(&format!("{prefix}.sample_rate")),
        );
        let endpoint = format!("{}/client-tts/v1/fulfill", base_url.trim_end_matches('/'));
        let mut request = self
            .tts_http
            .post(endpoint)
            .bearer_auth(access_token)
            .header("client-token", client_token)
            .header("Content-Type", "application/x-protobuf")
            .body(body);
        if !connection_id.is_empty() {
            request = request.header("Spotify-Connection-Id", connection_id);
        }
        let response = request.send().await?;
        let status = response.status();
        if status != StatusCode::FOUND && status != StatusCode::SEE_OTHER {
            return Err(AppError::LexiconUnavailable(format!(
                "narration fulfillment returned HTTP {status}"
            )));
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .filter(|value| value.starts_with("https://"))
            .ok_or_else(|| {
                AppError::LexiconUnavailable(
                    "narration fulfillment returned no secure audio location".into(),
                )
            })?;
        log::debug!(target: "spotify.dj", "resolved {kind} narration for {} (signed URL redacted, {} chars)", track.uri, location.len());
        Ok(true)
    }
}

fn parse_session(value: &Value, fallback_uri: &str, reason: &str) -> AppResult<DjSession> {
    let metadata = string_map(value.get("metadata"));
    let mut tracks = Vec::new();
    for track in value
        .get("pages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|page| {
            page.get("tracks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
    {
        if let Some(track) = parse_track(track) {
            tracks.push(track);
        }
    }
    if tracks.is_empty() {
        return Err(AppError::LexiconUnavailable(
            "Lexicon returned no playable DJ tracks".into(),
        ));
    }
    Ok(DjSession {
        available: true,
        context_uri: text(value.get("uri")).unwrap_or_else(|| fallback_uri.to_owned()),
        context_url: text(value.get("url")),
        interactivity_enabled: metadata
            .get("dj.interactivity_enabled")
            .is_some_and(|value| value == "true"),
        jump_button_label: metadata
            .get("dj.interactivity.localized_jump_button")
            .cloned(),
        volatile_context_id: metadata.get("playlist_volatile_context_id").cloned(),
        lexicon_current_time: metadata.get("lexicon_current_time").cloned(),
        lexicon_expiration_time: metadata.get("lexicon_expiration_time").cloned(),
        metadata,
        tracks,
        reason: reason.to_owned(),
        active: false,
        narration_resolved: false,
    })
}

fn parse_track(value: &Value) -> Option<DjTrack> {
    let metadata = string_map(value.get("metadata"));
    let raw_uri = text(value.get("uri"))?;
    if raw_uri.is_empty() || raw_uri == "spotify:delimiter" {
        return None;
    }
    let canonical_uri = metadata.get("canonical_track_uri").cloned();
    let mut uri = canonical_uri.clone().unwrap_or(raw_uri);
    if let Some(id) = uri.strip_prefix("spotify:media:") {
        uri = format!("spotify:track:{id}");
    }
    if !uri.starts_with("spotify:track:") && !uri.starts_with("spotify:episode:") {
        return None;
    }
    let narration_kinds = ["intro", "jump", "outro"]
        .into_iter()
        .filter(|kind| {
            metadata.contains_key(&format!("narration.{kind}.ssml"))
                || metadata.contains_key(&format!("narration.{kind}.commentary_id"))
        })
        .map(str::to_owned)
        .collect();
    let narration_image_url = ["intro", "jump", "outro"]
        .into_iter()
        .find_map(|kind| metadata.get(&format!("narration.{kind}.image")).cloned());
    Some(DjTrack {
        uri,
        canonical_uri,
        source: metadata.get("source.components").cloned(),
        station_uri: metadata.get("station_uri").cloned(),
        narration_kinds,
        narration_image_url,
        metadata,
    })
}

fn string_map(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| Some((key.clone(), text(Some(value))?)))
        .collect()
}

fn text(value: Option<&Value>) -> Option<String> {
    let value = value?;
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_i64().map(|number| number.to_string()))
        .or_else(|| value.as_bool().map(|boolean| boolean.to_string()))
}

fn tts_request(
    ssml: &str,
    language: Option<&String>,
    voice: Option<&String>,
    provider: Option<&String>,
    sample_rate: Option<&String>,
) -> Vec<u8> {
    let mut body = Vec::new();
    protobuf_string(&mut body, 2, ssml);
    protobuf_varint(&mut body, 3, 5); // ResolveRequest.AudioFormat.MP3
    if let Some(language) = language.filter(|value| !value.is_empty()) {
        protobuf_string(&mut body, 4, language);
    }
    let voice = voice
        .and_then(|value| value.strip_prefix("VOICE"))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1..=40).contains(value))
        .unwrap_or(1);
    protobuf_varint(&mut body, 5, voice);
    let provider = match provider.map(String::as_str) {
        Some("CLOUD_TTS") => 1,
        Some("READSPEAKER") => 2,
        Some("POLLY") => 3,
        Some("WELL_SAID") => 4,
        Some("SONANTIC_DEPRECATED") => 5,
        _ => 6, // SONANTIC_FAST
    };
    protobuf_varint(&mut body, 6, provider);
    let sample_rate = sample_rate
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (8_000..=96_000).contains(value))
        .unwrap_or(44_100);
    protobuf_varint(&mut body, 7, sample_rate);
    body
}

fn protobuf_string(output: &mut Vec<u8>, field: u64, value: &str) {
    push_varint(output, field << 3 | 2);
    push_varint(output, value.len() as u64);
    output.extend_from_slice(value.as_bytes());
}

fn protobuf_varint(output: &mut Vec<u8>, field: u64, value: u64) {
    push_varint(output, field << 3);
    push_varint(output, value);
}

fn push_varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_dynamic_tracks_narration_and_session_metadata() {
        let value = json!({
            "uri": DEFAULT_DJ_CONTEXT,
            "metadata": {
                "dj.interactivity_enabled": "true",
                "playlist_volatile_context_id": "volatile-1",
                "lexicon_current_time": "123"
            },
            "pages": [{"tracks": [
                {"uri": "spotify:delimiter"},
                {"uri": "spotify:media:abc", "metadata": {
                    "canonical_track_uri": "spotify:track:abc",
                    "source.components": "YourDJ,Music",
                    "narration.intro.ssml": "<speak>Hello</speak>",
                    "narration.intro.voice": "VOICE1"
                }}
            ]}]
        });
        let session = parse_session(&value, DEFAULT_DJ_CONTEXT, "interactive").unwrap();
        assert_eq!(session.tracks.len(), 1);
        assert_eq!(session.tracks[0].uri, "spotify:track:abc");
        assert_eq!(session.tracks[0].narration_kinds, ["intro"]);
        assert!(session.interactivity_enabled);
    }

    #[test]
    fn rejects_empty_dynamic_session() {
        let value = json!({"pages": [{"tracks": [{"uri": "spotify:delimiter"}]}]});
        assert!(matches!(
            parse_session(&value, DEFAULT_DJ_CONTEXT, "interactive"),
            Err(AppError::LexiconUnavailable(_))
        ));
    }

    #[test]
    fn encodes_client_tts_request_without_generated_proto_dependency() {
        let body = tts_request(
            "<speak>Hi</speak>",
            Some(&"en-US".to_owned()),
            Some(&"VOICE12".to_owned()),
            Some(&"POLLY".to_owned()),
            Some(&"44100".to_owned()),
        );
        assert!(body
            .windows(17)
            .any(|window| window == b"<speak>Hi</speak>"));
        assert!(body.windows(5).any(|window| window == b"en-US"));
        assert!(body.ends_with(&[0x38, 0xc4, 0xd8, 0x02]));
    }
}
