use std::collections::HashMap;
use std::time::Duration;

use regex::Regex;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tokio::sync::{Mutex, RwLock};

use crate::error::{AppError, AppResult};

const ENDPOINT: &str = "https://api-partner.spotify.com/pathfinder/v2/query";
const WEB_PLAYER: &str = "https://open.spotify.com/";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

/// Recently observed hashes, newest generated Web Player capture first. They
/// are fallbacks only: a rejected set triggers discovery from Spotify's own
/// currently served bundle and a single retry.
const HOME_HASHES: &[&str] = &[
    "76243c78b0e20ecdbe41b794dec8cbe73f75e585b0a7201b8d2e84578412847a",
    "23e37f2e58d82d567f27080101d36609009d8c3676457b1086cb0acc55b72a5d",
    "d62af2714f2623c923cc9eeca4b9545b4363abaa9188a9e94e2b63b823419a2c",
];

/// The generated libspot registry was refreshed on 2026-08-14. Wavee's
/// independently captured desktop-XPUI operation remains a useful fallback.
const ARTIST_OVERVIEW_HASHES: &[&str] = &[
    "1ac33ddab5d39a3a9c27802774e6d78b9405cc188c6f75aed007df2a32737c72",
    "7f86ff63e38c24973a2842b672abe44c910c1973978dc8a4a0cb648edef34527",
];
const TRACK_CREDITS_HASHES: &[&str] =
    &["e2ca40d46cf1fde36562261ccec754f23fb31b561877252e9fe0d6834aabb84b"];

pub struct PathfinderClient {
    http: Client,
    discovered: RwLock<HashMap<String, String>>,
    discovery_gate: Mutex<()>,
}

impl Default for PathfinderClient {
    fn default() -> Self {
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(25))
                .user_agent(USER_AGENT)
                .build()
                .unwrap_or_else(|_| Client::new()),
            discovered: RwLock::new(HashMap::new()),
            discovery_gate: Mutex::new(()),
        }
    }
}

impl PathfinderClient {
    pub async fn query(
        &self,
        operation: &str,
        variables: Value,
        access_token: &str,
        client_token: &str,
        connection_id: &str,
    ) -> AppResult<Value> {
        let candidates = self.candidates(operation).await;
        for hash in candidates {
            match self
                .query_hash(
                    operation,
                    &variables,
                    &hash,
                    access_token,
                    client_token,
                    connection_id,
                )
                .await
            {
                Ok(value) => {
                    self.discovered
                        .write()
                        .await
                        .insert(operation.to_owned(), hash);
                    return Ok(value);
                }
                Err(QueryFailure::HashRejected) => continue,
                Err(QueryFailure::App(error)) => return Err(error),
            }
        }

        log::warn!(target: "spotify.pathfinder", "all cached hashes for {operation} were rejected; discovering the current Web Player operation");
        let discovered = self.discover_hash(operation).await?;
        match self
            .query_hash(
                operation,
                &variables,
                &discovered,
                access_token,
                client_token,
                connection_id,
            )
            .await
        {
            Ok(value) => {
                self.discovered
                    .write()
                    .await
                    .insert(operation.to_owned(), discovered);
                Ok(value)
            }
            Err(QueryFailure::HashRejected) => Err(AppError::PersistedQueryExpired(format!(
                "Spotify rejected the freshly discovered {operation} operation hash"
            ))),
            Err(QueryFailure::App(error)) => Err(error),
        }
    }

    async fn candidates(&self, operation: &str) -> Vec<String> {
        let mut hashes = Vec::new();
        if let Some(hash) = self.discovered.read().await.get(operation) {
            hashes.push(hash.clone());
        }
        let env_name = format!(
            "RUSTIFY_PATHFINDER_{}_HASH",
            operation.to_ascii_uppercase().replace('-', "_")
        );
        if let Ok(hash) = std::env::var(env_name) {
            if valid_hash(&hash) && !hashes.contains(&hash) {
                hashes.push(hash);
            }
        }
        let known = match operation {
            "home" => HOME_HASHES,
            "queryArtistOverview" => ARTIST_OVERVIEW_HASHES,
            "queryTrackCreditsModal" => TRACK_CREDITS_HASHES,
            _ => &[],
        };
        for hash in known {
            if !hashes.iter().any(|candidate| candidate == hash) {
                hashes.push((*hash).to_owned());
            }
        }
        hashes
    }

