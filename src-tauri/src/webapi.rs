use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{AppError, AppResult};

const BASE: &str = "https://api.spotify.com/v1";

/// Maps a non-success Spotify HTTP status (and its already-extracted error
/// message) onto this app's error taxonomy. A free function, pulled out of
/// [`WebApi::send`], so the mapping itself is testable without standing up an
/// HTTP server. Does not handle 429 — that is decided before the body is
/// even read, see [`WebApi::send`] and [`parse_retry_after`].
fn map_error_status(status: reqwest::StatusCode, msg: String) -> AppError {
    match status {
        reqwest::StatusCode::BAD_REQUEST => AppError::BadRequest(msg),
        reqwest::StatusCode::UNAUTHORIZED => AppError::SessionExpired,
        reqwest::StatusCode::FORBIDDEN => AppError::Forbidden(msg),
        reqwest::StatusCode::NOT_FOUND => AppError::Unavailable(msg),
        s if s.is_server_error() => AppError::ServiceUnavailable { status: s.as_u16() },
        _ => AppError::WebApi(format!("{status}: {msg}")),
    }
}

/// Parses a `Retry-After` header value (already extracted as `Option<&str>`
/// so this needs no `reqwest::Response`) into whole seconds. Spotify sends
/// this as a plain integer, never the HTTP-date form, but a malformed or
/// missing header must fall back to `None` rather than panic or default to a
/// wait time nobody asked for.
fn parse_retry_after(value: Option<&str>) -> Option<u64> {
    value.and_then(|v| v.trim().parse::<u64>().ok())
}

/// Minimal Spotify Web API client.
///
/// Holds no token of its own: the caller passes the bearer token obtained from
/// the librespot OAuth flow, so there is exactly one credential in the app.
#[derive(Clone)]
pub struct WebApi {
    http: reqwest::Client,
}

impl Default for WebApi {
    fn default() -> Self {
        Self::new()
    }
}

impl WebApi {
    pub fn new() -> Self {
        Self {
            // Matches PathfinderClient's own builder fallback: a TLS-backend
            // init failure here would previously panic the whole app at
            // startup rather than degrade, for a client that's now built
            // once (see AppState::web_api) rather than per-request.
            http: reqwest::Client::builder()
                .user_agent(concat!("rustify/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_else(|error| {
                    log::error!("failed to build HTTP client with custom config: {error}");
                    reqwest::Client::new()
                }),
        }
    }

