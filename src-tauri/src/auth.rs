use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use librespot::core::config::SessionConfig;
use librespot_oauth::{OAuthClientBuilder, OAuthToken};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::state::{AuthState, TokenStore};
use crate::webapi::WebApi;

/// Preferred loopback port for the streaming login. Spotify's own desktop
/// client ID accepts any loopback port, so a busy port here is recoverable.
const STREAMING_PORT: u16 = 8898;

/// Loopback port for the Web API login. Fixed on purpose: a self-registered
/// Spotify app must declare this exact URI in the developer dashboard, so it
/// cannot be chosen at random the way the streaming one can.
const WEBAPI_PORT: u16 = 8899;

/// Scopes for the streaming login, against Spotify's desktop client ID.
///
/// Deliberately the full union, not just `streaming`, because this token is
/// the fallback whenever a private Web API token is unavailable — no client ID
/// configured, or its refresh failing. Narrowing it to the playback scopes made
/// that fallback silently under-privileged: search still worked (it needs no
/// scope) while every library and player call returned a bare 403.
///
/// ncspot requests the same broad set against this client ID for the same
/// reason.
pub const STREAMING_SCOPES: &[&str] = &[
    "streaming",
    "user-read-private",
    "user-read-email",
    "user-read-playback-state",
    "user-modify-playback-state",
    "user-read-currently-playing",
    "user-library-read",
    "user-library-modify",
    "playlist-read-private",
    "playlist-read-collaborative",
    "playlist-modify-private",
    "playlist-modify-public",
    "user-follow-read",
    "user-top-read",
    "user-read-recently-played",
];

/// Scopes for the Web API login. Requested against the app's own client ID
/// when one is configured, so library/search/Connect traffic draws on a
/// private quota instead of the globally shared one.
pub const WEBAPI_SCOPES: &[&str] = &[
    "user-read-private",
    "user-read-email",
    "user-read-playback-state",
    "user-modify-playback-state",
    "user-read-currently-playing",
    "user-library-read",
    "user-library-modify",
    "playlist-read-private",
    "playlist-read-collaborative",
    "playlist-modify-private",
    "playlist-modify-public",
    "user-follow-read",
    "user-top-read",
    "user-read-recently-played",
];

/// Environment variable holding the app's own OAuth client ID, used for Web
/// API traffic only. Normally set in `.env` at the project root; see
/// `.env.example`.
pub const CLIENT_ID_ENV: &str = "RUSTIFY_CLIENT_ID";

/// Fallback Web API client ID baked into the binary, used when
/// `RUSTIFY_CLIENT_ID` is not set in the environment. `.env` is not bundled
/// into a release build, so without this a distributed `.exe` would silently
/// fall back to the shared, globally-pooled librespot quota (see
/// `rate-limiting` in the docs). A Client ID is not a secret: it travels in the clear
/// in every OAuth redirect, and this flow is PKCE with no client secret, so
/// baking it in carries none of the risk a client *secret* would. The
/// environment variable still takes priority, so a packager who wants their
/// own private quota can override this without a rebuild.
const CLIENT_ID_FALLBACK: &str = "f0d03c5ba9204236873f6ff0ffb4a5e6";

/// Client ID for the streaming session.
///
/// Always Spotify's own desktop ID: it is the only one reliably granted the
/// `streaming` scope. Self-registered apps are typically refused that scope,
/// which is why this is not configurable.
pub fn streaming_client_id() -> String {
    SessionConfig::default().client_id
}

/// Client ID for Web API traffic: `RUSTIFY_CLIENT_ID` from the environment if
/// set, else the built-in [`CLIENT_ID_FALLBACK`].
///
/// Always returns a private ID — Web API traffic never has to share the
/// streaming login's globally-pooled quota purely for lack of configuration.
/// Registering your own app at developer.spotify.com and setting the
/// environment variable still overrides the built-in one, e.g. to use a
/// different quota than whoever built this binary.
pub fn webapi_client_id() -> String {
    std::env::var(CLIENT_ID_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| CLIENT_ID_FALLBACK.to_string())
}

fn port_is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn ephemeral_port() -> AppResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    Ok(port)
}

