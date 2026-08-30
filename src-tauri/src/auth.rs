use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use librespot::core::config::SessionConfig;
use librespot_oauth::{OAuthClientBuilder, OAuthToken};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::audio::{EqualizerSettings, StreamQuality};
use crate::error::{AppError, AppResult};
use crate::state::{events, AppState, AuthState, PlaybackState, TokenStore};
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
    "user-follow-modify",
    "user-top-read",
    "user-read-recently-played",
    // Required only by `PUT /playlists/{id}/images`. Spotify treats it as a
    // separate grant from playlist-modify-*, so editing a playlist's name
    // succeeds while replacing its cover 403s without it. Tokens minted
    // before this was added lack it until the next interactive login —
    // `library::update_playlist_image` says so in its error rather than
    // reporting a bare "Forbidden".
    "ugc-image-upload",
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
    "user-follow-modify",
    "user-top-read",
    "user-read-recently-played",
    "ugc-image-upload",
];

/// Environment variable holding the app's own OAuth client ID, used for Web
/// API traffic only. Normally set in `.env` at the project root; see
/// `.env.example`.
pub const CLIENT_ID_ENV: &str = "RUSTIFY_CLIENT_ID";

const DEVICE_AUTHORIZATION_URL: &str = "https://accounts.spotify.com/oauth2/device/authorize";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// Client ID for the streaming session.
///
/// Always Spotify's own desktop ID: it is the only one reliably granted the
/// `streaming` scope. Self-registered apps are typically refused that scope,
/// which is why this is not configurable.
pub fn streaming_client_id() -> String {
    SessionConfig::default().client_id
}

/// Client ID for Web API traffic: `RUSTIFY_CLIENT_ID` from the environment if
/// set (a packager/dev override — see `.env.example`), else whatever the user
/// saved through the first-run Setup screen, else an error.
///
/// There is deliberately no built-in fallback. Baking one in meant every user
/// who skipped configuration shared *the developer's* quota, which does not
/// scale past one person running the app — see `set_client_id` in
/// `commands.rs`, which the UI calls before any login is attempted.
pub fn webapi_client_id(data_dir: &Path) -> AppResult<String> {
    if let Ok(v) = std::env::var(CLIENT_ID_ENV) {
        let v = v.trim();
        if !v.is_empty() {
            return Ok(v.to_string());
        }
    }
    load_settings(data_dir)
        .and_then(|s| s.webapi_client_id)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Other("no Spotify Client ID configured".to_string()))
}

/// The Last.fm API key, if the user configured one.
///
/// Unlike `webapi_client_id` there is no environment override and no error
/// case: Last.fm is entirely optional, so "not configured" is an ordinary
/// answer, not a failure the caller has to handle.
pub fn lastfm_api_key(data_dir: &Path) -> Option<String> {
    load_settings(data_dir)
        .and_then(|s| s.lastfm_api_key)
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

/// Persisted app settings, distinct from `tokens.json`: this survives logout,
/// since the Client ID belongs to the Spotify app the user registered, not to
/// any one login session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub webapi_client_id: Option<String>,
    /// Optional Last.fm API key, used only to enrich the Listening DNA profile
    /// with community genre tags. Absent is the normal case and everything
    /// works without it — see `lastfm.rs`. Kept here rather than in
    /// `AppSettings` for the same reason as the Client ID: it identifies a
    /// third-party app registration, not a preference, and must survive
    /// logout.
    #[serde(default)]
    pub lastfm_api_key: Option<String>,
    /// Last volume chosen in the player. This is deliberately not exposed as
    /// a Settings control: the player volume is the one authoritative volume
    /// control, while this value only restores it on the next local session.
    #[serde(default = "default_volume_percent", alias = "default_volume_percent")]
    pub last_volume_percent: u8,
    #[serde(default)]
    pub reduce_motion: bool,
    #[serde(default = "default_cache_limit_mb")]
    pub cache_limit_mb: u32,
    #[serde(default)]
    pub audio_quality: StreamQuality,
    /// Decoder-level equal-power overlap. Spotify's clients expose a maximum
    /// of twelve seconds, which is also the range supported by the pinned
    /// librespot crossfade implementation.
    #[serde(default)]
    pub crossfade_seconds: u8,
    /// None follows the operating-system default. Names are the stable handle
    /// exposed by CPAL 0.16/rodio 0.21 on this pinned stack.
    #[serde(default)]
    pub output_device: Option<String>,
    #[serde(default)]
    pub equalizer: EqualizerSettings,
    /// Whether the collapsible Friend Activity rail is open. Defaults to
    /// `true` to match the rail's previous always-visible behaviour on Home.
    #[serde(default = "default_friends_panel_open")]
    pub friends_panel_open: bool,
}

