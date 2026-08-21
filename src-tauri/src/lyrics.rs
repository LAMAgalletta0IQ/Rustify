use serde::{Deserialize, Serialize};

use librespot::core::{session::Session, SpotifyUri};

use crate::error::{AppError, AppResult};

const ENDPOINT: &str = "https://lrclib.net/api/get";
const MAX_LYRICS_BYTES: u64 = 1024 * 1024;
const MAX_LINES: usize = 5_000;
const MAX_LINE_CHARS: usize = 10_000;
const MAX_LRC_MINUTES: u32 = 24 * 60;

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

/// Deliberately local and forward-compatible. librespot's public metadata
/// model uses an enum for `syncType`, so a newly introduced Spotify value
/// would reject the entire otherwise usable response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotifyLyricsResponse {
    colors: Option<SpotifyColors>,
    lyrics: SpotifyLyricsBody,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotifyColors {
    background: i32,
    text: i32,
    highlight_text: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotifyLyricsBody {
    #[serde(default)]
    lines: Vec<SpotifyLine>,
    #[serde(default)]
    provider_display_name: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    is_rtl_language: bool,
    #[serde(default)]
    sync_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotifyLine {
    #[serde(default)]
    start_time_ms: String,
    #[serde(default)]
    end_time_ms: String,
    #[serde(default)]
    words: String,
}

pub async fn fetch(
    session: &Session,
    track_uri: &str,
    track_name: &str,
    artist_name: &str,
    album_name: &str,
    duration_ms: u32,
) -> AppResult<LyricsResult> {
    validate_track_uri(track_uri)?;
    match fetch_spotify(session, track_uri, duration_ms).await {
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

pub(crate) fn validate_track_uri(track_uri: &str) -> AppResult<()> {
    let uri = SpotifyUri::from_uri(track_uri)
        .map_err(|error| AppError::BadRequest(format!("invalid track URI: {error}")))?;
    if !matches!(uri, SpotifyUri::Track { .. }) {
        return Err(AppError::BadRequest(
            "lyrics require a canonical Spotify track URI".to_string(),
        ));
    }
    Ok(())
}

async fn fetch_spotify(
    session: &Session,
    track_uri: &str,
    duration_ms: u32,
) -> AppResult<Option<LyricsResult>> {
    let SpotifyUri::Track { id } = SpotifyUri::from_uri(track_uri)
        .map_err(|error| AppError::BadRequest(format!("invalid track URI: {error}")))?
    else {
        return Ok(None);
    };

    let bytes = session
        .spclient()
        .get_lyrics(&id)
        .await
        .map_err(|error| AppError::WebApi(format!("Spotify lyrics request failed: {error}")))?;
    if bytes.len() as u64 > MAX_LYRICS_BYTES {
        return Err(AppError::WebApi(
            "Spotify lyrics response exceeded 1 MiB".to_string(),
        ));
    }
    let lyrics = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::WebApi(format!("invalid Spotify lyrics response: {error}")))?;
    Ok(Some(from_spotify(lyrics, duration_ms)))
}

fn from_spotify(response: SpotifyLyricsResponse, duration_ms: u32) -> LyricsResult {
    let is_line_synced = matches!(
        response.lyrics.sync_type.as_str(),
        "LINE_SYNCED" | "SYLLABLE_SYNCED"
    );
    let mut synced = Vec::with_capacity(response.lyrics.lines.len());
    let mut plain_lines = Vec::with_capacity(response.lyrics.lines.len());

    for line in response.lyrics.lines.into_iter().take(MAX_LINES) {
        let text = bounded_text(&line.words);
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
    normalize_end_times(&mut synced, duration_ms);

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
        sync_type: (status != "unavailable").then_some(if is_line_synced {
            "lineSynced"
        } else {
            "unsynced"
        }),
        language: (!response.lyrics.language.trim().is_empty()).then_some(response.lyrics.language),
        is_rtl: response.lyrics.is_rtl_language,
        colors: response.colors.map(|colors| LyricsColors {
            background: colors.background,
            text: colors.text,
            highlight_text: colors.highlight_text,
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

    if response
        .content_length()
        .is_some_and(|size| size > MAX_LYRICS_BYTES)
    {
        return Err(AppError::WebApi(
            "lyrics provider response exceeded 1 MiB".to_string(),
        ));
    }

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

    let bytes = response.bytes().await?;
    if bytes.len() as u64 > MAX_LYRICS_BYTES {
        return Err(AppError::WebApi(
            "lyrics provider response exceeded 1 MiB".to_string(),
        ));
    }
    let body: LrclibResponse = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::WebApi(format!("invalid lyrics response: {error}")))?;
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

    let mut synced = body
        .synced_lyrics
        .as_deref()
        .map(parse_lrc)
        .unwrap_or_default();
    normalize_end_times(&mut synced, duration_ms);
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
            let normalized_seconds = seconds.replace(':', ".");
            let (Ok(minutes), Ok(seconds)) =
                (minutes.parse::<u32>(), normalized_seconds.parse::<f64>())
            else {
                break;
            };
            if minutes > MAX_LRC_MINUTES || !seconds.is_finite() || !(0.0..60.0).contains(&seconds)
            {
                break;
            }
            timestamps.push(
                minutes
                    .saturating_mul(60_000)
                    .saturating_add((seconds * 1000.0).round().clamp(0.0, u32::MAX as f64) as u32),
            );
            rest = &after_open[close + 1..];
        }
        let text = bounded_text(rest);
        for start_ms in timestamps {
            if result.len() >= MAX_LINES {
                return result;
            }
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

fn bounded_text(value: &str) -> String {
    value.trim().chars().take(MAX_LINE_CHARS).collect()
}

/// Providers frequently omit end times or encode them as `"0"`. Preserve a
/// valid explicit vocal end (so the UI can show a genuine pause), otherwise
/// derive the boundary from the next distinct line or the track duration.
fn normalize_end_times(lines: &mut [LyricsLine], duration_ms: u32) {
    for index in 0..lines.len() {
        let start = lines[index].start_ms;
        let next_start = lines[index + 1..]
            .iter()
            .map(|line| line.start_ms)
            .find(|candidate| *candidate > start);
        let upper_bound = next_start.or((duration_ms > start).then_some(duration_ms));
        let explicit = lines[index]
            .end_ms
            .filter(|end| *end > start)
            .map(|end| upper_bound.map_or(end, |upper| end.min(upper)));
        lines[index].end_ms = explicit.or(upper_bound);
    }
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
        let mapped = from_spotify(serde_json::from_str(raw).unwrap(), 4_000);

        assert_eq!(mapped.provider, "Musixmatch");
        assert_eq!(mapped.sync_type, Some("lineSynced"));
        assert_eq!(mapped.language.as_deref(), Some("ar"));
        assert!(mapped.is_rtl);
        assert_eq!(mapped.colors.as_ref().unwrap().highlight_text, -1);
        assert_eq!(mapped.synced[0].start_ms, 1000);
        assert_eq!(mapped.synced[0].end_ms, Some(2_200));
        assert_eq!(mapped.synced[1].end_ms, Some(3100));
    }

    #[test]
    fn rejects_non_finite_and_out_of_range_lrc_timestamps() {
        let parsed = parse_lrc(
            "[00:NaN]bad\n[00:60.00]also bad\n[99999:01]too large\n[00:01.25]good\n[00:01:50]valid variant",
        );
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].start_ms, 1_250);
        assert_eq!(parsed[1].start_ms, 1_500);
    }

    #[test]
    fn derives_missing_ends_but_preserves_real_vocal_pauses() {
        let mut lines = vec![
            LyricsLine {
                start_ms: 1_000,
                end_ms: Some(2_000),
                text: "short vocal".into(),
            },
            LyricsLine {
                start_ms: 10_000,
                end_ms: Some(0),
                text: "next".into(),
            },
        ];
        normalize_end_times(&mut lines, 20_000);
        assert_eq!(lines[0].end_ms, Some(2_000));
        assert_eq!(lines[1].end_ms, Some(20_000));
    }

    #[test]
    fn accepts_new_synchronized_variant_and_optional_presentation_fields() {
        let raw = r#"{
            "lyrics": {
                "syncType": "SYLLABLE_SYNCED",
                "lines": [{"startTimeMs":"500","words":"future mode"}]
            }
        }"#;
        let mapped = from_spotify(serde_json::from_str(raw).unwrap(), 2_000);
        assert_eq!(mapped.status, "available");
        assert_eq!(mapped.sync_type, Some("lineSynced"));
        assert_eq!(mapped.provider, "Spotify");
        assert_eq!(mapped.colors, None);
        assert_eq!(mapped.synced[0].end_ms, Some(2_000));
    }
}
