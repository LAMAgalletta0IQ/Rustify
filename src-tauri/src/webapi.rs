use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{AppError, AppResult};

const BASE: &str = "https://api.spotify.com/v1";

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
            http: reqwest::Client::builder()
                .user_agent(concat!("rustify/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("failed to build HTTP client"),
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
            let retry_after = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            log::warn!("rate limited by Spotify (retry_after={retry_after:?})");
            return Err(AppError::RateLimited { retry_after });
        }

        if status == reqwest::StatusCode::NO_CONTENT {
            // PUT/POST player endpoints answer 204 with an empty body.
            return serde_json::from_str::<T>("null")
                .map_err(|e| AppError::WebApi(format!("unexpected empty response: {e}")));
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
            return Err(AppError::WebApi(format!("{status}: {msg}")));
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

    pub async fn post(&self, token: &str, path: &str, body: Value) -> AppResult<()> {
        let req = self.http.post(format!("{BASE}{path}")).json(&body);
        self.send::<Value>(req, token).await.map(|_| ())
    }

    pub async fn delete(&self, token: &str, path: &str, body: Value) -> AppResult<()> {
        let req = self.http.delete(format!("{BASE}{path}")).json(&body);
        self.send::<Value>(req, token).await.map(|_| ())
    }
}
