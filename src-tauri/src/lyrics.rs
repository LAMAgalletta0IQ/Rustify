use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const ENDPOINT: &str = "https://lrclib.net/api/get";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsResult {
    pub provider: &'static str,
    /// `available`, `instrumental`, or `unavailable`.
    pub status: &'static str,
    pub plain: Option<String>,
    pub synced: Vec<LyricsLine>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LyricsLine {
    pub start_ms: u32,
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
            provider: "LRCLIB",
            status: "unavailable",
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
            provider: "LRCLIB",
            status: "instrumental",
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
        provider: "LRCLIB",
        status,
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
                    text: "First".into()
                },
                LyricsLine {
                    start_ms: 3000,
                    text: "First".into()
                },
                LyricsLine {
                    start_ms: 12500,
                    text: "Second".into()
                },
            ]
        );
    }
}
