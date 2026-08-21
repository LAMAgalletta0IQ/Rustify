use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HomeFeed {
    pub greeting: Option<String>,
    pub sections: Vec<HomeSection>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HomeSection {
    pub uri: String,
    pub title: Option<String>,
    pub type_name: String,
    pub total_count: u64,
    pub items: Vec<HomeItem>,
}

/// A deliberately forward-compatible Home card. Spotify adds wrappers often;
/// known media types get normalized fields while the original typename and
/// attributes remain available for capability detection without matching a
/// localized title such as "Made for you".
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HomeItem {
    pub kind: String,
    pub uri: String,
    pub id: String,
    pub name: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub type_name: String,
    pub format: Option<String>,
    pub personalization: Option<HomePersonalization>,
    pub owner_name: Option<String>,
    pub made_for_username: Option<String>,
    pub total_count: Option<u64>,
    pub attributes: BTreeMap<String, String>,
}

/// Stable, non-localized classification derived from Spotify's format and
/// attribute fields. Names and section titles are intentionally not used.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HomePersonalization {
    DailyMix,
    DiscoverWeekly,
    ReleaseRadar,
    Daylist,
    ArtistMix,
    TopicMix,
    InspiredByMix,
    MadeForYou,
}

pub fn home_variables(limit: u32, time_zone: &str) -> Value {
    let time_zone = if time_zone.trim().is_empty() {
        "UTC"
    } else {
        time_zone
    };
    json!({
        "homeEndUserIntegration": "INTEGRATION_WEB_PLAYER",
        "timeZone": time_zone,
        "sp_t": "",
        "facet": "",
        "sectionItemsLimit": limit,
        "includeEpisodeContentRatingsV2": false
    })
}

pub fn parse_home(data: &Value) -> AppResult<HomeFeed> {
    let home = data.get("home").ok_or_else(|| {
        AppError::EndpointNotAvailable(
            "Pathfinder returned no data.home object for this account".into(),
        )
    })?;
    let greeting = label(home.get("greeting"));
    let sections = home
        .pointer("/sectionContainer/sections/items")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_section).collect())
        .unwrap_or_default();
    Ok(HomeFeed { greeting, sections })
}

fn parse_section(section: &Value) -> Option<HomeSection> {
    let section_items = section.get("sectionItems")?;
    let mut seen = HashSet::new();
    let items: Vec<_> = section_items
        .get("items")?
        .as_array()?
        .iter()
        .filter_map(parse_item)
        .filter(|item| seen.insert(item.uri.clone()))
        .collect();
    if items.is_empty() {
        return None;
    }
    let data = section.get("data").unwrap_or(&Value::Null);
    Some(HomeSection {
        uri: string(section.get("uri")).unwrap_or_default(),
        title: label(data.get("title")),
        type_name: string(data.get("__typename")).unwrap_or_default(),
        total_count: section_items
            .get("totalCount")
            .and_then(Value::as_u64)
            .unwrap_or(items.len() as u64),
        items,
    })
}

fn parse_item(item: &Value) -> Option<HomeItem> {
    let content = item.get("content")?;
    let type_name = string(content.get("__typename")).unwrap_or_default();
    let data = content.get("data")?;
    let uri = string(data.get("uri")).or_else(|| string(item.get("uri")))?;
    let attributes = attributes(data.get("attributes"));
    let made_for_username = attributes.get("madeFor.username").cloned();
    let format = string(data.get("format"));
    let personalization = personalization(format.as_deref(), made_for_username.is_some());
    let kind = wrapper_kind(&type_name, data.get("__typename"));
    let artists = data
        .pointer("/artists/items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|artist| label(artist.get("profile")))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.is_empty());
    let owner_name = data
        .pointer("/ownerV2/data")
        .and_then(|owner| string(owner.get("name")).or_else(|| label(owner.get("profile"))));
    let subtitle = artists
        .or_else(|| owner_name.clone())
        .or_else(|| string(data.get("publisher")))
        .or_else(|| string(data.get("type")));
    let name = string(data.get("name"))
        .or_else(|| label(data.get("profile")))
        .or_else(|| label(data.get("title")))
        .unwrap_or_else(|| uri.rsplit(':').next().unwrap_or(&uri).to_owned());
    Some(HomeItem {
        kind,
        id: uri.rsplit(':').next().unwrap_or(&uri).to_owned(),
        uri,
        name,
        subtitle,
        description: string(data.get("description")),
        image_url: image(data),
        type_name,
        format,
        personalization,
        owner_name,
        made_for_username,
        total_count: data.pointer("/content/totalCount").and_then(Value::as_u64),
        attributes,
    })
}

fn wrapper_kind(wrapper: &str, data_type: Option<&Value>) -> String {
    let haystack =
        format!("{} {}", wrapper, string(data_type).unwrap_or_default()).to_ascii_lowercase();
    for kind in [
        "playlist",
        "album",
        "artist",
        "episode",
        "podcast",
        "show",
        "audiobook",
        "track",
    ] {
        if haystack.contains(kind) {
            return kind.to_owned();
        }
    }
    "unknown".to_owned()
}

fn image(data: &Value) -> Option<String> {
    [
        "/images/items/0/sources/0/url",
        "/coverArt/sources/0/url",
        "/visuals/avatarImage/sources/0/url",
        "/coverImage/sources/0/url",
    ]
    .iter()
    .find_map(|pointer| data.pointer(pointer).and_then(Value::as_str))
    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
    .map(str::to_owned)
}

fn personalization(format: Option<&str>, made_for_user: bool) -> Option<HomePersonalization> {
    let semantic = match format.unwrap_or_default().to_ascii_lowercase().as_str() {
        "daily-mix" => Some(HomePersonalization::DailyMix),
        "discover-weekly" => Some(HomePersonalization::DiscoverWeekly),
        "release-radar" => Some(HomePersonalization::ReleaseRadar),
        "daylist" => Some(HomePersonalization::Daylist),
        "artist-mix-reader" | "artist-mix" => Some(HomePersonalization::ArtistMix),
        "topic-mix" => Some(HomePersonalization::TopicMix),
        "inspiredby-mix" | "inspired-by-mix" => Some(HomePersonalization::InspiredByMix),
        _ => None,
    };
    semantic.or(made_for_user.then_some(HomePersonalization::MadeForYou))
}

fn attributes(value: Option<&Value>) -> BTreeMap<String, String> {
    const SEMANTIC_KEYS: [&str; 4] = [
        "madeFor.username",
        "nicheMixLinks",
        "daylist_pretitle",
        "isAlgotorial",
    ];
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let key = string(entry.get("key"))?;
            if !SEMANTIC_KEYS.contains(&key.as_str()) {
                return None;
            }
            Some((key, string(entry.get("value"))?))
        })
        .collect()
}

