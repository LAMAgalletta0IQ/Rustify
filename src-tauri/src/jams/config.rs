use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::error::JamError;

/// Runtime configuration for the Jams module.
///
/// The *hosts* are known and stable. The endpoint paths ship with defaults
/// (see [`JamConfig::default`]) that can be overridden per-key from
/// `jams.toml`, because they are internal paths that change without notice.
/// The Pathfinder persisted-query hashes have **no** defaults — a wrong hash
/// is rejected outright — so metadata enrichment stays off until one is
/// captured.
///
/// ```toml
/// # jams.toml — every key optional; anything omitted keeps its default
/// spclient_base_url  = "https://spclient.wg.spotify.com"
/// pathfinder_url     = "https://api-partner.spotify.com/pathfinder/v2/query"
/// client_token_url   = "https://clienttoken.spotify.com/v1/clienttoken"
/// dealer_url         = "wss://dealer.spotify.com"
///
/// [spclient_endpoints]
/// create_jam = "/social-connect/v2/sessions/current_or_new"
/// join_jam   = "/social-connect/v2/sessions/join/{jam_id}"
/// leave      = "/social-connect/v2/sessions/leave"
///
/// [pathfinder_hashes]
/// FetchJam = "1e3b6cd0b9a4..."
/// ```
///
/// Overriding is per-key and additive: a `[spclient_endpoints]` table with
/// only `join_jam` in it replaces that one path and leaves the rest at their
/// defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JamConfig {
    /// Base URL for the SpClient family of APIs.
    pub spclient_base_url: String,
    /// Named endpoint path templates. `{jam_id}` is substituted at call time.
    /// Seeded by [`default_spclient_endpoints`]; a `jams.toml` entry with the
    /// same key wins. Actions whose key has no path (and no default) refuse
    /// loudly rather than guessing. See `README_jams.md` for the key names.
    pub spclient_endpoints: HashMap<String, String>,
    /// Pathfinder GraphQL gateway.
    pub pathfinder_url: String,
    /// Operation name -> persisted-query `sha256Hash`, captured from the
    /// official client. Empty until captured.
    pub pathfinder_hashes: HashMap<String, String>,
    /// Internal client-token mint service.
    pub client_token_url: String,
    /// Dealer WebSocket URL.
    pub dealer_url: String,
}

impl Default for JamConfig {
    fn default() -> Self {
        Self {
            spclient_base_url: "https://spclient.wg.spotify.com".to_owned(),
            spclient_endpoints: default_spclient_endpoints(),
            pathfinder_url: "https://api-partner.spotify.com/pathfinder/v2/query".to_owned(),
            pathfinder_hashes: HashMap::new(),
            client_token_url: "https://clienttoken.spotify.com/v1/clienttoken".to_owned(),
            dealer_url: "wss://dealer.spotify.com".to_owned(),
        }
    }
}

/// Default SpClient paths for the jam actions.
///
/// Jams are the current name for what the backend still calls a *social
/// connect session*: the same service that powered Group Sessions, whose
/// paths are stable enough to be worth defaulting. The previous behaviour —
/// no defaults at all, every action erroring with "capture it with a traffic
/// proxy" — meant the feature could not be used without a mitmproxy session
/// first, so these are pre-filled and left overridable rather than left blank.
///
/// > Two things this does **not** change. Paths are still config-driven, so a
/// > 404 or 400 here is fixed by editing `jams.toml`, not by rebuilding. And
/// > `reorder_queue` deliberately has **no** default: nothing public documents
/// > a social-connect queue-reorder call, and a guessed path would fire a real
/// > request at Spotify, so it still refuses.
///
/// The HTTP verb and query parameters are *not* configurable — they are fixed
/// per action in [`crate::jams::spclient`], because they differ per endpoint
/// (create is a `GET`, the rest are `POST`s).
pub fn default_spclient_endpoints() -> HashMap<String, String> {
    [
        // GET; returns the caller's active session or opens a new one.
        ("create_jam", "/social-connect/v2/sessions/current_or_new"),
        // GET; the current session, or 404 when there is none.
        ("current_jam", "/social-connect/v2/sessions/current"),
        // POST; `{jam_id}` is the join *token* from the invite link, not the
        // session id.
        ("join_jam", "/social-connect/v2/sessions/join/{jam_id}"),
        // POST; leaves whatever session this device is in.
        ("leave", "/social-connect/v2/sessions/leave"),
        // PUT; the names are expressed from the UI perspective, while the
        // server path uses the inverse queue-only-mode flag.
        (
            "queue_control_allowed",
            "/social-connect/v2/sessions/current/queue_only_mode/disabled",
        ),
        (
            "queue_control_denied",
            "/social-connect/v2/sessions/current/queue_only_mode/enabled",
        ),
        // v3 host moderation actions, independently confirmed by the
        // spotify-jam 0.2 package and the current social-connect schema.
        (
            "kick_member",
            "/social-connect/v3/sessions/{jam_id}/member/{member_id}/kick",
        ),
        ("end_jam", "/social-connect/v3/sessions/{jam_id}"),
        // POST; a jam's queue *is* the Connect queue, so this is the ordinary
        // public Web API endpoint (absolute, so it bypasses the spclient host).
        ("add_track", "https://api.spotify.com/v1/me/player/queue"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

impl JamConfig {
    /// Parses TOML over the defaults: keys present in the file win, keys
    /// absent from it keep their default. Without the merge, writing a
    /// `[spclient_endpoints]` table to override one path would silently blank
    /// every other one.
    pub fn from_toml_str(s: &str) -> Result<Self, JamError> {
        let mut config: Self = toml::from_str(s).map_err(|e| JamError::Config(e.to_string()))?;
        for (key, path) in default_spclient_endpoints() {
            config.spclient_endpoints.entry(key).or_insert(path);
        }
        Ok(config)
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, JamError> {
        let s = std::fs::read_to_string(path).map_err(|e| JamError::Config(e.to_string()))?;
        Self::from_toml_str(&s)
    }
}
