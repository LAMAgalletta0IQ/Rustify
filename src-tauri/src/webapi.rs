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
                .user_agent(concat!("spotify-rust/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    async fn send<T: DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
        token: &str,
    ) -> AppResult<T> {
        let resp = req.bearer_auth(token).send().await?;
        let status = resp.status();

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
                .unwrap_or_else(|| body.clone());
            return Err(AppError::WebApi(format!("{status}: {msg}")));
        }

        serde_json::from_str::<T>(&body)
            .map_err(|e| AppError::WebApi(format!("failed to parse response: {e}")))
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
        query: &[(&str, String)],
    ) -> AppResult<T> {
        let mut req = self.http.get(format!("{BASE}{path}"));
        if !query.is_empty() {
            req = req.query(query);
        }
        self.send(req, token).await
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