fn label(value: Option<&Value>) -> Option<String> {
    let value = value?;
    string(Some(value)).or_else(|| {
        ["transformedLabel", "translatedBaseText", "text", "name"]
            .iter()
            .find_map(|key| string(value.get(*key)))
    })
}

fn string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_semantic_home_cards_without_title_matching() {
        let data = json!({"home": {
            "greeting": {"transformedLabel": "Good evening"},
            "sectionContainer": {"sections": {"items": [{
                "uri": "spotify:section:made-for-user",
                "data": {"__typename": "HomeGenericSectionData", "title": {"transformedLabel": "Per te"}},
                "sectionItems": {"totalCount": 1, "items": [{
                    "uri": "spotify:playlist:mix1",
                    "content": {"__typename": "PlaylistResponseWrapper", "data": {
                        "__typename": "Playlist", "uri": "spotify:playlist:mix1", "name": "Mix giornaliero 1",
                        "format": "daily-mix", "ownerV2": {"data": {"name": "Spotify"}},
                        "attributes": [
                            {"key": "madeFor.username", "value": "alice"},
                            {"key": "request-id", "value": "private-correlation-value"}
                        ],
                        "images": {"items": [{"sources": [{"url": "https://image/1"}]}]},
                        "content": {"totalCount": 50}
                    }}
                }]}
            }]}}
        }});
        let feed = parse_home(&data).unwrap();
        assert_eq!(feed.sections[0].items[0].kind, "playlist");
        assert_eq!(
            feed.sections[0].items[0].personalization,
            Some(HomePersonalization::DailyMix)
        );
        assert_eq!(
            feed.sections[0].items[0].made_for_username.as_deref(),
            Some("alice")
        );
        assert_eq!(
            feed.sections[0].items[0].image_url.as_deref(),
            Some("https://image/1")
        );
        assert!(!feed.sections[0].items[0]
            .attributes
            .contains_key("request-id"));
    }

    #[test]
    fn classifies_personalized_formats_without_localized_names() {
        let cases = [
            ("daily-mix", HomePersonalization::DailyMix),
            ("discover-weekly", HomePersonalization::DiscoverWeekly),
            ("release-radar", HomePersonalization::ReleaseRadar),
            ("daylist", HomePersonalization::Daylist),
            ("artist-mix-reader", HomePersonalization::ArtistMix),
            ("topic-mix", HomePersonalization::TopicMix),
            ("inspiredby-mix", HomePersonalization::InspiredByMix),
        ];
        for (format, expected) in cases {
            assert_eq!(personalization(Some(format), false), Some(expected));
        }
        assert_eq!(
            personalization(Some("future-personalized-format"), true),
            Some(HomePersonalization::MadeForYou)
        );
        assert_eq!(personalization(Some("editorial"), false), None);
    }

    #[test]
    fn removes_duplicate_cards_and_rejects_unsafe_artwork_urls() {
        let item = json!({
            "uri": "spotify:playlist:same",
            "content": {"__typename": "PlaylistResponseWrapper", "data": {
                "uri": "spotify:playlist:same", "name": "Un nome localizzato",
                "images": {"items": [{"sources": [{"url": "javascript:alert(1)"}]}]}
            }}
        });
        let data = json!({"home": {"sectionContainer": {"sections": {"items": [{
            "uri": "spotify:section:test",
            "data": {"title": {"text": "Qualcosa"}},
            "sectionItems": {"items": [item.clone(), item]}
        }]}}}});
        let feed = parse_home(&data).unwrap();
        assert_eq!(feed.sections[0].items.len(), 1);
        assert_eq!(feed.sections[0].items[0].image_url, None);
    }
}