    async fn send<T: DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
        token: &str,
    ) -> AppResult<T> {
        // Built rather than sent directly so the URL is available for logging
        // when something fails — a bare "400 Bad Request" says nothing about
        // which request or why.
        let request = req.bearer_auth(token).build()?;
        let method = request.method().clone();
        let url = request.url().clone();

        // A malformed Authorization header makes Spotify answer 400, not 401,
        // which is indistinguishable from a bad query unless it is called out.
        if token.is_empty() {
            log::error!("{method} {url} attempted with an empty bearer token");
        }

        let resp = self.http.execute(request).await?;
        let status = resp.status();

        // 429 is common with librespot's shared default client ID: the quota is
        // pooled across every librespot-based client worldwide, so this can fire
        // on a first request with no prior usage. Surface it as its own variant
        // with the server's own wait hint rather than a generic HTTP error.
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = parse_retry_after(
                resp.headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok()),
            );
            log::warn!("rate limited by Spotify (retry_after={retry_after:?})");
            return Err(AppError::RateLimited { retry_after });
        }

        let body = resp.text().await?;
        if !status.is_success() {
            // Spotify errors are {"error":{"status":..,"message":".."}}
            let msg = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| {
                    if body.trim().is_empty() {
                        "no response body".to_string()
                    } else {
                        body.clone()
                    }
                });
            log::warn!("{method} {url} -> {status}: {msg}");
            return Err(map_error_status(status, msg));
        }

        // Several write endpoints return an empty 200 as well as 204. Parsing
        // that as JSON used to turn a successful library save into a failure.
        if body.trim().is_empty() {
            return serde_json::from_str::<T>("null")
                .map_err(|e| AppError::WebApi(format!("unexpected empty response: {e}")));
        }

        serde_json::from_str::<T>(&body)
            .map_err(|e| AppError::WebApi(format!("failed to parse response: {e}")))
    }

    /// Longest `Retry-After` we will sit through automatically. Beyond this the
    /// error goes to the UI so the user is not left staring at a frozen view.
    const MAX_AUTO_RETRY_SECS: u64 = 8;
    const MAX_RETRIES: u32 = 2;

    /// GETs are safe to repeat, so a short rate-limit window is absorbed here
    /// rather than surfaced. Longer waits, and all non-GET verbs, are returned
    /// to the caller untouched.
    pub async fn get<T: DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
        query: &[(&str, String)],
    ) -> AppResult<T> {
        let mut attempt = 0;
        loop {
            let mut req = self.http.get(format!("{BASE}{path}"));
            if !query.is_empty() {
                req = req.query(query);
            }

            match self.send(req, token).await {
                Err(AppError::RateLimited { retry_after }) if attempt < Self::MAX_RETRIES => {
                    // No header means Spotify did not say; back off gently
                    // rather than hammering.
                    let wait = retry_after.unwrap_or(1 << attempt);
                    if wait > Self::MAX_AUTO_RETRY_SECS {
                        return Err(AppError::RateLimited { retry_after });
                    }
                    log::warn!("rate limited on {path}, retrying in {wait}s");
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    attempt += 1;
                }
                other => return other,
            }
        }
    }

    pub async fn put(&self, token: &str, path: &str, body: Value) -> AppResult<()> {
        let req = self.http.put(format!("{BASE}{path}")).json(&body);
        self.send::<Value>(req, token).await.map(|_| ())
    }

    /// PUT with a caller-supplied content type and an already-encoded body.
    ///
    /// Exists for `/playlists/{id}/images`, the one Spotify endpoint in this
    /// app that does not take JSON: it wants a raw base64 JPEG payload with
    /// `Content-Type: image/jpeg`. Sending it as JSON is rejected with a 400
    /// that names neither the field nor the reason.
    pub async fn put_raw(
        &self,
        token: &str,
        path: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> AppResult<()> {
        let req = self
            .http
            .put(format!("{BASE}{path}"))
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body);
        self.send::<Value>(req, token).await.map(|_| ())
    }

    pub async fn put_query(
        &self,
        token: &str,
        path: &str,
        query: &[(&str, String)],
    ) -> AppResult<()> {
        // Spotify's edge requires an explicit zero-length entity for these
        // query-only mutations; a bodyless request is rejected with 411.
        let req = self
            .http
            .put(format!("{BASE}{path}"))
            .query(query)
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .body("");
        self.send::<Value>(req, token).await.map(|_| ())
    }

    pub async fn post(&self, token: &str, path: &str, body: Value) -> AppResult<()> {
        let req = self.http.post(format!("{BASE}{path}")).json(&body);
        self.send::<Value>(req, token).await.map(|_| ())
    }

    pub async fn post_query(
        &self,
        token: &str,
        path: &str,
        query: &[(&str, String)],
    ) -> AppResult<()> {
        let req = self
            .http
            .post(format!("{BASE}{path}"))
            .query(query)
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .body("");
        self.send::<Value>(req, token).await.map(|_| ())
    }

    pub async fn delete_query(
        &self,
        token: &str,
        path: &str,
        query: &[(&str, String)],
    ) -> AppResult<()> {
        let req = self
            .http
            .delete(format!("{BASE}{path}"))
            .query(query)
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .body("");
        self.send::<Value>(req, token).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_statuses_to_the_expected_error_kind() {
        use reqwest::StatusCode;

        assert_eq!(
            map_error_status(StatusCode::BAD_REQUEST, "bad".into()).kind(),
            "BadRequest"
        );
        assert_eq!(
            map_error_status(StatusCode::UNAUTHORIZED, "expired".into()).kind(),
            "SessionExpired"
        );
        assert_eq!(
            map_error_status(StatusCode::FORBIDDEN, "no".into()).kind(),
            "Forbidden"
        );
        assert_eq!(
            map_error_status(StatusCode::NOT_FOUND, "missing".into()).kind(),
            "Unavailable"
        );
        assert!(matches!(
            map_error_status(StatusCode::INTERNAL_SERVER_ERROR, "oops".into()),
            AppError::ServiceUnavailable { status: 500 }
        ));
        assert!(matches!(
            map_error_status(StatusCode::BAD_GATEWAY, "oops".into()),
            AppError::ServiceUnavailable { status: 502 }
        ));
        // Anything not explicitly handled (e.g. 418) falls through to the
        // generic WebApi variant rather than being silently miscategorised.
        assert_eq!(
            map_error_status(StatusCode::IM_A_TEAPOT, "?".into()).kind(),
            "WebApi"
        );
    }

    #[test]
    fn retry_after_parses_plain_integers_and_rejects_everything_else() {
        assert_eq!(parse_retry_after(Some("5")), Some(5));
        assert_eq!(parse_retry_after(Some(" 12 ")), Some(12));
        // Spotify is documented to send a plain integer, never the HTTP-date
        // form, but a malformed value must degrade to None, not panic.
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(parse_retry_after(Some("")), None);
        assert_eq!(parse_retry_after(None), None);
    }
}
