use serde::Serialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

const MAX_QUERY_CHARS: usize = 200;
const MAX_FIELD_CHARS: usize = 2_048;
const MAX_USERS: usize = 30;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserSearchHit {
    pub uri: String,
    pub username: String,
    pub display_name: String,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserSearchPage {
    pub users: Vec<UserSearchHit>,
    pub total: u32,
    pub next_offset: Option<u32>,
}

pub fn variables(query: &str, limit: u32, offset: u32) -> AppResult<Value> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::BadRequest("enter a name to search for".into()));
    }
    if query.chars().count() > MAX_QUERY_CHARS || query.chars().any(char::is_control) {
        return Err(AppError::BadRequest(
            "user search must be 200 characters or fewer".into(),
        ));
    }
    Ok(json!({
        "searchTerm": query,
        "offset": offset,
        "limit": limit.clamp(1, 30),
        "numberOfTopResults": 20,
        "includeAudiobooks": true,
        "includeAuthors": true,
        "includePreReleases": false,
        "includeEpisodeContentRatingsV2": false
    }))
}

fn string(value: Option<&Value>) -> Option<String> {
    value?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(MAX_FIELD_CHARS).collect())
}

fn image_url(data: &Value) -> Option<String> {
    data.pointer("/avatar/sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|source| {
            Some((
                source.get("width").and_then(Value::as_u64).unwrap_or(0),
                safe_image_url(string(source.get("url"))?)?,
            ))
        })
        .min_by_key(|(width, _)| width.abs_diff(300))
        .map(|(_, url)| url)
}

fn safe_image_url(value: String) -> Option<String> {
    reqwest::Url::parse(&value)
        .ok()
        .filter(|url| url.scheme() == "https" && url.host_str().is_some())
        .map(|_| value)
}

fn username_from_uri(uri: &str) -> Option<String> {
    uri.strip_prefix("spotify:user:")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.len() <= 256 && !value.chars().any(char::is_control))
        .map(str::to_owned)
}

pub fn parse(data: &Value) -> AppResult<UserSearchPage> {
    let users = data
        .pointer("/searchV2/users")
        .ok_or_else(|| AppError::Unavailable("Spotify user search is unavailable".into()))?;
    let items = users
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut parsed = Vec::with_capacity(items.len());
    for item in items.into_iter().take(MAX_USERS) {
        // Filtered search wraps each entity as { data: UserResponseWrapper }.
        // Be tolerant of an unwrapped fixture/response because Spotify has
        // changed this envelope independently from the operation schema.
        let entity = item.get("data").unwrap_or(&item);
        let uri = match string(entity.get("uri")) {
            Some(uri) if uri.starts_with("spotify:user:") => uri,
            _ => continue,
        };
        let username = string(entity.get("username"))
            .or_else(|| username_from_uri(&uri))
            .unwrap_or_default();
        if username.is_empty() {
            continue;
        }
        let display_name = string(entity.get("displayName"))
            .or_else(|| string(entity.get("name")))
            .unwrap_or_else(|| username.clone());
        parsed.push(UserSearchHit {
            uri,
            username,
            display_name,
            image_url: image_url(entity),
        });
    }
    parsed.sort_by(|left, right| left.username.cmp(&right.username));
    parsed.dedup_by(|left, right| left.username == right.username);

    let total = users
        .get("totalCount")
        .and_then(Value::as_u64)
        .unwrap_or(parsed.len() as u64)
        .min(u32::MAX as u64) as u32;
    let next_offset = users
        .pointer("/pagingInfo/nextOffset")
        .and_then(Value::as_u64)
        .filter(|next| *next > 0 && *next < total as u64)
        .map(|next| next.min(u32::MAX as u64) as u32);

    Ok(UserSearchPage {
        users: parsed,
        total,
        next_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_bounds_search_variables() {
        assert!(variables("  ", 20, 0).is_err());
        let value = variables(" Alice ", 99, 4).unwrap();
        assert_eq!(value["searchTerm"], "Alice");
        assert_eq!(value["limit"], 30);
        assert_eq!(value["offset"], 4);
        assert!(variables(&"x".repeat(MAX_QUERY_CHARS + 1), 10, 0).is_err());
    }

    #[test]
    fn parses_wrapped_users_images_and_paging() {
        let data = json!({"searchV2":{"users":{
            "totalCount": 3,
            "pagingInfo":{"limit":2,"nextOffset":2},
            "items":[
                {"data":{"uri":"spotify:user:b","username":"b","displayName":"Bee","avatar":{"sources":[{"url":"https://i.scdn.co/wide","width":640},{"url":"https://i.scdn.co/near","width":320}]}}},
                {"data":{"uri":"spotify:user:a","name":"Aye","avatar":{"sources":[]}}},
                {"data":{"uri":"spotify:artist:nope","username":"nope"}}
            ]
        }}});
        let page = parse(&data).unwrap();
        assert_eq!(page.users.len(), 2);
        assert_eq!(page.users[0].username, "a");
        assert_eq!(
            page.users[1].image_url.as_deref(),
            Some("https://i.scdn.co/near")
        );
        assert_eq!(page.total, 3);
        assert_eq!(page.next_offset, Some(2));
    }
}
