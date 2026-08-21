use reqwest::StatusCode;
use thiserror::Error;

/// Error type for the experimental Jams module.
///
/// Everything public returns `Result<_, JamError>`. The variants map onto the
/// distinct failure modes of the three internal services (SpClient, Pathfinder,
/// Dealer) so callers can react to each without matching on error strings.
#[derive(Debug, Error)]
pub enum JamError {
    /// The OAuth or `client-token` could not be supplied. For the OAuth bearer
    /// this usually means "not logged in"; for the client token it means the
    /// mint service refused or the response shape changed.
    #[error("client token missing, expired, or refused: {0}")]
    ClientTokenExpired(String),

    /// Pathfinder rejected the persisted query — almost always because the
    /// `sha256Hash` captured from the official client is stale. Re-capture it
    /// and update `JamConfig.pathfinder_hashes`.
    #[error("pathfinder persisted query {operation:?} (sha256 {hash:?}) is stale or rejected: {message}")]
    PersistedQueryExpired {
        operation: String,
        hash: String,
        message: String,
    },

    /// The dealer websocket dropped or closed. The listener reconnects
    /// automatically, so callers should treat this as "events paused", not
    /// "session dead".
    #[error("dealer websocket disconnected: {0}")]
    DealerDisconnected(String),

    /// A SpClient action targeted a jam that does not exist (HTTP 404).
    #[error("jam not found: {0}")]
    JamNotFound(String),

    /// A SpClient/Pathfinder call was refused (HTTP 401/403).
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("websocket error: {0}")]
    WebSocketError(#[from] tokio_tungstenite::tungstenite::Error),

    /// Non-2xx response that is not already classified above.
    #[error("spclient returned HTTP {status}: {body}")]
    Http { status: StatusCode, body: String },

    #[error("failed to decode jam payload: {0}")]
    Decode(#[from] serde_json::Error),

    /// The receiver end of the dealer event channel is gone.
    #[error("jam event channel closed: {0}")]
    ChannelClosed(String),

    /// Configuration is incomplete. Deliberately loud: endpoints are captured
    /// from the official client and must not be guessed.
    #[error("invalid jams configuration: {0}")]
    Config(String),
}