const fn default_friends_panel_open() -> bool {
    true
}

const fn default_volume_percent() -> u8 {
    50
}

const fn default_cache_limit_mb() -> u32 {
    2048
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            webapi_client_id: None,
            lastfm_api_key: None,
            last_volume_percent: default_volume_percent(),
            reduce_motion: false,
            cache_limit_mb: default_cache_limit_mb(),
            audio_quality: StreamQuality::default(),
            crossfade_seconds: 0,
            output_device: None,
            equalizer: EqualizerSettings::default(),
            friends_panel_open: default_friends_panel_open(),
        }
    }
}

fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

pub fn load_settings(data_dir: &Path) -> Option<Settings> {
    let raw = std::fs::read_to_string(settings_path(data_dir)).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn settings_or_default(data_dir: &Path) -> Settings {
    load_settings(data_dir).unwrap_or_default()
}

pub fn save_settings(data_dir: &Path, settings: &Settings) -> AppResult<()> {
    std::fs::create_dir_all(data_dir)?;
    let raw = serde_json::to_string(settings)
        .map_err(|e| AppError::Other(format!("failed to serialise settings: {e}")))?;
    std::fs::write(settings_path(data_dir), raw)?;
    Ok(())
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

/// Public half of an RFC 8628 pairing request. The device code itself remains
/// backend-only; it is a short-lived credential and the webview never needs it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthorization {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub url: String,
    pub expires_in: u64,
    pub expires_at_ms: u64,
    pub interval: u64,
}

struct PendingDeviceAuthorization {
    public: DeviceAuthorization,
    device_code: String,
    client_id: String,
    scopes: Vec<String>,
    started: Instant,
    cancelled: AtomicBool,
    polling: AtomicBool,
    wake: tokio::sync::Notify,
}

/// One pending pairing per application. Starting or cancelling a flow signals
/// an in-flight poller without exposing its device code to command arguments.
#[derive(Default)]
pub struct DeviceAuthStore {
    pending: tokio::sync::RwLock<Option<Arc<PendingDeviceAuthorization>>>,
}

impl DeviceAuthStore {
    pub async fn cancel(&self) {
        if let Some(pending) = self.pending.write().await.take() {
            pending.cancelled.store(true, Ordering::Release);
            // One completion poller is permitted; notify_one stores a permit
            // even if cancellation wins the tiny race before `notified()` is
            // registered.
            pending.wake.notify_one();
        }
    }

    async fn replace(&self, pending: Arc<PendingDeviceAuthorization>) {
        self.cancel().await;
        self.pending.write().await.replace(pending);
    }

    async fn current(&self) -> AppResult<Arc<PendingDeviceAuthorization>> {
        self.pending
            .read()
            .await
            .clone()
            .ok_or_else(|| AppError::BadRequest("no device authorization is pending".to_string()))
    }

    async fn clear_if(&self, completed: &Arc<PendingDeviceAuthorization>) {
        let mut guard = self.pending.write().await;
        if guard
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, completed))
        {
            guard.take();
        }
    }
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    #[serde(default = "default_device_poll_interval")]
    interval: u64,
}

const fn default_device_poll_interval() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default = "default_token_lifetime")]
    expires_in: u64,
    #[serde(default = "default_token_type")]
    token_type: String,
    scope: Option<String>,
}

const fn default_token_lifetime() -> u64 {
    3600
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

#[derive(Debug, Deserialize)]
struct DeviceOAuthError {
    error: String,
    error_description: Option<String>,
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

/// Windows DPAPI wrapping for `tokens.json`, bound to the current Windows
/// user profile — no `CRYPTPROTECT_LOCAL_MACHINE` flag, so a copy of the file
/// on another machine, or read under another account on this one, cannot be
/// decrypted. Deliberately kept as a private submodule of `auth.rs`: nothing
/// outside this file needs to know tokens are encrypted at rest, and the
/// blob format (raw DPAPI bytes, no envelope) is an implementation detail of
/// [`load_stored_tokens`]/[`save_stored_tokens`].
mod dpapi {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    /// Encrypts `data`. `None` on any Windows API failure — callers treat
    /// "could not protect" as a reason to fall back to plaintext, never as a
    /// panic; DPAPI has no documented failure mode for an ordinary
    /// interactive user session, but this is credential storage, not a place
    /// to assume that.
    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(data.len()).ok()?,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        unsafe {
            CryptProtectData(
                &input,
                windows::core::PCWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
            .ok()?;
        }
        Some(take_blob(output))
    }

    /// Decrypts a blob previously produced by [`protect`]. `None` on any
    /// failure — a corrupted file, one from another user account, and one
    /// from another machine all degrade the same way: no stored tokens,
    /// never a panic.
    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(data.len()).ok()?,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        unsafe {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
            .ok()?;
        }
        Some(take_blob(output))
    }

    /// Copies a DPAPI output blob into an owned `Vec` and frees the buffer
    /// Windows allocated for it. Both `CryptProtectData` and
    /// `CryptUnprotectData` require the caller to `LocalFree` `pbData`.
    fn take_blob(blob: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let bytes = if blob.pbData.is_null() || blob.cbData == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(blob.pbData, blob.cbData as usize) }.to_vec()
        };
        if !blob.pbData.is_null() {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(blob.pbData as *mut _)));
            }
        }
        bytes
    }
}