/// Redirect for the streaming login, preferring [`STREAMING_PORT`].
///
/// A stale process from a previous run — or any other librespot client — can
/// hold the preferred port, which used to hang the login outright. The desktop
/// client ID accepts arbitrary loopback ports, so falling back to an ephemeral
/// one is safe.
fn streaming_redirect_uri() -> AppResult<String> {
    if port_is_free(STREAMING_PORT) {
        return Ok(format!("http://127.0.0.1:{STREAMING_PORT}/login"));
    }
    let port = ephemeral_port()?;
    log::warn!("port {STREAMING_PORT} is busy; using {port} for the login redirect");
    Ok(format!("http://127.0.0.1:{port}/login"))
}

/// Redirect for the Web API login. Register this exact string as a redirect URI
/// on the Spotify app whose ID is in [`CLIENT_ID_ENV`].
pub fn webapi_redirect_uri() -> String {
    format!("http://127.0.0.1:{WEBAPI_PORT}/login")
}

/// Persisted between runs so returning users skip the browser round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTokens {
    /// Refresh token for the streaming (desktop client ID) login.
    pub refresh_token: String,
    /// Refresh token for the Web API login. Absent when no private client ID
    /// is configured, and when reading a file written before the split.
    #[serde(default)]
    pub webapi_refresh_token: Option<String>,
}

/// The pair of logins backing one session.
pub struct SessionTokens {
    /// Bearer used to build librespot `Credentials`.
    pub streaming_access: String,
    pub streaming_refresh: String,
    /// Bearer used for every Web API call; the one the refresher keeps alive.
    pub webapi: OAuthToken,
    /// Client ID the Web API token was issued under, so the refresher reuses it.
    pub webapi_client_id: String,
}

impl SessionTokens {
    /// Both roles served by a single login, for when no private client ID is set.
    fn shared(tok: OAuthToken) -> Self {
        Self {
            streaming_access: tok.access_token.clone(),
            streaming_refresh: tok.refresh_token.clone(),
            webapi: tok,
            webapi_client_id: streaming_client_id(),
        }
    }

    pub fn stored(&self) -> StoredTokens {
        StoredTokens {
            refresh_token: self.streaming_refresh.clone(),
            // An empty string must be stored as absent, not as `Some("")`:
            // Spotify rejects a blank refresh token with `invalid_request:
            // refresh_token must be supplied`, which would fail every restore
            // until the file was deleted by hand.
            webapi_refresh_token: if self.is_split() {
                non_empty(&self.webapi.refresh_token)
            } else {
                None
            },
        }
    }

    fn is_split(&self) -> bool {
        self.webapi_client_id != streaming_client_id()
    }
}

/// `Some` only for a non-blank token, so an empty string never reaches the
/// token endpoint as if it were a credential.
fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn tokens_path(data_dir: &Path) -> PathBuf {
    data_dir.join("tokens.json")
}

pub fn load_stored_tokens(data_dir: &Path) -> Option<StoredTokens> {
    let raw = std::fs::read_to_string(tokens_path(data_dir)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Writes the token file, **never erasing a Web API refresh token it does not
/// have a replacement for**.
///
/// A session that fell back to the shared token carries
/// `webapi_refresh_token: None`, and a plain write of that would delete a
/// perfectly valid stored credential. That is exactly what happened in
/// practice: one 429 on `/me` at startup made `restore_login` degrade to the
/// shared token, the ensuing save wiped the private refresh token, and every
/// later launch was stuck on librespot's globally-pooled quota — collecting
/// more 429s, which kept the cycle going. The only way out was a manual
/// re-login, and the next 429 undid it again.
///
/// Deliberate clearing goes through [`clear_stored_tokens`], which removes the
/// file outright, so preserving on `None` here cannot strand a dead token.
pub fn save_stored_tokens(data_dir: &Path, tokens: &StoredTokens) -> AppResult<()> {
    std::fs::create_dir_all(data_dir)?;

    let mut tokens = tokens.clone();
    if tokens.webapi_refresh_token.is_none() {
        if let Some(prev) = load_stored_tokens(data_dir).and_then(|s| s.webapi_refresh_token) {
            log::debug!("keeping the stored web api refresh token; this session has none");
            tokens.webapi_refresh_token = Some(prev);
        }
    }

    let raw = serde_json::to_string(&tokens)
        .map_err(|e| AppError::Other(format!("failed to serialise tokens: {e}")))?;
    std::fs::write(tokens_path(data_dir), raw)?;
    Ok(())
}

pub fn clear_stored_tokens(data_dir: &Path) {
    let _ = std::fs::remove_file(tokens_path(data_dir));
}

fn build_client(
    client_id: &str,
    redirect_uri: &str,
    scopes: &[&str],
) -> AppResult<librespot_oauth::OAuthClient> {
    OAuthClientBuilder::new(client_id, redirect_uri, scopes.to_vec())
        .open_in_browser()
        .with_custom_message("Login successful. You can close this tab and return to the app.")
        .build()
        .map_err(|e| AppError::Auth(e.to_string()))
}

async fn authorize(client_id: &str, redirect_uri: &str, scopes: &[&str]) -> AppResult<OAuthToken> {
    build_client(client_id, redirect_uri, scopes)?
        .get_access_token_async()
        .await
        .map_err(|e| AppError::Auth(e.to_string()))
}

async fn refresh(
    client_id: &str,
    redirect_uri: &str,
    scopes: &[&str],
    refresh_token: &str,
) -> AppResult<OAuthToken> {
    build_client(client_id, redirect_uri, scopes)?
        .refresh_token_async(refresh_token)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))
}

