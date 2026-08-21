use std::collections::HashMap;
use std::sync::Arc;

use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use tracing::{debug, warn};

use super::client_token::ClientTokenManager;
use super::config::JamConfig;
use super::error::JamError;
use super::token::TokenProvider;
use super::{ClientIdentity, ConnectionId};

/// Generic HTTPS client for the SpClient family of APIs, which serves the Jam
/// actions (create / join / update / leave) along with playback and messaging.
///
/// The **base URL is a known host**; the **endpoint paths are internal** and
/// change without notice, so:
///
/// - [`JamApiClient::post`] / [`JamApiClient::get`] accept any path or absolute
///   URL directly for ad-hoc probing (e.g. while sniffing traffic).
/// - the higher-level actions below resolve a path *template* out of
///   `JamConfig.spclient_endpoints`, which
///   [`default_spclient_endpoints`](super::config::default_spclient_endpoints)
///   pre-fills, and **refuse loudly** for any name that has neither a default
///   nor a configured path — they never guess an endpoint.
///
/// Every request carries the OAuth bearer, `client-token`, and the current
/// `Spotify-Connection-Id` from the shared [`ConnectionId`], tying the call to
/// the dealer websocket session.
#[derive(Clone)]
pub struct JamApiClient {
    http: Client,
    base_url: String,
    endpoints: HashMap<String, String>,
    /// This app's Connect device id. social-connect keys a session to it, so
    /// every jam action carries it as `local_device_id`.
    local_device_id: String,
    connection_id: ConnectionId,
    access_token: Arc<dyn TokenProvider>,
    /// Bearer for public `api.spotify.com` calls, when the host supplies a
    /// different one. See [`JamApiClient::bearer_for`].
    web_api_token: Option<Arc<dyn TokenProvider>>,
    client_token: ClientTokenManager,
}

impl JamApiClient {
    pub fn new(
        http: Client,
        config: &JamConfig,
        identity: &ClientIdentity,
        connection_id: ConnectionId,
        access_token: Arc<dyn TokenProvider>,
        web_api_token: Option<Arc<dyn TokenProvider>>,
        client_token: ClientTokenManager,
    ) -> Result<Self, JamError> {
        if config.spclient_base_url.trim().is_empty() {
            return Err(JamError::Config("spclient_base_url must not be empty".into()));
        }
        Ok(Self {
            http,
            base_url: config.spclient_base_url.trim_end_matches('/').to_owned(),
            endpoints: config.spclient_endpoints.clone(),
            local_device_id: identity.device_id.clone(),
            connection_id,
            access_token,
            web_api_token,
            client_token,
        })
    }

    /// Generic POST to an arbitrary path or absolute URL, attaching the OAuth
    /// bearer and `client-token` headers.
    pub async fn post<B: Serialize>(&self, endpoint: &str, body: &B) -> Result<Value, JamError> {
        let url = self.absolute(endpoint);
        let url_for_log = url.clone();
        debug!("spclient POST {url}");
        let resp = self
            .http
            .post(url)
            .headers(self.headers(&url_for_log).await?)
            .json(body)
            .send()
            .await?;
        self.drain(resp, "POST", &url_for_log).await
    }