/// Serialises and DPAPI-encrypts `tokens`, writing the ciphertext to
/// `tokens.json`. Falls back to plaintext (logging a warning) if DPAPI
/// itself fails, so a Windows API hiccup degrades to the pre-encryption
/// behaviour rather than losing the session.
fn write_protected(data_dir: &Path, tokens: &StoredTokens) -> AppResult<()> {
    std::fs::create_dir_all(data_dir)?;
    let plaintext = serde_json::to_vec(tokens)
        .map_err(|e| AppError::Other(format!("failed to serialise tokens: {e}")))?;
    match dpapi::protect(&plaintext) {
        Some(ciphertext) => std::fs::write(tokens_path(data_dir), ciphertext)?,
        None => {
            log::warn!("DPAPI protect failed; writing tokens.json as plaintext");
            std::fs::write(tokens_path(data_dir), plaintext)?;
        }
    }
    Ok(())
}

/// Loads and decrypts `tokens.json`, migrating a pre-encryption plaintext
/// file in place.
///
/// Tries DPAPI first, since that is the format every write after this change
/// produces. On failure — which includes "this file is still the old
/// plaintext format" — falls back to parsing the raw bytes as JSON directly.
/// A successful fallback parse re-saves the file encrypted immediately, so an
/// existing install migrates silently on the next read with no forced
/// re-login; a failed fallback parse means the file is genuinely corrupted or
/// foreign, and this returns `None` exactly as it would for a missing file,
/// never a panic.
pub fn load_stored_tokens(data_dir: &Path) -> Option<StoredTokens> {
    let raw = std::fs::read(tokens_path(data_dir)).ok()?;

    if let Some(plaintext) = dpapi::unprotect(&raw) {
        return serde_json::from_slice(&plaintext).ok();
    }

    let tokens: StoredTokens = serde_json::from_slice(&raw).ok()?;
    log::info!("migrating tokens.json from plaintext to DPAPI-encrypted storage");
    if let Err(e) = write_protected(data_dir, &tokens) {
        log::warn!("could not re-save tokens.json encrypted; still usable as plaintext: {e}");
    }
    Some(tokens)
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

    write_protected(data_dir, &tokens)
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

fn device_http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| AppError::Auth(format!("build device authorization client: {error}")))
}

fn oauth_error(status: reqwest::StatusCode, body: &[u8], stage: &str) -> AppError {
    let parsed = serde_json::from_slice::<DeviceOAuthError>(body).ok();
    let (code, description) = parsed.map_or_else(
        || {
            (
                "unknown_error".to_string(),
                format!("HTTP {}", status.as_u16()),
            )
        },
        |error| {
            let description = error
                .error_description
                .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
            (error.error, description)
        },
    );
    if code == "unauthorized_client" {
        AppError::EndpointNotAvailable(format!(
            "Spotify device authorization is not enabled for this client ({description})"
        ))
    } else {
        AppError::Auth(format!(
            "device authorization {stage}: {code} ({description})"
        ))
    }
}

