use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::AppResult;
use crate::webapi::WebApi;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