/// Interactive login: opens the system browser and waits for the loopback
/// redirect.
///
/// Runs **two** authorizations, the way ncspot does — one against Spotify's
/// desktop ID for `streaming`, one against [`webapi_client_id`] (the
/// environment override, or the built-in fallback) for Web API access.
pub async fn interactive_login() -> AppResult<SessionTokens> {
    let streaming = authorize(
        &streaming_client_id(),
        &streaming_redirect_uri()?,
        STREAMING_SCOPES,
    )
    .await?;

    let id = webapi_client_id();
    log::info!("second authorization for Web API access under a private client ID");
    let webapi = authorize(&id, &webapi_redirect_uri(), WEBAPI_SCOPES).await?;

    Ok(SessionTokens {
        streaming_access: streaming.access_token,
        streaming_refresh: streaming.refresh_token,
        webapi,
        webapi_client_id: id,
    })
}

/// Silent login from stored refresh tokens. The caller falls back to
/// [`interactive_login`] if this fails (token revoked, scopes changed, etc.).
pub async fn restore_login(stored: &StoredTokens) -> AppResult<SessionTokens> {
    let streaming = refresh(
        &streaming_client_id(),
        &streaming_redirect_uri()?,
        STREAMING_SCOPES,
        &stored.refresh_token,
    )
    .await?;

    // A stored token for the current Web API client ID is required; if the ID
    // changed since the last run (env var added/edited) there is nothing to
    // refresh yet under it, so fall back to the shared token rather than
    // failing the restore. `and_then(non_empty)` guards files written before
    // the check above, which may hold `""` rather than a real token.
    let webapi_rt = stored
        .webapi_refresh_token
        .as_deref()
        .and_then(non_empty);
    let id = webapi_client_id();

    match webapi_rt.as_deref() {
        Some(rt) => {
            match refresh(&id, &webapi_redirect_uri(), WEBAPI_SCOPES, rt).await {
                Ok(mut webapi) => {
                    // Spotify only returns `refresh_token` when it rotates one.
                    // An omitted field means "keep using the one you have", not
                    // "you no longer have one" — but the empty string reads as
                    // the latter, which made `stored()` report the session as
                    // having no Web API credential at all.
                    if webapi.refresh_token.trim().is_empty() {
                        webapi.refresh_token = rt.to_string();
                    }
                    Ok(SessionTokens {
                        streaming_access: streaming.access_token,
                        streaming_refresh: streaming.refresh_token,
                        webapi,
                        webapi_client_id: id,
                    })
                }
                // Must NOT propagate: the streaming refresh above already
                // succeeded, and Spotify rotates refresh tokens on use — so
                // the stored streaming token is now dead and the replacement
                // is only in `streaming`. Failing here would drop it on the
                // floor and lock the user out on the next launch. Degrade to
                // the shared token instead; the caller persists the rotation.
                Err(e) => {
                    log::warn!("web api token refresh failed, falling back to the shared token: {e}");
                    Ok(SessionTokens::shared(streaming))
                }
            }
        }
        None => {
            log::warn!(
                "no Web API token stored for the current client ID; using the shared token \
                 for this session. Log out and back in to split the quota."
            );
            Ok(SessionTokens::shared(streaming))
        }
    }
}