/// Starts Spotify's RFC 8628 device authorization flow. The known streaming
/// client is used because ordinary dashboard client IDs are commonly rejected
/// with `unauthorized_client`; this capability is probed by the request itself.
pub async fn start_device_authorization(store: &DeviceAuthStore) -> AppResult<DeviceAuthorization> {
    let client_id = streaming_client_id();
    let scope = STREAMING_SCOPES.join(" ");
    let response = device_http_client()?
        .post(DEVICE_AUTHORIZATION_URL)
        .form(&[("client_id", client_id.as_str()), ("scope", scope.as_str())])
        .send()
        .await?;
    let status = response.status();
    let body = response.bytes().await?;
    if !status.is_success() {
        return Err(oauth_error(status, &body, "request failed"));
    }
    let wire: DeviceCodeResponse = serde_json::from_slice(&body).map_err(|error| {
        AppError::Auth(format!("invalid device authorization response: {error}"))
    })?;
    if wire.device_code.trim().is_empty()
        || !valid_user_code(&wire.user_code)
        || wire.expires_in == 0
        || wire.interval == 0
    {
        return Err(AppError::Auth(
            "device authorization response contained invalid required fields".to_string(),
        ));
    }
    validate_pairing_url(&wire.verification_uri)?;
    if let Some(url) = wire.verification_uri_complete.as_deref() {
        validate_pairing_url(url)?;
    }
    let url = wire
        .verification_uri_complete
        .clone()
        .unwrap_or_else(|| wire.verification_uri.clone());
    let public = DeviceAuthorization {
        user_code: wire.user_code,
        verification_uri: wire.verification_uri,
        verification_uri_complete: wire.verification_uri_complete,
        url,
        expires_in: wire.expires_in,
        expires_at_ms: unix_time_ms().saturating_add(wire.expires_in.saturating_mul(1_000)),
        interval: wire.interval,
    };
    let pending = Arc::new(PendingDeviceAuthorization {
        public: public.clone(),
        device_code: wire.device_code,
        client_id,
        scopes: STREAMING_SCOPES
            .iter()
            .map(|scope| (*scope).to_string())
            .collect(),
        started: Instant::now(),
        cancelled: AtomicBool::new(false),
        polling: AtomicBool::new(false),
        wake: tokio::sync::Notify::new(),
    });
    store.replace(pending).await;
    Ok(public)
}

fn valid_user_code(code: &str) -> bool {
    (4..=32).contains(&code.len())
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn validate_pairing_url(raw: &str) -> AppResult<()> {
    let url = reqwest::Url::parse(raw)
        .map_err(|_| AppError::Auth("Spotify returned an invalid pairing URL".into()))?;
    let trusted_host = url
        .host_str()
        .is_some_and(|host| host == "spotify.com" || host.ends_with(".spotify.com"));
    if url.scheme() != "https"
        || !trusted_host
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AppError::Auth(
            "Spotify returned an untrusted pairing URL".into(),
        ));
    }
    Ok(())
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().min(u64::MAX as u128) as u64
        })
}

enum PollResponse {
    Pending,
    SlowDown,
    Token(DeviceTokenResponse),
}

fn decode_poll_response(status: reqwest::StatusCode, body: &[u8]) -> AppResult<PollResponse> {
    if status.is_success() {
        let token = serde_json::from_slice(body)
            .map_err(|error| AppError::Auth(format!("invalid device token response: {error}")))?;
        return Ok(PollResponse::Token(token));
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return Ok(PollResponse::SlowDown);
    }
    let parsed = serde_json::from_slice::<DeviceOAuthError>(body)
        .map_err(|_| oauth_error(status, body, "token exchange failed"))?;
    match parsed.error.as_str() {
        "authorization_pending" => Ok(PollResponse::Pending),
        "slow_down" => Ok(PollResponse::SlowDown),
        _ => Err(oauth_error(status, body, "token exchange failed")),
    }
}

async fn poll_device_token(pending: &PendingDeviceAuthorization) -> AppResult<OAuthToken> {
    let client = device_http_client()?;
    let mut interval = Duration::from_secs(pending.public.interval);
    loop {
        // RFC 8628 says not to exceed the server's interval. Waiting before
        // the first request also gives the UI time to present the code and
        // avoids an unnecessary guaranteed `authorization_pending` response.
        if pending.cancelled.load(Ordering::Acquire) {
            return Err(AppError::Auth(
                "device authorization was cancelled".to_string(),
            ));
        }
        if pending.started.elapsed() >= Duration::from_secs(pending.public.expires_in) {
            return Err(AppError::Auth(
                "device authorization code expired".to_string(),
            ));
        }
        let remaining = Duration::from_secs(pending.public.expires_in)
            .saturating_sub(pending.started.elapsed());
        tokio::select! {
            _ = tokio::time::sleep(interval.min(remaining)) => {}
            _ = pending.wake.notified() => {}
        }
        if pending.cancelled.load(Ordering::Acquire) {
            return Err(AppError::Auth(
                "device authorization was cancelled".to_string(),
            ));
        }
        if pending.started.elapsed() >= Duration::from_secs(pending.public.expires_in) {
            return Err(AppError::Auth(
                "device authorization code expired".to_string(),
            ));
        }

        let response = client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", pending.client_id.as_str()),
                ("grant_type", DEVICE_GRANT_TYPE),
                ("device_code", pending.device_code.as_str()),
            ])
            .send()
            .await;
        match response {
            Ok(response) => {
                let status = response.status();
                let body = response.bytes().await?;
                match decode_poll_response(status, &body)? {
                    PollResponse::Pending => {}
                    PollResponse::SlowDown => {
                        interval = interval.saturating_add(Duration::from_secs(5));
                    }
                    PollResponse::Token(token) => {
                        if token.access_token.trim().is_empty()
                            || token.refresh_token.trim().is_empty()
                            || token.expires_in == 0
                            || !token.token_type.eq_ignore_ascii_case("bearer")
                        {
                            return Err(AppError::Auth(
                                "device token response contained invalid token fields".to_string(),
                            ));
                        }
                        let scopes = token.scope.map_or_else(
                            || pending.scopes.clone(),
                            |scope| scope.split_whitespace().map(str::to_string).collect(),
                        );
                        if !scopes.iter().any(|scope| scope == "streaming") {
                            return Err(AppError::Auth(
                                "device authorization did not grant Spotify streaming".into(),
                            ));
                        }
                        return Ok(OAuthToken {
                            access_token: token.access_token,
                            refresh_token: token.refresh_token,
                            expires_at: Instant::now() + Duration::from_secs(token.expires_in),
                            token_type: token.token_type,
                            scopes,
                        });
                    }
                }
            }
            Err(error) => {
                // A transient network failure must not throw away a still-valid
                // pairing. The next interval retries; no token or code is logged.
                log::debug!("spotify.auth: device token poll failed transiently: {error}");
            }
        }
    }
}

