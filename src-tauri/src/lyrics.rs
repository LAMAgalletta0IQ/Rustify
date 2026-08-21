use serde::{Deserialize, Serialize};

use librespot::core::{session::Session, SpotifyUri};
use librespot::metadata::{lyrics::SyncType as SpotifySyncType, Lyrics as SpotifyLyrics};

use crate::error::{AppError, AppResult};

const ENDPOINT: &str = "https://lrclib.net/api/get";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsResult {
    pub provider: String,
    /// `available`, `instrumental`, or `unavailable`.
    pub status: &'static str,
    /// `lineSynced`, `unsynced`, or absent when no provider returned lyrics.
    pub sync_type: Option<&'static str>,
    pub language: Option<String>,
    pub is_rtl: bool,
    pub colors: Option<LyricsColors>,
    pub plain: Option<String>,
    pub synced: Vec<LyricsLine>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LyricsColors {
    /// Signed ARGB values used by Spotify's own lyrics surface.
    pub background: i32,
    pub text: i32,
    pub highlight_text: i32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LyricsLine {
    pub start_ms: u32,
    pub end_ms: Option<u32>,
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibResponse {
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

pub async fn fetch(
    session: &Session,
    track_uri: &str,
    track_name: &str,
    artist_name: &str,
    album_name: &str,
    duration_ms: u32,
) -> AppResult<LyricsResult> {
    match fetch_spotify(session, track_uri).await {
        Ok(Some(result)) if result.status != "unavailable" => return Ok(result),
        Ok(_) => log::debug!(
            target: "spotify.lyrics",
            "Spotify lyrics unavailable for this playable; trying LRCLIB"
        ),
        Err(error) => log::debug!(
            target: "spotify.lyrics",
            "Spotify color-lyrics request failed safely; trying LRCLIB: {error}"
        ),
    }

    fetch_lrclib(track_name, artist_name, album_name, duration_ms).await
}

async fn fetch_spotify(session: &Session, track_uri: &str) -> AppResult<Option<LyricsResult>> {
    let uri = SpotifyUri::from_uri(track_uri)
        .map_err(|error| AppError::BadRequest(format!("invalid track URI: {error}")))?;
    let SpotifyUri::Track { id } = uri else {
        return Ok(None);
    };

    let lyrics = SpotifyLyrics::get(session, &id)
        .await
        .map_err(|error| AppError::WebApi(format!("Spotify lyrics request failed: {error}")))?;
    Ok(Some(from_spotify(lyrics)))
}

fn from_spotify(response: SpotifyLyrics) -> LyricsResult {
    let is_line_synced = response.lyrics.sync_type == SpotifySyncType::LineSynced;
    let mut synced = Vec::with_capacity(response.lyrics.lines.len());
    let mut plain_lines = Vec::with_capacity(response.lyrics.lines.len());

    for line in response.lyrics.lines {
        let text = line.words.trim().to_string();
        plain_lines.push(text.clone());
        if is_line_synced {
            let Ok(start_ms) = line.start_time_ms.parse::<u64>() else {
                continue;
            };
            let end_ms = line
                .end_time_ms
                .parse::<u64>()
                .ok()
                .map(|value| value.min(u32::MAX as u64) as u32);
            synced.push(LyricsLine {
                start_ms: start_ms.min(u32::MAX as u64) as u32,
                end_ms,
                text,
            });
        }
    }
    synced.sort_by_key(|line| line.start_ms);

    let plain = (!plain_lines.is_empty())
        .then(|| plain_lines.join("\n"))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    let instrumental = plain_lines.iter().all(|line| {
        let line = line.trim();
        line.is_empty() || matches!(line, "♪" | "♫" | "♬")
    });
    let status = if instrumental && !plain_lines.is_empty() {
        "instrumental"
    } else if plain.is_some() || !synced.is_empty() {
        "available"
    } else {
        "unavailable"
    };

    LyricsResult {
        provider: if response.lyrics.provider_display_name.trim().is_empty() {
            "Spotify".to_string()
        } else {
            response.lyrics.provider_display_name
        },
        status,
        sync_type: Some(if is_line_synced {
            "lineSynced"
        } else {
            "unsynced"
        }),
        language: (!response.lyrics.language.trim().is_empty()).then_some(response.lyrics.language),
        is_rtl: response.lyrics.is_rtl_language,
        colors: Some(LyricsColors {
            background: response.colors.background,
            text: response.colors.text,
            highlight_text: response.colors.highlight_text,
        }),
        plain,
        synced,
    }
}

async fn fetch_lrclib(
    track_name: &str,
    artist_name: &str,
    album_name: &str,
    duration_ms: u32,
) -> AppResult<LyricsResult> {
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "Rustify/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/LAMAgalletta0IQ/Rustify)"
        ))
        .timeout(std::time::Duration::from_secs(12))
        .build()?;
    let duration = ((duration_ms as f64) / 1000.0).round() as u32;
    let response = client
        .get(ENDPOINT)
        .query(&[
            ("track_name", track_name),
            ("artist_name", artist_name),
            ("album_name", album_name),
            ("duration", &duration.to_string()),
        ])
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(LyricsResult {
            provider: "LRCLIB".to_string(),
            status: "unavailable",
            sync_type: None,
            language: None,
            is_rtl: false,
            colors: None,
            plain: None,
            synced: vec![],
        });
    }
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        return Err(AppError::RateLimited { retry_after });
    }
    if response.status().is_server_error() {
        return Err(AppError::ServiceUnavailable {
            status: response.status().as_u16(),
        });
    }
    if !response.status().is_success() {
        return Err(AppError::WebApi(format!(
            "lyrics provider returned {}",
            response.status()
        )));
    }

    let body: LrclibResponse = response.json().await?;
    if body.instrumental {
        return Ok(LyricsResult {
            provider: "LRCLIB".to_string(),
            status: "instrumental",
            sync_type: None,
            language: None,
            is_rtl: false,
            colors: None,
            plain: None,
            synced: vec![],
        });
    }

    let synced = body
        .synced_lyrics
        .as_deref()
        .map(parse_lrc)
        .unwrap_or_default();
    let plain = body
        .plain_lyrics
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let status = if plain.is_some() || !synced.is_empty() {
        "available"
    } else {
        "unavailable"
    };
    Ok(LyricsResult {
        provider: "LRCLIB".to_string(),
        status,
        sync_type: Some(if synced.is_empty() {
            "unsynced"
        } else {
            "lineSynced"
        }),
        language: None,
        is_rtl: false,
        colors: None,
        plain,
        synced,
    })
}

