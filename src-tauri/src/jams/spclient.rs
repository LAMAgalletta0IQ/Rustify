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
            return Err(JamError::Config(
                "spclient_base_url must not be empty".into(),
            ));
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
        debug!("spclient POST {}", safe_url(&url));
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
        debug!("spclient POST {} {query:?} ({shape})", safe_url(&url));
        let mut req = self
            .http
            .post(url)
            .headers(self.headers(&url_for_log).await?)
            .query(query);
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
        debug!("spclient GET {}", safe_url(&url));
        let mut req = self
            .http
            .get(url)
            .headers(self.headers(&url_for_log).await?);
        for (key, value) in query {
            req = req.query(&[(key, value)]);
        }
        let resp = req.send().await?;
        self.drain(resp, "GET", &url_for_log).await
    }

    /// PUT action with an explicit zero-length body. Social-connect's
    /// queue-only toggle uses this shape.
    pub async fn put_empty(&self, endpoint: &str) -> Result<Value, JamError> {
        let url = self.absolute(endpoint);
        let url_for_log = url.clone();
        debug!("spclient PUT {} (content-length: 0)", safe_url(&url));
        let resp = self
            .http
            .put(url)
            .headers(self.headers(&url_for_log).await?)
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .body(Vec::<u8>::new())
            .send()
            .await?;
        self.drain(resp, "PUT", &url_for_log).await
    }

    /// DELETE action used by a host to terminate a v3 social-connect session.
    pub async fn delete(&self, endpoint: &str) -> Result<Value, JamError> {
        let url = self.absolute(endpoint);
        let url_for_log = url.clone();
        debug!("spclient DELETE {}", safe_url(&url));
        let resp = self
            .http
            .delete(url)
            .headers(self.headers(&url_for_log).await?)
            .send()
            .await?;
        self.drain(resp, "DELETE", &url_for_log).await
    }

    /// Resolves a named endpoint template to an absolute URL, substituting
    /// `{name}` placeholders. Errors when the name has no configured path.
    fn resolve(&self, name: &str, vars: &[(&str, &str)]) -> Result<String, JamError> {
        let template = self.endpoints.get(name).ok_or_else(|| {
            JamError::Config(format!(
            "no endpoint path configured for '{name}'; capture it with a traffic proxy and add it \
             to JamConfig.spclient_endpoints"
        ))
        })?;
        let mut path = template.clone();
        for (key, value) in vars {
            path = path.replace(&format!("{{{key}}}"), &path_segment(value));
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
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        // The spotify-jam 0.2 capture identifies the caller as the desktop
        // surface. This is not a fabricated entitlement or credential: it is
        // the protocol's platform discriminator, paired with the genuine
        // Login5 bearer/client-token from this librespot desktop session.
        headers.insert(
            "app-platform",
            reqwest::header::HeaderValue::from_static("Win32_x86_64"),
        );
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| {
                JamError::Config("OAuth token contains invalid header bytes".into())
            })?,
        );
        headers.insert(
            "client-token",
            reqwest::header::HeaderValue::from_str(&client_token).map_err(|_| {
                JamError::Config("client-token contains invalid header bytes".into())
            })?,
        );
        let conn_id = self.connection_id.current();
        if !conn_id.is_empty() {
            headers.insert(
                "Spotify-Connection-Id",
                reqwest::header::HeaderValue::from_str(&conn_id).map_err(|_| {
                    JamError::Config("connection id contains invalid header bytes".into())
                })?,
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
        let safe_url = safe_url(url);
        let safe_body = safe_body(&text);
        warn!(
            "spclient {method} {safe_url} -> {status}: {}",
            match text.trim().is_empty() {
                true => "<empty body>",
                false => &safe_body,
            }
        );
        let detail = match text.trim().is_empty() {
            true => format!("{method} {safe_url} (no response body)"),
            false => format!("{method} {safe_url}: {safe_body}"),
        };
        match status.as_u16() {
            401 | 403 => Err(JamError::PermissionDenied(detail)),
            404 => Err(JamError::JamNotFound(detail)),
            _ => Err(JamError::Http {
                status,
                body: detail,
            }),
        }
    }
}

fn path_segment(value: &str) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn safe_url(value: &str) -> String {
    const JOIN: &str = "/sessions/join/";
    let Some(marker) = value.find(JOIN) else {
        return value.to_owned();
    };
    let start = marker + JOIN.len();
    let end = value[start..]
        .find(['?', '#'])
        .map(|offset| start + offset)
        .unwrap_or(value.len());
    let mut redacted = value.to_owned();
    redacted.replace_range(start..end, "<redacted>");
    redacted
}