pub async fn complete_device_authorization(store: &DeviceAuthStore) -> AppResult<SessionTokens> {
    let pending = store.current().await?;
    if pending.polling.swap(true, Ordering::AcqRel) {
        return Err(AppError::BadRequest(
            "device authorization is already being completed".into(),
        ));
    }
    let result = poll_device_token(&pending).await.map(SessionTokens::shared);
    store.clear_if(&pending).await;
    result
}

/// Interactive login: opens the system browser and waits for the loopback
/// redirect.
///
/// Runs **two** authorizations, the way ncspot does — one against Spotify's
/// desktop ID for `streaming`, one against [`webapi_client_id`] (the
/// environment override, or the ID saved through Setup) for Web API access.
pub async fn interactive_login(data_dir: &Path) -> AppResult<SessionTokens> {
    let streaming = authorize(
        &streaming_client_id(),
        &streaming_redirect_uri()?,
        STREAMING_SCOPES,
    )
    .await?;

    let id = webapi_client_id(data_dir)?;
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
pub async fn restore_login(stored: &StoredTokens, data_dir: &Path) -> AppResult<SessionTokens> {
    let streaming = refresh(
        &streaming_client_id(),
        &streaming_redirect_uri()?,
        STREAMING_SCOPES,
        &stored.refresh_token,
    )
    .await?;

    // A stored token for the current Web API client ID is required; if the ID
    // changed since the last run (env var added/edited, or Setup re-run) there
    // is nothing to refresh yet under it, so fall back to the shared token
    // rather than failing the restore. `and_then(non_empty)` guards files
    // written before the check above, which may hold `""` rather than a real
    // token.
    let webapi_rt = stored.webapi_refresh_token.as_deref().and_then(non_empty);
    let Ok(id) = webapi_client_id(data_dir) else {
        return Ok(SessionTokens::shared(streaming));
    };

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
                    log::warn!(
                        "web api token refresh failed, falling back to the shared token: {e}"
                    );
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
    app: tauri::AppHandle,
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
    let scopes: &[&str] = if split {
        WEBAPI_SCOPES
    } else {
        STREAMING_SCOPES
    };

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
                    if is_grant_rejected(&e) {
                        // Refresh tokens now expire after six months. This is
                        // terminal, not a transient network failure: clear the
                        // rejected credential and move the whole app to the
                        // logged-out state instead of retrying forever.
                        log::warn!("web api refresh grant expired; ending the session: {e}");
                        clear_stored_tokens(&data_dir);
                        tokens.set(String::new()).await;
                        let state = app.state::<AppState>();
                        if let Some(session) = state.spotify.write().await.take() {
                            let _ = session.spirc.shutdown();
                            session.remote_task.abort();
                            session.connect_state_task.abort();
                            // `session.refresh_task` is this task. Dropping its
                            // handle detaches it, and returning below ends it.
                        }
                        state.sleep_timer.cancel(None).await;
                        *state.auth.write().await = AuthState::default();
                        *state.playback.write().await = PlaybackState::default();
                        let _ = app.emit(events::AUTH, AuthState::default());
                        return;
                    }
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

    // Spotify removed `product` from `/me` for newer Development Mode apps in
    // February 2026. Enforce Premium when the field is present; when omitted,
    // let librespot perform the authoritative streaming capability check.
    if let Some(product) = me.product.as_deref() {
        if product != "premium" {
            return Err(AppError::PremiumRequired(product.to_string()));
        }
    }

    Ok(AuthState {
        logged_in: true,
        display_name: me.display_name,
        user_id: Some(me.id),
        product: me.product,
        avatar_url: me
            .images
            .and_then(|imgs| imgs.into_iter().next())
            .map(|i| i.url),
    })
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn old_settings_files_receive_functional_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"webapi_client_id":"client"}"#).unwrap();
        assert_eq!(settings.webapi_client_id.as_deref(), Some("client"));
        assert_eq!(settings.last_volume_percent, 50);
        assert_eq!(settings.cache_limit_mb, 2048);
        assert_eq!(settings.crossfade_seconds, 0);
        assert!(!settings.reduce_motion);
        // A settings.json written before the friends panel existed must not
        // silently hide it — the rail was always visible until this toggle
        // existed, so an absent field means "open", not "closed".
        assert!(settings.friends_panel_open);
    }

    #[test]
    fn legacy_default_volume_migrates_to_last_player_volume() {
        let settings: Settings = serde_json::from_str(r#"{"default_volume_percent":73}"#).unwrap();
        assert_eq!(settings.last_volume_percent, 73);
        let serialized = serde_json::to_string(&settings).unwrap();
        assert!(serialized.contains("last_volume_percent"));
        assert!(!serialized.contains("default_volume_percent"));
    }

    #[test]
    fn device_poll_respects_pending_and_slow_down() {
        assert!(matches!(
            decode_poll_response(
                reqwest::StatusCode::BAD_REQUEST,
                br#"{"error":"authorization_pending"}"#,
            ),
            Ok(PollResponse::Pending)
        ));
        assert!(matches!(
            decode_poll_response(
                reqwest::StatusCode::BAD_REQUEST,
                br#"{"error":"slow_down"}"#,
            ),
            Ok(PollResponse::SlowDown)
        ));
    }

    #[test]
    fn device_token_and_public_view_never_mix_credentials() {
        let response = decode_poll_response(
            reqwest::StatusCode::OK,
            br#"{"access_token":"access","refresh_token":"refresh","expires_in":3600,"token_type":"Bearer","scope":"streaming user-read-private"}"#,
        )
        .expect("valid response");
        let PollResponse::Token(token) = response else {
            panic!("expected token")
        };
        assert_eq!(token.scope.as_deref(), Some("streaming user-read-private"));

        let public = DeviceAuthorization {
            user_code: "ABCD-EFGH".to_string(),
            verification_uri: "https://spotify.com/pair".to_string(),
            verification_uri_complete: None,
            url: "https://spotify.com/pair".to_string(),
            expires_in: 600,
            expires_at_ms: 1_000_000,
            interval: 5,
        };
        let json = serde_json::to_string(&public).expect("serialize public view");
        assert!(!json.contains("device_code"));
        assert!(!json.contains("access"));
        assert!(!json.contains("refresh"));
    }

    #[test]
    fn validates_pairing_urls_and_codes() {
        assert!(validate_pairing_url("https://spotify.com/pair").is_ok());
        assert!(validate_pairing_url("https://accounts.spotify.com/pair?code=ABCD").is_ok());
        assert!(validate_pairing_url("http://spotify.com/pair").is_err());
        assert!(validate_pairing_url("https://spotify.com.evil.test/pair").is_err());
        assert!(validate_pairing_url("https://spotify.com@evil.test/pair").is_err());
        assert!(valid_user_code("ABCD-EFGH"));
        assert!(!valid_user_code("<script>"));
    }

    #[tokio::test]
    async fn cancellation_wakes_poll_immediately_and_duplicate_poll_is_rejected() {
        let store = DeviceAuthStore::default();
        let pending = Arc::new(PendingDeviceAuthorization {
            public: DeviceAuthorization {
                user_code: "ABCD-EFGH".into(),
                verification_uri: "https://spotify.com/pair".into(),
                verification_uri_complete: None,
                url: "https://spotify.com/pair".into(),
                expires_in: 600,
                expires_at_ms: unix_time_ms() + 600_000,
                interval: 300,
            },
            device_code: "backend-only".into(),
            client_id: "client".into(),
            scopes: vec!["streaming".into()],
            started: Instant::now(),
            cancelled: AtomicBool::new(false),
            polling: AtomicBool::new(false),
            wake: tokio::sync::Notify::new(),
        });
        store.replace(pending).await;

        let first = complete_device_authorization(&store);
        tokio::pin!(first);
        tokio::select! {
            _ = &mut first => panic!("long poll unexpectedly completed"),
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
        }
        assert!(matches!(
            complete_device_authorization(&store).await,
            Err(AppError::BadRequest(_))
        ));
        store.cancel().await;
        let cancelled = tokio::time::timeout(Duration::from_millis(100), first)
            .await
            .expect("cancellation should wake the poll");
        assert!(matches!(cancelled, Err(AppError::Auth(message)) if message.contains("cancelled")));
    }

    #[tokio::test]
    #[ignore = "live Spotify device authorization capability probe"]
    async fn requests_live_device_code() {
        let store = DeviceAuthStore::default();
        let auth = start_device_authorization(&store)
            .await
            .expect("device authorization endpoint");
        assert!(!auth.user_code.is_empty());
        assert!(auth.url.starts_with("https://"));
        store.cancel().await;
    }
}