    /// POST with query parameters and an optional JSON body. The jam actions
    /// need both: social-connect takes `local_device_id` in the query string
    /// while the body is usually empty.
    pub async fn post_with_query(
        &self,
        endpoint: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value, JamError> {
        let url = self.absolute(endpoint);
        let url_for_log = url.clone();
        let shape = match body {
            Some(_) => "json body",
            None => "empty body, content-length: 0",
        };
        debug!("spclient POST {url} {query:?} ({shape})");
        let mut req = self.http.post(url).headers(self.headers(&url_for_log).await?).query(query);
        // An empty body is not the same as `{}` to every endpoint, so only
        // send one when the caller supplied it — but a POST with no body at
        // all carries no `Content-Length` either, and Google's frontend in
        // front of spclient rejects that with `411 Length Required` before
        // Spotify ever sees the request. An empty byte body sets
        // `Content-Length: 0`, which satisfies it.
        req = match body {
            Some(body) => req.json(body),
            // Set the header ourselves rather than relying on the transport to
            // infer it from an empty body: hyper does add it, but only after
            // the request reaches its h1/h2 encoder, and anything that short-
            // circuits that path (a proxy layer, a redirect replay) drops it
            // again. An explicit header survives all of them.
            None => req
                .header(reqwest::header::CONTENT_LENGTH, "0")
                .body(Vec::<u8>::new()),
        };
        let resp = req.send().await?;
        self.drain(resp, "POST", &url_for_log).await
    }

    /// Generic GET to an arbitrary path or absolute URL, with query parameters
    /// passed as `(key, value)` pairs.
    pub async fn get(&self, endpoint: &str, query: &[(&str, String)]) -> Result<Value, JamError> {
        let url = self.absolute(endpoint);
        let url_for_log = url.clone();
        debug!("spclient GET {url}");
        let mut req = self.http.get(url).headers(self.headers(&url_for_log).await?);
        for (key, value) in query {
            req = req.query(&[(key, value)]);
        }
        let resp = req.send().await?;
        self.drain(resp, "GET", &url_for_log).await
    }

    /// Resolves a named endpoint template to an absolute URL, substituting
    /// `{name}` placeholders. Errors when the name has no configured path.
    fn resolve(&self, name: &str, vars: &[(&str, &str)]) -> Result<String, JamError> {
        let template = self.endpoints.get(name).ok_or_else(|| JamError::Config(format!(
            "no endpoint path configured for '{name}'; capture it with a traffic proxy and add it \
             to JamConfig.spclient_endpoints"
        )))?;
        let mut path = template.clone();
        for (key, value) in vars {
            path = path.replace(&format!("{{{key}}}"), value);
        }
        Ok(self.absolute(&path))
    }

    /// The Connect device id every jam action needs. Empty means the module
    /// was built without a live librespot session, which social-connect would
    /// answer with an opaque 400 — so say what is actually wrong instead.
    fn device_id(&self) -> Result<String, JamError> {
        if self.local_device_id.trim().is_empty() {
            return Err(JamError::Config(
                "no Connect device id; jams need a live librespot session (log in first)".into(),
            ));
        }
        Ok(self.local_device_id.clone())
    }

    fn absolute(&self, endpoint: &str) -> String {
        if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
            endpoint.to_owned()
        } else {
            format!("{}/{}", self.base_url, endpoint.trim_start_matches('/'))
        }
    }

    /// Picks the bearer for a URL.
    ///
    /// The two are **not** interchangeable. spclient and Pathfinder are
    /// first-party services: they answer a token minted for a self-registered
    /// Client ID with `403 RBAC: access denied` no matter its scopes, so those
    /// hosts get the token librespot obtains under Spotify's own desktop
    /// client id. The public Web API is the opposite case — it is what the
    /// app's own Client ID exists for, and using it keeps that traffic on the
    /// user's own quota.
    fn bearer_for(&self, url: &str) -> Option<String> {
        let public_api = url.starts_with("https://api.spotify.com/");
        match (public_api, &self.web_api_token) {
            (true, Some(provider)) => provider.access_token(),
            _ => self.access_token.access_token(),
        }
    }

    async fn headers(&self, url: &str) -> Result<reqwest::header::HeaderMap, JamError> {
        let token = self.bearer_for(url).ok_or_else(|| {
            // Not "you are logged out": the app can be fully authenticated
            // while this is empty, because the bearer these hosts need is the
            // first-party one the host app fetches from librespot (login5),
            // not the app's own Web API token. See `jams_bridge::refresh`.
            JamError::ClientTokenExpired(format!(
                "no bearer token available for {url}; the host app has not supplied one yet                  (check the log for a librespot login5 warning)"
            ))
        })?;
        let client_token = self.client_token.get_token().await?;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {token}").parse().expect("valid authorization header"),
        );
        headers.insert(
            "client-token",
            client_token.parse().expect("valid client-token header"),
        );
        let conn_id = self.connection_id.current();
        if !conn_id.is_empty() {
            headers.insert(
                "Spotify-Connection-Id",
                conn_id.parse().expect("valid connection id header"),
            );
        }
        Ok(headers)
    }

    /// Reads the response and classifies a failure.
    ///
    /// Failures log method, URL, status and body — these endpoints answer a
    /// wrong header or a stale path with an **empty-bodied 400**, so the URL
    /// in the log is often the only thing distinguishing "the path moved" from
    /// "the connection id is wrong". The error carries it too, for the same
    /// reason.
    async fn drain(
        &self,
        resp: reqwest::Response,
        method: &str,
        url: &str,
    ) -> Result<Value, JamError> {
        let status = resp.status();
        let text = resp.text().await?;
        if status.is_success() {
            if text.trim().is_empty() {
                return Ok(Value::Null);
            }
            return Ok(serde_json::from_str(&text)?);
        }
        warn!("spclient {method} {url} -> {status}: {}", match text.trim().is_empty() {
            true => "<empty body>",
            false => text.trim(),
        });
        let detail = match text.trim().is_empty() {
            true => format!("{method} {url} (no response body)"),
            false => format!("{method} {url}: {text}"),
        };
        match status.as_u16() {
            401 | 403 => Err(JamError::PermissionDenied(detail)),
            404 => Err(JamError::JamNotFound(detail)),
            _ => Err(JamError::Http { status, body: detail }),
        }
    }
}