fn safe_body(value: &str) -> String {
    fn redact(value: &mut Value) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    let key = key.to_ascii_lowercase();
                    if key.contains("authorization")
                        || key.contains("access_token")
                        || key.contains("refresh_token")
                        || key.contains("client_token")
                        || key.contains("join_session_token")
                        || key.contains("cookie")
                    {
                        *child = Value::String("<redacted>".into());
                    } else {
                        redact(child);
                    }
                }
            }
            Value::Array(values) => values.iter_mut().for_each(redact),
            _ => {}
        }
    }

    let mut output = match serde_json::from_str::<Value>(value) {
        Ok(mut json) => {
            redact(&mut json);
            json.to_string()
        }
        Err(_) => value.to_owned(),
    };
    if output.len() > 2_000 {
        output.truncate(2_000);
        output.push('…');
    }
    output
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
        // Exact spotify-jam 0.2 `startJam` request. `activate=true` is the
        // state-changing switch; omitting it can merely return an inactive
        // current session. Device/Dealer binding is carried by the real
        // connection header, not by undocumented query parameters.
        self.get(&url, &[("activate", "true".to_owned())]).await
    }

    /// The session this device is in, if any. 404 surfaces as
    /// [`JamError::JamNotFound`], which is the normal "not in a jam" answer.
    pub async fn current_jam(&self) -> Result<Value, JamError> {
        let url = self.resolve("current_jam", &[])?;
        self.get(&url, &[("local_device_id", self.device_id()?)])
            .await
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
    pub async fn reorder_queue(
        &self,
        jam_id: &str,
        new_order: Vec<usize>,
    ) -> Result<Value, JamError> {
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

    /// Allows or denies participant queue control. The service expresses the
    /// permission inversely as `queue_only_mode`.
    pub async fn set_queue_control(&self, allowed: bool) -> Result<Value, JamError> {
        let name = if allowed {
            "queue_control_allowed"
        } else {
            "queue_control_denied"
        };
        let url = self.resolve(name, &[])?;
        self.put_empty(&url).await
    }

    /// Removes a participant and returns the updated session payload.
    pub async fn kick_member(&self, jam_id: &str, member_id: &str) -> Result<Value, JamError> {
        if member_id.trim().is_empty() {
            return Err(JamError::Config("member id must not be empty".into()));
        }
        let url = self.resolve(
            "kick_member",
            &[("jam_id", jam_id), ("member_id", member_id)],
        )?;
        self.post_with_query(&url, &[], None).await
    }

    /// Ends a hosted session for every participant.
    pub async fn end_jam(&self, jam_id: &str) -> Result<Value, JamError> {
        let url = self.resolve("end_jam", &[("jam_id", jam_id)])?;
        self.delete(&url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    fn capture_once(status: &str, body: &str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        let status = status.to_owned();
        let body = body.to_owned();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 2048];
            while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                let count = stream.read(&mut chunk).unwrap();
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..count]);
            }
            tx.send(String::from_utf8_lossy(&bytes).into_owned())
                .unwrap();
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        (format!("http://{address}"), rx)
    }

    fn client(base_url: String) -> JamApiClient {
        let http = Client::builder().build().unwrap();
        let config = JamConfig {
            spclient_base_url: base_url,
            ..JamConfig::default()
        };
        let identity = ClientIdentity {
            device_id: "device-1".into(),
            ..ClientIdentity::default()
        };
        let connection = ConnectionId::new();
        connection.set("connection-1");
        JamApiClient::new(
            http.clone(),
            &config,
            &identity,
            connection,
            Arc::new(super::super::token::AccessToken::new("first-party")),
            None,
            ClientTokenManager::supplied(
                http,
                identity.clone(),
                super::super::token::AccessToken::new("client-token-1"),
            ),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn create_matches_captured_start_jam_request() {
        let (base, request) = capture_once(
            "200 OK",
            r#"{"session_id":"jam-1","join_session_uri":"spotify:socialsession:token"}"#,
        );
        let response = client(base).create_jam(None).await.unwrap();
        assert_eq!(response["session_id"], "jam-1");
        let request = request.recv_timeout(Duration::from_secs(2)).unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(request
            .starts_with("GET /social-connect/v2/sessions/current_or_new?activate=true HTTP/1.1"));
        assert!(!request.contains("local_device_id"));
        assert!(!request.contains("type=REMOTE"));
        assert!(lower.contains("authorization: bearer first-party"));
        assert!(lower.contains("client-token: client-token-1"));
        assert!(lower.contains("spotify-connection-id: connection-1"));
        assert!(lower.contains("app-platform: win32_x86_64"));
    }

    #[tokio::test]
    async fn current_and_permissions_use_the_expected_v2_routes() {
        let (base, request) = capture_once("200 OK", r#"{"session_id":"jam-1"}"#);
        client(base).current_jam().await.unwrap();
        assert!(request
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .starts_with("GET /social-connect/v2/sessions/current?local_device_id=device-1"));

        let (base, request) = capture_once("200 OK", "");
        client(base).set_queue_control(false).await.unwrap();
        let request = request.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(request.starts_with(
            "PUT /social-connect/v2/sessions/current/queue_only_mode/enabled HTTP/1.1"
        ));
        assert!(request.to_ascii_lowercase().contains("content-length: 0"));
    }

    #[tokio::test]
    async fn host_moderation_uses_v3_and_classifies_not_found() {
        let (base, request) = capture_once("200 OK", r#"{"session_id":"jam-1"}"#);
        client(base).kick_member("jam-1", "member-2").await.unwrap();
        assert!(request
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .starts_with("POST /social-connect/v3/sessions/jam-1/member/member-2/kick HTTP/1.1"));

        let (base, request) = capture_once("404 Not Found", r#"{"error":"gone"}"#);
        let error = client(base).end_jam("jam-1").await.unwrap_err();
        assert!(matches!(error, JamError::JamNotFound(_)));
        assert!(request
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .starts_with("DELETE /social-connect/v3/sessions/jam-1 HTTP/1.1"));
    }

    #[tokio::test]
    async fn join_and_leave_bind_the_live_connect_device() {
        let (base, request) = capture_once("200 OK", r#"{"session_id":"jam-1"}"#);
        client(base).join_jam("invite-token").await.unwrap();
        let request = request.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(request.starts_with(
            "POST /social-connect/v2/sessions/join/invite-token?local_device_id=device-1&playback_control=listen_and_control HTTP/1.1"
        ));
        assert!(request.to_ascii_lowercase().contains("content-length: 0"));

        let (base, request) = capture_once("200 OK", "");
        client(base).leave("jam-1").await.unwrap();
        assert!(request
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .starts_with(
                "POST /social-connect/v2/sessions/leave?local_device_id=device-1 HTTP/1.1"
            ));
    }

    #[test]
    fn path_parameters_cannot_escape_their_segment() {
        assert_eq!(
            path_segment("member/../?admin=true"),
            "member%2F..%2F%3Fadmin%3Dtrue"
        );
        assert_eq!(path_segment("ordinary-token"), "ordinary-token");
    }

    #[test]
    fn failure_diagnostics_redact_join_credentials() {
        assert_eq!(
            safe_url("https://spclient/social-connect/v2/sessions/join/secret?x=1"),
            "https://spclient/social-connect/v2/sessions/join/<redacted>?x=1"
        );
        let body = safe_body(
            r#"{"join_session_token":"secret","nested":{"access_token":"also-secret"},"error":"bad"}"#,
        );
        assert!(!body.contains("also-secret"));
        assert!(!body.contains("\"secret\""));
        assert!(body.contains("<redacted>"));
        assert!(body.contains("bad"));
    }

    /// Opt-in destructive integration test for a dedicated Spotify test
    /// account/device. It creates and always attempts to end a real Jam.
    /// CI never runs it, and credentials are read only from the process
    /// environment so they cannot enter fixtures or Git.
    #[tokio::test]
    #[ignore = "requires an explicitly provisioned live Spotify test session"]
    async fn live_host_lifecycle_when_explicitly_enabled() {
        fn required(name: &str) -> String {
            std::env::var(name).unwrap_or_else(|_| panic!("missing {name}"))
        }
        assert_eq!(required("SPOTIFY_RUN_JAM_LIVE"), "1");
        let http = Client::builder().build().unwrap();
        let identity = ClientIdentity {
            device_id: required("SPOTIFY_TEST_ACCOUNT_1_DEVICE_ID"),
            ..ClientIdentity::default()
        };
        let connection = ConnectionId::new();
        connection.set(required("SPOTIFY_TEST_ACCOUNT_1_CONNECTION_ID"));
        let api = JamApiClient::new(
            http.clone(),
            &JamConfig::default(),
            &identity,
            connection,
            Arc::new(super::super::token::AccessToken::new(required(
                "SPOTIFY_TEST_ACCOUNT_1_ACCESS_TOKEN",
            ))),
            None,
            ClientTokenManager::supplied(
                http,
                identity.clone(),
                super::super::token::AccessToken::new(required(
                    "SPOTIFY_TEST_ACCOUNT_1_CLIENT_TOKEN",
                )),
            ),
        )
        .unwrap();

        let session =
            super::super::session::session_from_value(api.create_jam(None).await.unwrap()).unwrap();
        assert!(!session.id.is_empty());
        assert!(session.join_uri.is_some() || session.join_url.is_some());
        let session_id = session.id.clone();
        let lifecycle = async {
            let current = super::super::session::session_from_value(api.current_jam().await?)?;
            assert_eq!(current.id, session_id);
            api.set_queue_control(false).await?;
            let denied = super::super::session::session_from_value(api.current_jam().await?)?;
            assert!(denied.queue_only_mode);
            api.set_queue_control(true).await?;
            Result::<(), JamError>::Ok(())
        }
        .await;
        let cleanup = api.end_jam(&session_id).await;
        lifecycle.unwrap();
        cleanup.unwrap();
    }
}