#[cfg(test)]
mod token_persistence_tests {
    use super::*;

    /// Regression test for the self-sustaining 429 loop documented on
    /// [`save_stored_tokens`]: a degraded session with no Web API refresh
    /// token must not erase a previously-stored one.
    #[test]
    fn writing_none_preserves_the_previously_stored_webapi_token() {
        let dir = tempfile::tempdir().unwrap();
        save_stored_tokens(
            dir.path(),
            &StoredTokens {
                refresh_token: "streaming-1".into(),
                webapi_refresh_token: Some("private-refresh".into()),
            },
        )
        .unwrap();

        // A later session degrades to the shared token and has nothing of
        // its own to persist for the Web API side.
        save_stored_tokens(
            dir.path(),
            &StoredTokens {
                refresh_token: "streaming-2".into(),
                webapi_refresh_token: None,
            },
        )
        .unwrap();

        let stored = load_stored_tokens(dir.path()).unwrap();
        assert_eq!(stored.refresh_token, "streaming-2");
        assert_eq!(stored.webapi_refresh_token.as_deref(), Some("private-refresh"));
    }

    #[test]
    fn writing_some_overwrites_the_stored_webapi_token() {
        let dir = tempfile::tempdir().unwrap();
        save_stored_tokens(
            dir.path(),
            &StoredTokens {
                refresh_token: "streaming-1".into(),
                webapi_refresh_token: Some("old-private-refresh".into()),
            },
        )
        .unwrap();

        save_stored_tokens(
            dir.path(),
            &StoredTokens {
                refresh_token: "streaming-1".into(),
                webapi_refresh_token: Some("rotated-private-refresh".into()),
            },
        )
        .unwrap();

        let stored = load_stored_tokens(dir.path()).unwrap();
        assert_eq!(
            stored.webapi_refresh_token.as_deref(),
            Some("rotated-private-refresh")
        );
    }

