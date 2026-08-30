use serde::{Deserialize, Serialize};
use serde_json::json;
#[cfg(feature = "ts-rs-export")]
use ts_rs::TS;

use crate::error::AppResult;
use crate::webapi::WebApi;

/// Note the one-sided rename: Spotify sends snake_case (`is_active`), while
/// the webview expects camelCase. `rename_all = "camelCase"` applies to *both*
/// directions, which made every device list fail to parse with
/// "missing field `isActive`" — so the rename is restricted to serialisation
/// and deserialisation keeps the wire names.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-rs-export", derive(TS))]
#[serde(rename_all(serialize = "camelCase"))]
#[cfg_attr(
    feature = "ts-rs-export",
    ts(export, export_to = "../../src/lib/generated/", rename_all = "camelCase")
)]
pub struct Device {
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: String,
    pub is_active: bool,
    pub is_restricted: bool,
    pub volume_percent: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct DevicesResponse {
    devices: Vec<Device>,
}

/// Lists every Connect device visible to the account, including this app
/// (which registers itself through Spirc).
pub async fn list_devices(api: &WebApi, token: &str) -> AppResult<Vec<Device>> {
    let resp: DevicesResponse = api.get(token, "/me/player/devices", &[]).await?;
    Ok(resp.devices)
}

// ---- remote playback ----------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RemoteImage {
    pub url: String,
    pub width: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteNamed {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RemoteAlbum {
    pub id: String,
    pub uri: String,
    pub name: String,
    #[serde(default)]
    pub images: Vec<RemoteImage>,
}

/// The `item` of `/me/player`. Covers a track; an episode omits `artists`
/// and `album`, hence the defaults.
#[derive(Debug, Deserialize)]
pub struct RemoteItem {
    pub uri: String,
    pub name: String,
    #[serde(default)]
    pub artists: Vec<RemoteNamed>,
    pub album: Option<RemoteAlbum>,
    #[serde(default)]
    pub images: Vec<RemoteImage>,
    pub duration_ms: u32,
}

#[derive(Debug, Deserialize)]
pub struct RemotePlayback {
    pub device: Option<Device>,
    pub context: Option<RemoteContext>,
    pub is_playing: bool,
    pub progress_ms: Option<u32>,
    pub item: Option<RemoteItem>,
    /// "off" | "context" | "track"
    pub repeat_state: Option<String>,
    pub shuffle_state: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteContext {
    pub uri: String,
}

/// Current playback across the account, whatever device it is on.
///
/// `Ok(None)` when Spotify answers 204 — nothing is playing anywhere. This is
/// the only way to see playback happening on another device: librespot emits
/// `PlayerEvent`s only for audio this app is producing itself.
pub async fn current_playback(api: &WebApi, token: &str) -> AppResult<Option<RemotePlayback>> {
    api.get(token, "/me/player", &[]).await
}

/// Moves playback to another Connect device (or back to this app).
///
/// `play: true` resumes on the target; `false` preserves the current
/// play/pause state.
pub async fn transfer_playback(
    api: &WebApi,
    token: &str,
    device_id: &str,
    play: bool,
) -> AppResult<()> {
    api.put(
        token,
        "/me/player",
        json!({ "device_ids": [device_id], "play": play }),
    )
    .await
}
