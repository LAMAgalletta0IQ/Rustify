use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::Client;
use serde_json::json;
use tracing::{debug, trace};

use super::error::JamError;
use super::token::AccessToken;
use super::ClientIdentity;

/// A minted client token and its lifetime.
#[derive(Debug, Clone)]
pub struct ClientToken {
    pub token: String,
    /// Seconds from issue until expiry (mint service value, unadjusted).
    pub expires_in: u64,
    issued_at: Instant,
}

impl ClientToken {
    /// Valid while more than a safety margin (60 s) remains. The mint service
    /// reports lifetimes conservatively, but never trust it to the last second.
    fn is_valid(&self) -> bool {
        self.issued_at.elapsed() < Duration::from_secs(self.expires_in.saturating_sub(60))
    }
}

/// Where the `client-token` comes from.
#[derive(Clone)]
enum Source {
    /// Mint it here, from the JSON `clienttoken` service.
    ///
    /// > This is **best-effort and usually refused** now. The service answers
    /// > a plain JSON mint with a hashcash challenge
    /// > (`response_type: RESPONSE_CHALLENGES_PRESENT`) that has to be solved
    /// > before a grant is issued, and rejects a `client_data` shape that does
    /// > not match the client id — desktop ids expect `connectivity_sdk_data`,
    /// > not the `js_sdk_data` below. Kept for standalone use
    /// > (`examples/jam_demo.rs`); the app uses [`Source::Supplied`].
    Mint,
    /// Use a token the host app supplies — for Rustify, librespot's, which
    /// implements the full protobuf handshake including the challenge.
    Supplied(AccessToken),
}

struct Inner {
    http: Client,
    endpoint: String,
    identity: ClientIdentity,
    source: Source,
    state: Mutex<Option<ClientToken>>,
}

/// Manages Spotify's internal `client-token`.
///
/// Pathfinder and most SpClient endpoints demand a `client-token` request
/// header in addition to the OAuth bearer. It is minted from a generated
/// device id by the internal clienttoken service (default
/// `https://clienttoken.spotify.com/v1/clienttoken`), is short-lived, and is
/// never persisted — a fresh one can always be minted from the same device id.
///
/// The mint endpoint is a known host, but the request/response shapes are
/// internal and can drift; parse errors surface loudly via `JamError` rather
/// than being papered over.
#[derive(Clone)]
pub struct ClientTokenManager {
    inner: Arc<Inner>,
}

impl ClientTokenManager {
    pub fn new(http: Client, endpoint: impl Into<String>) -> Self {
        Self::with_identity(http, endpoint, ClientIdentity::default())
    }

    /// The mint validates `client_id` against Spotify's own registry, so the
    /// identity must carry a real one — an invented value (this used to send
    /// `"rustify-jams"`) is rejected and every jam call then fails on the
    /// header rather than the endpoint.
    pub fn with_identity(http: Client, endpoint: impl Into<String>, identity: ClientIdentity) -> Self {
        Self {
            inner: Arc::new(Inner {
                http,
                endpoint: endpoint.into(),
                identity,
                source: Source::Mint,
                state: Mutex::new(None),
            }),
        }
    }

    /// Reads the token out of a cell the host app keeps current, instead of
    /// minting one.
    ///
    /// Rustify fills that cell from `librespot`'s `SpClient::client_token()`,
    /// which speaks the real protobuf protocol and solves the hashcash
    /// challenge the mint now demands. Doing it here as well would be a second
    /// implementation of a handshake we already have working.
    pub fn supplied(http: Client, identity: ClientIdentity, cell: AccessToken) -> Self {
        Self {
            inner: Arc::new(Inner {
                http,
                endpoint: String::new(),
                identity,
                source: Source::Supplied(cell),
                state: Mutex::new(None),
            }),
        }
    }

    pub fn device_id(&self) -> &str {
        &self.inner.identity.device_id
    }

    /// Returns a usable token, minting one only if the cached copy is absent
    /// or near expiry. Concurrent callers share the cached token. Never holds
    /// the cache lock across an await, so this future stays `Send`.
    pub async fn get_token(&self) -> Result<String, JamError> {
        if let Source::Supplied(cell) = &self.inner.source {
            return <AccessToken as super::token::TokenProvider>::access_token(cell).ok_or_else(|| {
                JamError::ClientTokenExpired(
                    "the host app has not supplied a client-token yet (is the librespot session                      up?)"
                        .into(),
                )
            });
        }
        if let Some(cached) = self.cached_valid() {
            return Ok(cached);
        }
        let fresh = self.fetch_token().await?;
        trace!("client token cached, valid for {}s", fresh.expires_in);
        if let Ok(mut state) = self.inner.state.lock() {
            *state = Some(fresh.clone());
        }
        Ok(fresh.token)
    }

    /// Forces a fresh token from the mint endpoint, bypassing the cache.
    pub async fn fetch_token(&self) -> Result<ClientToken, JamError> {
        let body = json!({
            "client_data": {
                "client_version": self.inner.identity.client_version,
                "client_id": self.inner.identity.client_id,
                "js_sdk_data": {
                    "device_id": self.inner.identity.device_id,
                    "device_brand": "unknown",
                    "device_model": "unknown",
                    "os": "windows",
                    "os_version": "10",
                    "device_type": "computer",
                },
            }
        });

        let resp = self
            .inner
            .http
            .post(&self.inner.endpoint)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            // Not `JamError::Http`: that reads "spclient returned ...", which
            // sends you looking at the jam endpoint when the mint is what
            // failed.
            return Err(JamError::ClientTokenExpired(format!(
                "{} refused the mint request ({status}): {text}",
                self.inner.endpoint
            )));
        }

        let v: serde_json::Value = serde_json::from_str(&text)?;
        // The mint service nests the grant under `granted_token`. If the shape
        // ever changes this fails loudly instead of minting a bogus token.
        let granted = &v["granted_token"];
        let token = granted["token"]
            .as_str()
            .ok_or_else(|| JamError::ClientTokenExpired(format!("unexpected clienttoken response: {text}")))?;
        let expires_in = granted["expires_after_secs"]
            .as_u64()
            .ok_or_else(|| JamError::ClientTokenExpired(format!("clienttoken response missing expiry: {text}")))?;

        debug!("minted client token, expires in {expires_in}s");
        Ok(ClientToken {
            token: token.to_owned(),
            expires_in,
            issued_at: Instant::now(),
        })
    }

    fn cached_valid(&self) -> Option<String> {
        let state = self.inner.state.lock().ok()?;
        state.as_ref().filter(|t| t.is_valid()).map(|t| t.token.clone())
    }
}