    #[test]
    fn clear_stored_tokens_removes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        save_stored_tokens(
            dir.path(),
            &StoredTokens {
                refresh_token: "streaming-1".into(),
                webapi_refresh_token: Some("private-refresh".into()),
            },
        )
        .unwrap();
        assert!(load_stored_tokens(dir.path()).is_some());

        clear_stored_tokens(dir.path());
        assert!(load_stored_tokens(dir.path()).is_none());

        // Clearing an already-absent file must not panic or error.
        clear_stored_tokens(dir.path());
    }

    #[test]
    fn load_stored_tokens_on_a_fresh_dir_is_none_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_stored_tokens(dir.path()).is_none());
    }

    #[test]
    fn load_stored_tokens_on_corrupted_json_is_none_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tokens.json"), b"{not json").unwrap();
        assert!(load_stored_tokens(dir.path()).is_none());
    }

    /// tokens.json is not readable JSON at all once `save_stored_tokens` has
    /// touched it — confirms it is genuinely being DPAPI-encrypted and not
    /// just written through unchanged.
    #[test]
    fn saved_tokens_file_is_not_plaintext_json() {
        let dir = tempfile::tempdir().unwrap();
        save_stored_tokens(
            dir.path(),
            &StoredTokens {
                refresh_token: "streaming-1".into(),
                webapi_refresh_token: Some("private-refresh".into()),
            },
        )
        .unwrap();
        let raw = std::fs::read(dir.path().join("tokens.json")).unwrap();
        assert!(serde_json::from_slice::<StoredTokens>(&raw).is_err());
        assert!(!String::from_utf8_lossy(&raw).contains("private-refresh"));
    }

    #[test]
    fn dpapi_protect_unprotect_roundtrips() {
        let plaintext = b"a secret refresh token";
        let ciphertext =
            dpapi::protect(plaintext).expect("DPAPI protect should succeed for the current user");
        assert_ne!(ciphertext, plaintext);
        let roundtripped = dpapi::unprotect(&ciphertext)
            .expect("DPAPI unprotect should succeed for the same user/machine");
        assert_eq!(roundtripped, plaintext);
    }

    #[test]
    fn dpapi_unprotect_rejects_garbage_input() {
        assert!(dpapi::unprotect(b"not a dpapi blob").is_none());
    }

    /// The migration path described on [`load_stored_tokens`]: an existing
    /// plaintext install must keep working (no forced re-login) and end up
    /// encrypted on disk after being read once.
    #[test]
    fn a_plaintext_tokens_file_is_migrated_to_encrypted_storage_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let plaintext_tokens = StoredTokens {
            refresh_token: "streaming-1".into(),
            webapi_refresh_token: Some("private-refresh".into()),
        };
        // Bypass save_stored_tokens to write the pre-encryption plaintext
        // format directly, simulating an install from before this change.
        std::fs::write(
            dir.path().join("tokens.json"),
            serde_json::to_vec(&plaintext_tokens).unwrap(),
        )
        .unwrap();

        let loaded = load_stored_tokens(dir.path()).expect("plaintext file should still load");
        assert_eq!(loaded.refresh_token, "streaming-1");
        assert_eq!(
            loaded.webapi_refresh_token.as_deref(),
            Some("private-refresh")
        );

        let raw_after = std::fs::read(dir.path().join("tokens.json")).unwrap();
        assert!(serde_json::from_slice::<StoredTokens>(&raw_after).is_err());
        assert!(dpapi::unprotect(&raw_after).is_some());

        // Re-reading the now-encrypted file must return identical tokens —
        // the migration did not lose or alter anything.
        let reloaded = load_stored_tokens(dir.path()).unwrap();
        assert_eq!(reloaded.refresh_token, loaded.refresh_token);
        assert_eq!(reloaded.webapi_refresh_token, loaded.webapi_refresh_token);
    }

    /// Merge-preserve behaviour (see `save_stored_tokens`'s doc comment)
    /// still holds once every write round-trips through DPAPI: a plaintext
    /// install with a private refresh token, migrated on read, must still
    /// keep that token through a subsequent degraded (`None`) save.
    #[test]
    fn merge_preservation_holds_across_a_plaintext_to_encrypted_migration() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("tokens.json"),
            serde_json::to_vec(&StoredTokens {
                refresh_token: "streaming-1".into(),
                webapi_refresh_token: Some("private-refresh".into()),
            })
            .unwrap(),
        )
        .unwrap();

        // First touch migrates the file to encrypted storage.
        load_stored_tokens(dir.path()).unwrap();

        // A degraded session then saves with no Web API refresh token.
        save_stored_tokens(
            dir.path(),
            &StoredTokens {
                refresh_token: "streaming-2".into(),
                webapi_refresh_token: None,
            },
        )
        .unwrap();

        let stored = load_stored_tokens(dir.path()).unwrap();
        assert_eq!(stored.refresh_token, "streaming-2");
        assert_eq!(
            stored.webapi_refresh_token.as_deref(),
            Some("private-refresh")
        );
    }
}

