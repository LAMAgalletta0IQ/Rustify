use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreditContributor {
    pub name: String,
    pub artist_uri: Option<String>,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreditGroup {
    pub role_name: String,
    pub contributors: Vec<CreditContributor>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrackCredits {
    pub track_name: Option<String>,
    pub record_label: Option<String>,
    pub groups: Vec<CreditGroup>,
}

pub fn variables(track_uri: &str) -> AppResult<Value> {
    let Some(id) = track_uri.trim().strip_prefix("spotify:track:") else {
        return Err(AppError::BadRequest(
            "credits require a Spotify track URI".to_string(),
        ));
    };
    if id.len() != 22 || !id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(AppError::BadRequest(
            "invalid Spotify track URI".to_string(),
        ));
    }
    Ok(json!({
        "trackUri": track_uri.trim(),
        "contributorsLimit": 100,
        "contributorsOffset": 0
    }))
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn role_order(role: &str) -> u8 {
    match role {
        "Artist" => 0,
        "Composition & Lyrics" => 1,
        "Production & Engineering" => 2,
        "Performers" => 3,
        _ => 4,
    }
}

pub fn parse(data: &Value) -> AppResult<TrackCredits> {
    let track = data.get("trackUnion").ok_or_else(|| {
        AppError::Unavailable("track credits were absent from Pathfinder".to_string())
    })?;
    let track_name = text(track.get("name"));
    let record_label = track
        .pointer("/creditsTrait/sources/items")
        .and_then(Value::as_array)
        .and_then(|items| items.iter().find_map(|item| text(item.get("name"))));

    // Indexes point into `groups`, preserving Spotify's first-seen order while
    // merging repeat composer/lyricist rows for the same person in that group.
    let mut groups: Vec<CreditGroup> = Vec::new();
    let mut group_indexes: HashMap<String, usize> = HashMap::new();
    let mut contributor_indexes: HashMap<(String, String, Option<String>), usize> = HashMap::new();
    let contributors = track
        .pointer("/creditsTrait/contributors/items")
        .and_then(Value::as_array);
    for item in contributors.into_iter().flatten() {
        let Some(name) = text(item.get("name")) else {
            continue;
        };
        let group_name =
            text(item.pointer("/roleGroup/name")).unwrap_or_else(|| "Other".to_string());
        let artist_uri = text(item.get("uri")).filter(|uri| uri.starts_with("spotify:artist:"));
        let role = text(item.get("role"));
        let group_index = *group_indexes.entry(group_name.clone()).or_insert_with(|| {
            groups.push(CreditGroup {
                role_name: group_name.clone(),
                contributors: Vec::new(),
            });
            groups.len() - 1
        });
        let key = (group_name, name.to_ascii_lowercase(), artist_uri.clone());
        if let Some(contributor_index) = contributor_indexes.get(&key).copied() {
            if let Some(role) = role {
                let roles = &mut groups[group_index].contributors[contributor_index].roles;
                if !roles.contains(&role) {
                    roles.push(role);
                }
            }
            continue;
        }
        let mut roles = Vec::new();
        if let Some(role) = role {
            roles.push(role);
        }
        let contributor_index = groups[group_index].contributors.len();
        groups[group_index].contributors.push(CreditContributor {
            name,
            artist_uri,
            roles,
        });
        contributor_indexes.insert(key, contributor_index);
    }
    groups.sort_by_key(|group| role_order(&group.role_name));

    Ok(TrackCredits {
        track_name,
        record_label,
        groups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_track_uri() {
        let uri = "spotify:track:0123456789ABCDEFGHIJKL";
        assert_eq!(variables(uri).unwrap()["trackUri"], uri);
        assert!(variables("spotify:episode:0123456789ABCDEFGHIJKL").is_err());
        assert!(variables("spotify:track:../../path").is_err());
    }

    #[test]
    fn groups_deduplicates_and_orders_credits() {
        let data = json!({"trackUnion":{
          "name":"A Song",
          "creditsTrait":{
            "sources":{"items":[{"name":"Example Records"}]},
            "contributors":{"items":[
              {"name":"Alex","role":"Producer","roleGroup":{"name":"Production & Engineering"}},
              {"name":"Sam","uri":"spotify:artist:abc","role":"Composer","roleGroup":{"name":"Composition & Lyrics"}},
              {"name":"Sam","uri":"spotify:artist:abc","role":"Lyricist","roleGroup":{"name":"Composition & Lyrics"}},
              {"name":"Singer","uri":"https://unsafe.invalid","role":"Main Artist","roleGroup":{"name":"Artist"}}
            ]}
          }
        }});
        let credits = parse(&data).unwrap();
        assert_eq!(credits.track_name.as_deref(), Some("A Song"));
        assert_eq!(credits.record_label.as_deref(), Some("Example Records"));
        assert_eq!(credits.groups[0].role_name, "Artist");
        assert_eq!(credits.groups[1].role_name, "Composition & Lyrics");
        assert_eq!(credits.groups[1].contributors.len(), 1);
        assert_eq!(
            credits.groups[1].contributors[0].roles,
            ["Composer", "Lyricist"]
        );
        assert_eq!(credits.groups[0].contributors[0].artist_uri, None);
    }

    #[test]
    fn missing_trait_is_a_valid_empty_credit_set() {
        assert!(parse(&json!({"trackUnion":{"name":"Unknown"}}))
            .unwrap()
            .groups
            .is_empty());
        assert!(parse(&json!({})).is_err());
    }
}