// ---------------------------------------------------------------------------
// Jam actions.
//
// Jams are the current name for Spotify's *social connect sessions* (formerly
// Group Sessions), and that is the service these call. Paths come from
// `JamConfig.spclient_endpoints` — pre-filled by `default_spclient_endpoints`,
// overridable from `jams.toml` — so a path that drifts is a config edit, not a
// rebuild. The verbs and query parameters below are fixed per action because
// they differ per endpoint.
//
// `reorder_queue` has no default path and therefore still refuses: no public
// capture documents one, and guessing would fire a real request.
// ---------------------------------------------------------------------------

impl JamApiClient {
    /// Returns the session this device is already in, or opens a new one.
    ///
    /// `context_uri` is accepted for API symmetry but not sent: a
    /// social-connect session adopts whatever the host device is playing
    /// rather than being created around a context.
    pub async fn create_jam(&self, context_uri: Option<&str>) -> Result<Value, JamError> {
        if let Some(uri) = context_uri.filter(|u| !u.is_empty()) {
            debug!("create_jam ignoring context_uri {uri}: the jam adopts current playback");
        }
        let url = self.resolve("create_jam", &[])?;
        self.get(
            &url,
            &[
                ("local_device_id", self.device_id()?),
                ("type", "REMOTE".to_owned()),
            ],
        )
        .await
    }

    /// The session this device is in, if any. 404 surfaces as
    /// [`JamError::JamNotFound`], which is the normal "not in a jam" answer.
    pub async fn current_jam(&self) -> Result<Value, JamError> {
        let url = self.resolve("current_jam", &[])?;
        self.get(&url, &[("local_device_id", self.device_id()?)]).await
    }

    /// Joins by **join token** — the trailing segment of an invite link
    /// (`open.spotify.com/socialsession/<token>`), not the session id and not
    /// a `spotify.link` shortlink id; `JamManager::resolve_join_token` sorts
    /// those out before calling this.
    ///
    /// > Until 2026-08 the default path here was `v3`, which 404s. Only the
    /// > join path was ever in doubt — the rest of social-connect answers on
    /// > `v2`, and so does this one. A brief fallback chain that tried several
    /// > shapes on 404 established that and has been removed.
    pub async fn join_jam(&self, jam_id: &str) -> Result<Value, JamError> {
        let url = self.resolve("join_jam", &[("jam_id", jam_id)])?;
        self.post_with_query(
            &url,
            &[
                ("local_device_id", self.device_id()?),
                ("playback_control", "listen_and_control".to_owned()),
            ],
            None,
        )
        .await
    }

    /// Queues a track for everyone in the jam. A jam's queue *is* the Connect
    /// queue, so this is the public Web API queue endpoint by default —
    /// `jam_id` is unused, kept so the call site reads the same as the others.
    pub async fn add_track(&self, jam_id: &str, track_uri: &str) -> Result<Value, JamError> {
        let url = self.resolve("add_track", &[("jam_id", jam_id)])?;
        self.post_with_query(
            &url,
            &[
                ("uri", track_uri.to_owned()),
                ("device_id", self.device_id()?),
            ],
            None,
        )
        .await
    }

    /// Not available: no captured path exists for reordering a jam queue, so
    /// this errors rather than guessing one. Add `reorder_queue` to
    /// `[spclient_endpoints]` in `jams.toml` once you have captured it.
    pub async fn reorder_queue(&self, jam_id: &str, new_order: Vec<usize>) -> Result<Value, JamError> {
        let url = self.resolve("reorder_queue", &[("jam_id", jam_id)])?;
        let body = json!({ "jam_id": jam_id, "order": new_order });
        self.post(&url, &body).await
    }

    /// Leaves whatever session this device is in.
    pub async fn leave(&self, jam_id: &str) -> Result<Value, JamError> {
        let url = self.resolve("leave", &[("jam_id", jam_id)])?;
        self.post_with_query(&url, &[("local_device_id", self.device_id()?)], None)
            .await
    }
}