#[cfg(test)]
mod grant_rejection_tests {
    use super::*;

    /// Table test over real and near-miss Spotify OAuth error strings.
    /// `is_grant_rejected` is the sole gate on deleting stored credentials
    /// (see its doc comment) — a false positive here logs a user out on a
    /// transient failure, and a false negative leaves a dead token stored
    /// forever.
    #[test]
    fn only_invalid_grant_and_invalid_client_are_rejections() {
        let cases: &[(&str, bool)] = &[
            ("invalid_grant", true),
            ("Invalid_Grant: refresh token revoked", true),
            ("error=invalid_client, no such client", true),
            ("INVALID_GRANT", true),
            // Near-misses that must NOT clear stored credentials.
            ("invalid_request: refresh_token must be supplied", false),
            ("invalid_scope", false),
            ("server_error", false),
            ("temporarily_unavailable", false),
            ("connection reset by peer", false),
            ("", false),
        ];

        for (message, expect_rejected) in cases {
            let err = AppError::Auth((*message).to_string());
            assert_eq!(
                is_grant_rejected(&err),
                *expect_rejected,
                "message {message:?} should map to rejected={expect_rejected}"
            );
        }
    }

    #[test]
    fn only_the_auth_variant_can_be_a_grant_rejection() {
        assert!(!is_grant_rejected(&AppError::BadRequest(
            "invalid_grant".into()
        )));
        assert!(!is_grant_rejected(&AppError::WebApi("invalid_grant".into())));
        assert!(!is_grant_rejected(&AppError::SessionExpired));
    }
}
