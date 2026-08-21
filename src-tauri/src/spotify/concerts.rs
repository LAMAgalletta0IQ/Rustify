use std::collections::HashSet;

use serde::Serialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConcertEvent {
    pub uri: String,
    pub title: String,
    pub start_date_iso: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub is_festival: bool,
    pub event_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConcertFeed {
    pub available: bool,
    pub total_count: u64,
    pub events: Vec<ConcertEvent>,
}

pub fn variables(artist_id: &str, locale: &str) -> AppResult<Value> {
    if artist_id.len() != 22 || !artist_id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(AppError::BadRequest(
            "invalid Spotify artist ID".to_string(),
        ));
    }
    let locale = locale.trim();
    let locale = if locale.len() <= 32
        && locale
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        locale
    } else {
        ""
    };
    Ok(json!({
        "uri": format!("spotify:artist:{artist_id}"),
        "locale": locale,
        "preReleaseV2": false
    }))
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn event_url(uri: &str) -> Option<String> {
    let id = uri.strip_prefix("spotify:concert:")?;
    (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .then(|| format!("https://open.spotify.com/concert/{id}"))
}

pub fn parse(data: &Value) -> AppResult<ConcertFeed> {
    let artist = data.get("artistUnion").ok_or_else(|| {
        AppError::Unavailable("artist concert data was absent from Pathfinder".to_string())
    })?;
    let concerts = artist.pointer("/goods/concerts");
    let total_count = concerts
        .and_then(|value| value.get("totalCount"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut seen = HashSet::new();
    let mut events: Vec<ConcertEvent> = concerts
        .and_then(|value| value.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("data"))
        .filter_map(|event| {
            let uri = text(event.get("uri"))?;
            if !seen.insert(uri.clone()) {
                return None;
            }
            Some(ConcertEvent {
                title: text(event.get("title")).unwrap_or_else(|| "Live event".to_string()),
                start_date_iso: text(event.get("startDateIsoString")),
                venue: text(event.pointer("/location/name")),
                city: text(event.pointer("/location/city")),
                is_festival: event
                    .get("festival")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                event_url: event_url(&uri),
                uri,
            })
        })
        .collect();
    events.sort_by(
        |left, right| match (&left.start_date_iso, &right.start_date_iso) {
            (Some(left), Some(right)) => left.cmp(right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left.title.cmp(&right.title),
        },
    );

    Ok(ConcertFeed {
        available: true,
        total_count: total_count.max(events.len() as u64),
        events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_variables_and_locale() {
        let id = "0123456789ABCDEFGHIJKL";
        assert_eq!(variables(id, "it-IT").unwrap()["locale"], "it-IT");
        assert_eq!(variables(id, "it/../../x").unwrap()["locale"], "");
        assert!(variables("not-an-id", "en").is_err());
    }

    #[test]
    fn parses_sorted_deduplicated_artist_concerts() {
        let data = json!({"artistUnion":{"goods":{"concerts":{"totalCount":3,"items":[
          {"data":{"uri":"spotify:concert:b2","title":"Later","startDateIsoString":"2027-02-02T20:00:00Z","festival":false,"location":{"name":"Arena","city":"Rome"}}},
          {"data":{"uri":"spotify:concert:a1","title":"Festival","startDateIsoString":"2027-01-01T19:00:00+01:00","festival":true,"location":{"name":"Park","city":"Milan"}}},
          {"data":{"uri":"spotify:concert:a1","title":"Duplicate"}}
        ]}}}});
        let feed = parse(&data).unwrap();
        assert_eq!(feed.total_count, 3);
        assert_eq!(feed.events.len(), 2);
        assert_eq!(feed.events[0].title, "Festival");
        assert_eq!(feed.events[0].venue.as_deref(), Some("Park"));
        assert!(feed.events[0].is_festival);
        assert_eq!(
            feed.events[0].event_url.as_deref(),
            Some("https://open.spotify.com/concert/a1")
        );
    }

    #[test]
    fn missing_goods_is_a_supported_empty_feed() {
        assert_eq!(
            parse(&json!({"artistUnion":{"uri":"spotify:artist:x"}}))
                .unwrap()
                .events,
            Vec::new()
        );
        assert!(parse(&json!({})).is_err());
    }
}