    async fn query_hash(
        &self,
        operation: &str,
        variables: &Value,
        hash: &str,
        access_token: &str,
        client_token: &str,
        connection_id: &str,
    ) -> Result<Value, QueryFailure> {
        let payload = json!({
            "operationName": operation,
            "variables": variables,
            "extensions": {"persistedQuery": {"version": 1, "sha256Hash": hash}}
        });
        let desktop_artist = operation == "queryArtistOverview";
        let mut request = self
            .http
            .post(ENDPOINT)
            .bearer_auth(access_token)
            .header("client-token", client_token)
            .header(
                "app-platform",
                if desktop_artist {
                    "Win32_x86_64"
                } else {
                    "WebPlayer"
                },
            )
            .header(
                "Origin",
                if desktop_artist {
                    "https://xpui.app.spotify.com"
                } else {
                    "https://open.spotify.com"
                },
            )
            .header(
                "Referer",
                if desktop_artist {
                    "https://xpui.app.spotify.com/"
                } else {
                    "https://open.spotify.com/"
                },
            )
            .header("Accept", "application/json");
        if desktop_artist {
            request = request
                .header("spotify-app-version", "896000000")
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.7680.179 Spotify/1.2.88.483 Safari/537.36")
                .header("Accept-Language", "en");
        }
        if !connection_id.is_empty() {
            request = request.header("Spotify-Connection-Id", connection_id);
        }
        let response = request
            .json(&payload)
            .send()
            .await
            .map_err(AppError::from)?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());
        let body = response.text().await.map_err(AppError::from)?;
        log::debug!(target: "spotify.pathfinder", "{operation} -> {status} ({} bytes)", body.len());
        if status == StatusCode::UNAUTHORIZED {
            return Err(AppError::SessionExpired.into());
        }
        if status == StatusCode::FORBIDDEN {
            return Err(AppError::Forbidden(safe_excerpt(&body)).into());
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(AppError::RateLimited { retry_after }.into());
        }
        if status == StatusCode::PRECONDITION_FAILED || persisted_query_not_found(&body) {
            return Err(QueryFailure::HashRejected);
        }
        if !status.is_success() {
            return Err(AppError::ServiceUnavailable {
                status: status.as_u16(),
            }
            .into());
        }
        let response: Value = serde_json::from_str(&body)
            .map_err(|error| AppError::WebApi(format!("invalid Pathfinder JSON: {error}")))?;
        if response
            .get("errors")
            .and_then(Value::as_array)
            .is_some_and(|errors| !errors.is_empty())
        {
            return Err(AppError::WebApi(format!(
                "Pathfinder {operation}: {}",
                safe_excerpt(&body)
            ))
            .into());
        }
        Ok(response.get("data").cloned().unwrap_or(Value::Null))
    }

    async fn discover_hash(&self, operation: &str) -> AppResult<String> {
        let _gate = self.discovery_gate.lock().await;
        if let Some(hash) = self.discovered.read().await.get(operation) {
            return Ok(hash.clone());
        }
        let html = self.fetch_text(WEB_PLAYER).await?;
        let bundle_re = Regex::new(r#"<script[^>]+src="([^"]+)""#)
            .map_err(|error| AppError::Other(error.to_string()))?;
        let main_url = bundle_re
            .captures_iter(&html)
            .filter_map(|capture| capture.get(1).map(|value| value.as_str()))
            .find(|url| {
                url.contains("/web-player/web-player.")
                    || url.contains("/mobile-web-player/mobile-web-player.")
            })
            .ok_or_else(|| {
                AppError::PersistedQueryExpired(
                    "current Web Player bundle was not advertised".into(),
                )
            })?;
        let main = self.fetch_text(main_url).await?;
        if let Some(hash) = find_operation_hash(&main, operation) {
            return Ok(hash);
        }

        let base = main_url
            .rsplit_once('/')
            .map(|(base, _)| format!("{base}/"))
            .unwrap_or_else(|| "https://open.spotifycdn.com/cdn/build/web-player/".into());
        let mut chunks = webpack_chunks(&main)?;
        chunks.sort_by_key(|name| {
            let lower = name.to_ascii_lowercase();
            (
                !lower.contains("home"),
                !lower.contains("shelf"),
                name.clone(),
            )
        });
        for chunk in chunks.into_iter().take(192) {
            let Ok(body) = self.fetch_text(&format!("{base}{chunk}")).await else {
                continue;
            };
            if let Some(hash) = find_operation_hash(&body, operation) {
                log::info!(target: "spotify.pathfinder", "discovered current {operation} hash from Web Player chunk {chunk}");
                return Ok(hash);
            }
        }
        Err(AppError::PersistedQueryExpired(format!(
            "could not discover operation {operation} in the current Web Player bundles"
        )))
    }

    async fn fetch_text(&self, url: &str) -> AppResult<String> {
        let response = self
            .http
            .get(url)
            .header("Accept", "text/html,application/javascript,*/*")
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(AppError::ServiceUnavailable {
                status: response.status().as_u16(),
            });
        }
        let body = response.text().await?;
        if body.len() > 24 * 1024 * 1024 {
            return Err(AppError::PersistedQueryExpired(
                "Web Player bundle exceeded the 24 MiB discovery safety limit".into(),
            ));
        }
        Ok(body)
    }
}