fn parse_lrc(input: &str) -> Vec<LyricsLine> {
    let mut result = Vec::new();
    for raw in input.lines() {
        let mut rest = raw.trim();
        let mut timestamps = Vec::new();
        while let Some(after_open) = rest.strip_prefix('[') {
            let Some(close) = after_open.find(']') else {
                break;
            };
            let stamp = &after_open[..close];
            let Some((minutes, seconds)) = stamp.split_once(':') else {
                break;
            };
            let (Ok(minutes), Ok(seconds)) = (minutes.parse::<u32>(), seconds.parse::<f64>())
            else {
                break;
            };
            timestamps.push(
                minutes
                    .saturating_mul(60_000)
                    .saturating_add((seconds * 1000.0).round().clamp(0.0, u32::MAX as f64) as u32),
            );
            rest = &after_open[close + 1..];
        }
        let text = rest.trim().to_string();
        for start_ms in timestamps {
            result.push(LyricsLine {
                start_ms,
                end_ms: None,
                text: text.clone(),
            });
        }
    }
    result.sort_by_key(|line| line.start_ms);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_sorts_lrc_with_multiple_timestamps() {
        let parsed = parse_lrc("[ar:Artist]\n[00:12.50]Second\n[00:01.250][00:03.00]First");
        assert_eq!(
            parsed,
            vec![
                LyricsLine {
                    start_ms: 1250,
                    end_ms: None,
                    text: "First".into()
                },
                LyricsLine {
                    start_ms: 3000,
                    end_ms: None,
                    text: "First".into()
                },
                LyricsLine {
                    start_ms: 12500,
                    end_ms: None,
                    text: "Second".into()
                },
            ]
        );
    }

    #[test]
    fn maps_spotify_line_sync_language_colors_and_safe_times() {
        let raw = r#"{
            "colors":{"background":-16777216,"highlightText":-1,"text":-8355712},
            "hasVocalRemoval":false,
            "lyrics":{
                "isDenseTypeface":false,"isRtlLanguage":true,"language":"ar",
                "lines":[
                    {"startTimeMs":"2200","endTimeMs":"3100","words":"second"},
                    {"startTimeMs":"1000","endTimeMs":"bad","words":"first"},
                    {"startTimeMs":"bad","endTimeMs":"0","words":"ignored synced timestamp"}
                ],
                "provider":"musixmatch","providerDisplayName":"Musixmatch",
                "providerLyricsId":"id","syncLyricsUri":"spotify:lyrics:id",
                "syncType":"LINE_SYNCED"
            }
        }"#;
        let mapped = from_spotify(serde_json::from_str(raw).unwrap());

        assert_eq!(mapped.provider, "Musixmatch");
        assert_eq!(mapped.sync_type, Some("lineSynced"));
        assert_eq!(mapped.language.as_deref(), Some("ar"));
        assert!(mapped.is_rtl);
        assert_eq!(mapped.colors.as_ref().unwrap().highlight_text, -1);
        assert_eq!(mapped.synced[0].start_ms, 1000);
        assert_eq!(mapped.synced[0].end_ms, None);
        assert_eq!(mapped.synced[1].end_ms, Some(3100));
    }
}
