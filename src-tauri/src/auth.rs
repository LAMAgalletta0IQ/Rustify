use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use librespot::core::config::SessionConfig;
use librespot_oauth::{OAuthClientBuilder, OAuthToken};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::state::{AuthState, TokenStore};
use crate::webapi::WebApi;

/// Loopback redirect librespot's OAuth helper serves on. Registered against
/// Spotify's own desktop client ID, which is what `SessionConfig::default()`
/// carries.
pub const REDIRECT_URI: &str = "http://127.0.0.1:8898/login";

/// One login serves both purposes, so the scope list is the union of what
/// librespot needs to stream and what the Web API needs for library/search/
/// Connect control.
pub const SCOPES: &[&str] = &[
    // Playback via librespot
    "streaming",
    // Profile (also how we read `product` to enforce the Premium check)
    "user-read-private",
    "user-read-email",
    // Connect device list + transfer
    "user-read-playback-state",
    "user-modify-playback-state",
    "user-read-currently-playing",
    // Library
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

/// Persisted between runs so returning users skip the browser round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTokens {
    pub refresh_token: String,
}

fn tokens_path(data_dir: &Path) -> PathBuf {
    data_dir.join("tokens.json")
}

pub fn load_stored_tokens(data_dir: &Path) -> Option<StoredTokens> {
    let raw = std::fs::read_to_string(tokens_path(data_dir)).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save_stored_tokens(data_dir: &Path, tokens: &StoredTokens) -> AppResult<()> {
    std::fs::create_dir_all(data_dir)?;
    let raw = serde_json::to_string(tokens)
        .map_err(|e| AppError::Other(format!("failed to serialise tokens: {e}")))?;
    std::fs::write(tokens_path(data_dir), raw)?;
    Ok(())
}

pub fn clear_stored_tokens(data_dir: &Path) {
    let _ = std::fs::remove_file(tokens_path(data_dir));
}

fn build_client(client_id: &str) -> AppResult<librespot_oauth::OAuthClient> {
    OAuthClientBuilder::new(client_id, REDIRECT_URI, SCOPES.to_vec())
        .open_in_browser()
        .with_custom_message(
            "Login successful. You can close this tab and return to the app.",
        )
        .build()
        .map_err(|e| AppError::Auth(e.to_string()))
}

/// Interactive login: opens the system browser and waits for the loopback
/// redirect. Blocking work is confined to librespot's own async helper.
pub async fn interactive_login(client_id: &str) -> AppResult<OAuthToken> {
    build_client(client_id)?
        .get_access_token_async()
        .await
        .map_err(|e| AppError::Auth(e.to_string()))
}

/// Silent login using a stored refresh token. Falls back to `interactive_login`
/// at the call site if this fails (token revoked, scopes changed, etc.).
pub async fn refresh_login(client_id: &str, refresh_token: &str) -> AppResult<OAuthToken> {
    build_client(client_id)?
        .refresh_token_async(refresh_token)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))
}

pub fn default_client_id() -> String {
    SessionConfig::default().client_id
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
    mut refresh_token: String,
    tokens: TokenStore,
    data_dir: PathBuf,
    mut expires_at: Instant,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        loop {
            let delay = expires_at
                .saturating_duration_since(Instant::now())
                .saturating_sub(REFRESH_MARGIN)
                .max(MIN_REFRESH_DELAY);

            tokio::time::sleep(delay).await;

            match refresh_login(&client_id, &refresh_token).await {
                Ok(tok) => {
                    tokens.set(tok.access_token).await;
                    expires_at = tok.expires_at;

                    // Spotify may hand back a new refresh token; if so the old
                    // one stops working, so persist the replacement.
                    if !tok.refresh_token.is_empty() && tok.refresh_token != refresh_token {
                        refresh_token = tok.refresh_token;
                        if let Err(e) = save_stored_tokens(
                            &data_dir,
                            &StoredTokens {
                                refresh_token: refresh_token.clone(),
                            },
                        ) {
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
pub async fn fetch_profile_require_premium(
    api: &WebApi,
    token: &str,
) -> AppResult<AuthState> {
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