enum QueryFailure {
    HashRejected,
    App(AppError),
}

impl From<AppError> for QueryFailure {
    fn from(value: AppError) -> Self {
        Self::App(value)
    }
}

fn persisted_query_not_found(body: &str) -> bool {
    body.contains("PersistedQueryNotFound") || body.contains("PERSISTED_QUERY_NOT_FOUND")
}

fn safe_excerpt(body: &str) -> String {
    body.chars().take(300).collect()
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn find_operation_hash(body: &str, operation: &str) -> Option<String> {
    let operation = regex::escape(operation);
    let patterns = [
        format!(
            r#"(?s){}.{{0,600}}?sha256Hash\\?":\\?"([a-f0-9]{{64}})"#,
            operation
        ),
        format!(r#""{}","(?:query|mutation)","([a-f0-9]{{64}})""#, operation),
    ];
    patterns.into_iter().find_map(|pattern| {
        Regex::new(&pattern)
            .ok()?
            .captures(body)?
            .get(1)
            .map(|value| value.as_str().to_owned())
    })
}

fn webpack_chunks(js: &str) -> AppResult<Vec<String>> {
    let map_re = Regex::new(r#"\{(?:\d+:"[^"]+",?)+\}"#)
        .map_err(|error| AppError::Other(error.to_string()))?;
    let pair_re =
        Regex::new(r#"(\d+):"([^"]+)""#).map_err(|error| AppError::Other(error.to_string()))?;
    let mut maps: Vec<HashMap<u32, String>> = map_re
        .find_iter(js)
        .map(|raw| {
            pair_re
                .captures_iter(raw.as_str())
                .filter_map(|capture| {
                    Some((
                        capture.get(1)?.as_str().parse().ok()?,
                        capture.get(2)?.as_str().to_owned(),
                    ))
                })
                .collect()
        })
        .filter(|map: &HashMap<u32, String>| !map.is_empty())
        .collect();
    let name_index = maps
        .iter()
        .enumerate()
        .max_by_key(|(_, map)| {
            map.values()
                .filter(|value| value.contains('-') || value.contains('/'))
                .count()
        })
        .map(|(index, _)| index);
    let hash_index = maps
        .iter()
        .enumerate()
        .max_by_key(|(_, map)| {
            map.values()
                .filter(|value| {
                    (6..=12).contains(&value.len())
                        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                .count()
        })
        .map(|(index, _)| index);
    let (Some(name_index), Some(hash_index)) = (name_index, hash_index) else {
        return Err(AppError::PersistedQueryExpired(
            "could not locate Webpack chunk maps".into(),
        ));
    };
    let hashes = maps.swap_remove(hash_index);
    let adjusted_name_index = if hash_index < name_index {
        name_index.saturating_sub(1)
    } else {
        name_index
    };
    let names = maps.get(adjusted_name_index).ok_or_else(|| {
        AppError::PersistedQueryExpired("could not read Webpack chunk-name map".into())
    })?;
    let chunks = names
        .iter()
        .filter_map(|(id, name)| Some(format!("{name}.{}.js", hashes.get(id)?)))
        .collect();
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_operation_hash_from_both_bundle_forms() {
        let hash = "a".repeat(64);
        let object = format!(r#"home blah sha256Hash\":\"{hash}"#);
        assert_eq!(
            find_operation_hash(&object, "home").as_deref(),
            Some(hash.as_str())
        );
        let tuple = format!(r#""home","query","{hash}""#);
        assert_eq!(
            find_operation_hash(&tuple, "home").as_deref(),
            Some(hash.as_str())
        );
    }

    #[test]
    fn combines_scored_webpack_maps() {
        let js = r#"x={1:"home-web",2:"search-web"};y={1:"abcdef12",2:"1234abcd"}"#;
        let chunks = webpack_chunks(js).unwrap();
        assert!(chunks.contains(&"home-web.abcdef12.js".to_owned()));
    }

    #[tokio::test]
    async fn includes_newest_artist_overview_hash_first() {
        let candidates = PathfinderClient::default()
            .candidates("queryArtistOverview")
            .await;
        assert_eq!(candidates[0], ARTIST_OVERVIEW_HASHES[0]);
        assert_eq!(candidates.len(), ARTIST_OVERVIEW_HASHES.len());
    }

    /// Opt-in because it downloads Spotify's currently deployed Web Player.
    /// It needs no account and catches changes to Webpack's bundle layout.
    #[tokio::test]
    #[ignore = "live Web Player discovery"]
    async fn discovers_live_home_hash() {
        let hash = PathfinderClient::default()
            .discover_hash("home")
            .await
            .unwrap();
        assert!(valid_hash(&hash));
    }
}