/// Whether Spotify actively rejected the grant, as opposed to the request
/// merely failing to complete.
///
/// `invalid_grant` is the OAuth code for "this refresh token is dead" and is
/// the only signal that justifies deleting stored credentials. Anything else —
/// DNS not up yet, TLS failure, 429, 5xx — is transient and must leave the
/// tokens alone, or a flaky moment at startup costs the user their login.
pub fn is_grant_rejected(e: &AppError) -> bool {
    match e {
        AppError::Auth(msg) => {
            let m = msg.to_ascii_lowercase();
            m.contains("invalid_grant") || m.contains("invalid_client")
        }
        _ => false,
    }
}

/// Refresh this long before expiry, so a slow request cannot leave a window
/// where the token is already dead.
const REFRESH_MARGIN: Duration = Duration::from_secs(5 * 60);
/// Floor on the sleep, so a short-lived or already-expired token cannot spin.
const MIN_REFRESH_DELAY: Duration = Duration::from_secs(30);

/// Keeps the Web API bearer token alive for as long as the session lasts.
///
/// Only the *Web API* token is refreshed here: librespot's `Session` maintains
/// its own connection and internal token provider once connected, so it does
/// not need re-authenticating on this schedule.
pub fn spawn_refresher(
    client_id: String,
    mut stored: StoredTokens,
    tokens: TokenStore,
    data_dir: PathBuf,
    mut expires_at: Instant,
) -> tauri::async_runtime::JoinHandle<()> {
    // Whichever token this refresher owns: the private one when split, else
    // the shared streaming one.
    let mut refresh_token = stored
        .webapi_refresh_token
        .clone()
        .unwrap_or_else(|| stored.refresh_token.clone());
    let split = stored.webapi_refresh_token.is_some();

    let redirect_uri = if split {
        webapi_redirect_uri()
    } else {
        // Recomputed per refresh below would be better, but a refresh does not
        // actually bind the port — it is only echoed to the token endpoint.
        format!("http://127.0.0.1:{STREAMING_PORT}/login")
    };
    let scopes: &[&str] = if split { WEBAPI_SCOPES } else { STREAMING_SCOPES };

    tauri::async_runtime::spawn(async move {
        loop {
            let delay = expires_at
                .saturating_duration_since(Instant::now())
                .saturating_sub(REFRESH_MARGIN)
                .max(MIN_REFRESH_DELAY);

            tokio::time::sleep(delay).await;

            match refresh(&client_id, &redirect_uri, scopes, &refresh_token).await {
                Ok(tok) => {
                    tokens.set(tok.access_token).await;
                    expires_at = tok.expires_at;

                    // Spotify may hand back a new refresh token; if so the old
                    // one stops working, so persist the replacement.
                    if !tok.refresh_token.is_empty() && tok.refresh_token != refresh_token {
                        refresh_token = tok.refresh_token;
                        if split {
                            stored.webapi_refresh_token = Some(refresh_token.clone());
                        } else {
                            stored.refresh_token = refresh_token.clone();
                        }
                        if let Err(e) = save_stored_tokens(&data_dir, &stored) {
                            log::warn!("could not persist rotated refresh token: {e}");
                        }
                    }
                    log::info!("web api token refreshed");
                }
                Err(e) => {
                    // Transient network failure is likely; retry on the floor
                    // rather than killing the session.
                    log::warn!("token refresh failed, retrying shortly: {e}");
                    expires_at = Instant::now() + REFRESH_MARGIN + MIN_REFRESH_DELAY;
                }
            }
        }
    })
}

#[derive(Debug, Deserialize)]
struct MeResponse {
    id: String,
    display_name: Option<String>,
    /// "premium" | "free" | "open"
    product: Option<String>,
    images: Option<Vec<ImageObj>>,
}

#[derive(Debug, Deserialize)]
struct ImageObj {
    url: String,
}

/// Fetches the profile and enforces the Premium requirement.
///
/// This is a plain read of the account's own `product` field, done *before*
/// starting playback so a free-tier user gets a clear message instead of a
/// silent failure deep inside librespot. It is not a bypass of anything.
pub async fn fetch_profile_require_premium(api: &WebApi, token: &str) -> AppResult<AuthState> {
    let me: MeResponse = api.get(token, "/me", &[]).await?;

    let product = me.product.clone().unwrap_or_else(|| "unknown".into());
    if product != "premium" {
        return Err(AppError::PremiumRequired(product));
    }

    Ok(AuthState {
        logged_in: true,
        display_name: me.display_name,
        user_id: Some(me.id),
        product: Some(product),
        avatar_url: me
            .images
            .and_then(|imgs| imgs.into_iter().next())
            .map(|i| i.url),
    })
}
