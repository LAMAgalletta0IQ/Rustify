//! Rich Spotify user profiles through the authenticated SpClient session.
//!
//! The public Web API only exposes the current account and does not provide a
//! user search surface. Spotify's own clients use `user-profile-view/v3` for
//! both the current user and profiles reached from playlists/friend activity.

use http::Method;
use librespot::core::session::Session;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileArtist {
    pub uri: String,
    pub name: String,
    pub image_url: Option<String>,
    pub followers_count: Option<u64>,
    pub is_following: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePlaylist {
    pub uri: String,
    pub name: String,
    pub image_url: Option<String>,
    pub owner_name: Option<String>,
    pub owner_uri: Option<String>,
    pub is_following: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub username: String,
    pub uri: String,
    pub display_name: String,
    pub image_url: Option<String>,
    pub following_count: Option<u64>,
    pub total_public_playlists_count: Option<u64>,
    pub is_current_user: bool,
    pub allow_follows: bool,
    pub show_follows: bool,
    /// Signed RGB/ARGB integer supplied by Spotify's profile service.
    pub color: Option<i64>,
    pub recently_played_artists: Vec<ProfileArtist>,
    pub public_playlists: Vec<ProfilePlaylist>,
    /// These can be empty because the profile owner hid follows or because
    /// the account/region does not expose the optional endpoint.
    pub followers: Vec<ProfileArtist>,
    pub following: Vec<ProfileArtist>,
}

#[derive(Debug, Deserialize)]
struct RawProfile {
    uri: Option<String>,
    name: Option<String>,
    image_url: Option<String>,
    following_count: Option<u64>,
    recently_played_artists: Option<Vec<RawArtist>>,
    public_playlists: Option<Vec<RawPlaylist>>,
    total_public_playlists_count: Option<u64>,
    is_current_user: Option<bool>,
    color: Option<i64>,
    allow_follows: Option<bool>,
    show_follows: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawProfiles {
    profiles: Option<Vec<RawArtist>>,
}

#[derive(Debug, Deserialize)]
struct RawArtist {
    uri: Option<String>,
    name: Option<String>,
    image_url: Option<String>,
    followers_count: Option<u64>,
    is_following: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawPlaylist {
    uri: Option<String>,
    name: Option<String>,
    image_url: Option<String>,
    owner_name: Option<String>,
    owner_uri: Option<String>,
    is_following: Option<bool>,
}

fn path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

pub fn normalize_username(value: &str) -> AppResult<String> {
    let value = value.trim();
    let value = value
        .strip_prefix("spotify:user:")
        .or_else(|| value.strip_prefix("https://open.spotify.com/user/"))
        .unwrap_or(value)
        .split(['?', '#', '/'])
        .next()
        .unwrap_or_default()
        .trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(AppError::BadRequest(
            "enter a Spotify username, user URI, or profile URL".to_string(),
        ));
    }
    Ok(value.to_string())
}

fn valid_artist(value: RawArtist) -> Option<ProfileArtist> {
    let uri = value.uri?.trim().to_string();
    let name = value.name?.trim().to_string();
    if uri.is_empty() || name.is_empty() {
        return None;
    }
    Some(ProfileArtist {
        uri,
        name,
        image_url: value.image_url.filter(|url| !url.trim().is_empty()),
        followers_count: value.followers_count,
        is_following: value.is_following,
    })
}

fn valid_playlist(value: RawPlaylist) -> Option<ProfilePlaylist> {
    let uri = value.uri?.trim().to_string();
    let name = value.name?.trim().to_string();
    if uri.is_empty() || name.is_empty() {
        return None;
    }
    Some(ProfilePlaylist {
        uri,
        name,
        image_url: value.image_url.filter(|url| !url.trim().is_empty()),
        owner_name: value.owner_name,
        owner_uri: value.owner_uri,
        is_following: value.is_following,
    })
}

async fn profile(session: &Session, encoded_username: &str) -> AppResult<RawProfile> {
    let endpoint = format!(
        "/user-profile-view/v3/profile/{encoded_username}?playlist_limit=20&artist_limit=20&episode_limit=0&market=from_token"
    );
    let bytes = session
        .spclient()
        .request_as_json(&Method::GET, &endpoint, None, None)
        .await
        .map_err(|error| AppError::Unavailable(format!("profile request failed: {error}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Other(format!("invalid Spotify profile response: {error}")))
}

async fn relation(
    session: &Session,
    encoded_username: &str,
    kind: &str,
) -> AppResult<Vec<ProfileArtist>> {
    let endpoint =
        format!("/user-profile-view/v3/profile/{encoded_username}/{kind}?market=from_token");
    let bytes = session
        .spclient()
        .request_as_json(&Method::GET, &endpoint, None, None)
        .await
        .map_err(|error| {
            AppError::Unavailable(format!("profile {kind} request failed: {error}"))
        })?;
    let response: RawProfiles = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Other(format!("invalid profile {kind} response: {error}")))?;
    Ok(response
        .profiles
        .unwrap_or_default()
        .into_iter()
        .filter_map(valid_artist)
        .collect())
}

pub async fn fetch(session: &Session, username_or_uri: &str) -> AppResult<UserProfile> {
    let username = normalize_username(username_or_uri)?;
    let encoded = path_segment(&username);
    let raw = profile(session, &encoded).await?;

    // Follow lists are separately permissioned. Load both concurrently, but
    // never discard the useful profile when Spotify keeps either list private.
    let (followers, following) = tokio::join!(
        relation(session, &encoded, "followers"),
        relation(session, &encoded, "following")
    );
    let followers = followers.unwrap_or_else(|error| {
        log::debug!(target: "spotify.profile", "followers unavailable for {username}: {error}");
        Vec::new()
    });
    let following = following.unwrap_or_else(|error| {
        log::debug!(target: "spotify.profile", "following unavailable for {username}: {error}");
        Vec::new()
    });

    let uri = raw
        .uri
        .filter(|uri| !uri.trim().is_empty())
        .unwrap_or_else(|| format!("spotify:user:{username}"));
    let display_name = raw
        .name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| username.clone());

    Ok(UserProfile {
        username,
        uri,
        display_name,
        image_url: raw.image_url.filter(|url| !url.trim().is_empty()),
        following_count: raw.following_count,
        total_public_playlists_count: raw.total_public_playlists_count,
        is_current_user: raw.is_current_user.unwrap_or(false),
        allow_follows: raw.allow_follows.unwrap_or(false),
        show_follows: raw.show_follows.unwrap_or(false),
        color: raw.color,
        recently_played_artists: raw
            .recently_played_artists
            .unwrap_or_default()
            .into_iter()
            .filter_map(valid_artist)
            .collect(),
        public_playlists: raw
            .public_playlists
            .unwrap_or_default()
            .into_iter()
            .filter_map(valid_playlist)
            .collect(),
        followers,
        following,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_supported_user_identifiers() {
        assert_eq!(normalize_username("alice").unwrap(), "alice");
        assert_eq!(normalize_username("spotify:user:alice").unwrap(), "alice");
        assert_eq!(
            normalize_username("https://open.spotify.com/user/alice?si=secret").unwrap(),
            "alice"
        );
        assert!(normalize_username("spotify:user:").is_err());
    }

    #[test]
    fn path_encoding_cannot_change_the_endpoint() {
        assert_eq!(path_segment("name/../?x=y"), "name%2F..%2F%3Fx%3Dy");
        assert_eq!(path_segment("Bjork fan"), "Bjork%20fan");
    }

    #[test]
    fn parses_current_profile_shape_and_filters_invalid_items() {
        let raw: RawProfile = serde_json::from_str(
            r#"{
              "uri":"spotify:user:alice","name":"Alice","image_url":"avatar",
              "following_count":42,"total_public_playlists_count":3,
              "is_current_user":true,"allow_follows":true,"show_follows":true,
              "recently_played_artists":[
                {"uri":"spotify:artist:a","name":"Artist","followers_count":12},
                {"uri":"","name":"Broken"}
              ],
              "public_playlists":[{"uri":"spotify:playlist:p","name":"Public","owner_name":"Alice"}]
            }"#,
        )
        .unwrap();
        assert_eq!(raw.name.as_deref(), Some("Alice"));
        assert_eq!(
            raw.recently_played_artists
                .unwrap()
                .into_iter()
                .filter_map(valid_artist)
                .count(),
            1
        );
        assert_eq!(
            raw.public_playlists
                .unwrap()
                .into_iter()
                .filter_map(valid_playlist)
                .count(),
            1
        );
    }
}
