use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use super::client_token::ClientTokenManager;
use super::error::JamError;
use super::token::TokenProvider;
use super::ConnectionId;

/// Events decoded from the dealer websocket.
///
/// The typed variants are best-effort matches on the `type` string seen in
/// captured payloads; the exact strings are internal and may drift, so anything
/// unrecognised surfaces as [`JamEvent::Unknown`] instead of being dropped.
///
/// Serialised to the webview as-is (`camelCase` variant and field names) when
/// forwarded by the app's jam bridge.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JamEvent {
    /// Authoritative social-connect update delivered on librespot's Dealer
    /// connection. `reason` matches `SessionUpdateReason` names.
    #[serde(rename_all = "camelCase")]
    SessionUpdate {
        reason: String,
        session: Option<super::session::JamSession>,
        updated_members: Vec<super::session::JamMember>,
    },
    /// Device discoverability/exposure update from social-connect.
    BroadcastStatus(Value),
    #[serde(rename_all = "camelCase")]
    TrackAdded { jam_id: String, track: Value },
    #[serde(rename_all = "camelCase")]
    QueueReordered { jam_id: String, queue: Value },
    #[serde(rename_all = "camelCase")]
    MemberJoined { jam_id: String, member: Value },
    #[serde(rename_all = "camelCase")]
    MemberLeft { jam_id: String, member_id: String },
    #[serde(rename_all = "camelCase")]
    VoteUpdated { jam_id: String, vote: Value },
    /// Full jam state push.
    JamState(Value),
    /// Any payload whose `type` is not recognised.
    #[serde(rename_all = "camelCase")]
    Unknown {
        jam_id: Option<String>,
        kind: String,
        payload: Value,
    },
}

/// WebSocket listener for `wss://dealer.spotify.com`.
///
/// Runs in a background task and pushes decoded [`JamEvent`]s into an `mpsc`
/// channel. Reliability guarantees, in order of importance:
///
/// 1. **Heartbeat** — sends an application-level ping on a fixed interval and
///    tracks the time since the last inbound frame; a connection that goes
///    silent is torn down so the reconnection logic can rebuild it.
/// 2. **Reconnection** — on any disconnect the task sleeps with exponential
///    backoff (bounded) and reconnects; the event channel survives across
///    reconnects, so a consumer never needs to re-subscribe.
///
/// The dealer connect handshake carries both the OAuth bearer and the
/// `client-token`. The exact handshake shape and the event `type` strings are
/// internal and configurable-by-capture; adjust them if the connection is
/// refused or events stop arriving. The `connectionId` sent in the handshake is
/// drawn from a shared [`ConnectionId`] that is rotated on every (re)connect;
/// the SpClient/Pathfinder clients attach the same id as their
/// `Spotify-Connection-Id` header, so the server can route pushes to this
/// websocket.
pub struct DealerClient {
    url: String,
    connection_id: ConnectionId,
    device_id: String,
    access_token: Arc<dyn TokenProvider>,
    client_token: ClientTokenManager,
    base_backoff: Duration,
    max_backoff: Duration,
    dead_timeout: Duration,
    tx: mpsc::Sender<JamEvent>,
}

impl DealerClient {
    pub fn new(
        url: impl Into<String>,
        connection_id: ConnectionId,
        access_token: Arc<dyn TokenProvider>,
        client_token: ClientTokenManager,
        buffer: usize,
    ) -> (Self, mpsc::Receiver<JamEvent>) {
        let (tx, rx) = mpsc::channel(buffer);
        let device_id = format!("dealer-{}", uuid::Uuid::new_v4());
        (
            Self {
                url: url.into(),
                connection_id,
                device_id,
                access_token,
                client_token,
                base_backoff: Duration::from_secs(1),
                max_backoff: Duration::from_secs(30),
                dead_timeout: Duration::from_secs(90),
                tx,
            },
            rx,
        )
    }

    pub fn with_backoff(mut self, base: Duration, max: Duration) -> Self {
        self.base_backoff = base;
        self.max_backoff = max;
        self
    }

    pub fn with_dead_timeout(mut self, timeout: Duration) -> Self {
        self.dead_timeout = timeout;
        self
    }

    /// Spawns the listener on the current runtime. The returned handle is the
    /// only way to stop the loop (abort it).
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(self.run())
    }

    async fn run(self) {
        let mut backoff = self.base_backoff;
        loop {
            let started = Instant::now();
            match self.connect_and_listen().await {
                Ok(()) => info!("dealer closed cleanly; reconnecting"),
                Err(e) => error!("dealer connection lost: {e}"),
            }
            // A session that survived a while suggests a healthy network;
            // reset the backoff instead of compounding it.
            if started.elapsed() > Duration::from_secs(60) {
                backoff = self.base_backoff;
            }
            warn!("dealer reconnecting in {backoff:?}");
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(self.max_backoff);
        }
    }

    async fn connect_and_listen(&self) -> Result<(), JamError> {
        let token = self.access_token.access_token().ok_or_else(|| {
            JamError::ClientTokenExpired(
                "no first-party access token yet; the dealer waits for the host app to supply                  one (see the librespot login5 warning in the log)"
                    .into(),
            )
        })?;
        let client_token = self.client_token.get_token().await?;

        // The dealer authenticates from the query string at handshake time —
        // it closes the socket immediately otherwise, so the token has to be
        // resolved before connecting, not after.
        let separator = if self.url.contains('?') { '&' } else { '?' };
        let url = format!("{}{separator}access_token={token}", self.url);
        let (mut ws, _resp) = connect_async(url).await?;
        info!("dealer connected to {}", self.url);

        // No `connectionId` here: the server assigns one and announces it on
        // this socket (see `note_connection_id`). Sending a self-invented id
        // was the bug behind every jam request 400ing.
        let connect = serde_json::json!({
            "type": "connect",
            "auth": { "token": token, "clientToken": client_token },
            "device": {
                "deviceInfo": {
                    "deviceId": self.device_id,
                    "brand": "unknown",
                    "clientVersion": "1.2.0",
                }
            },
        });
        ws.send(Message::Text(connect.to_string())).await?;

        let (mut writer, mut reader) = ws.split();
        let mut heartbeat = tokio::time::interval(self.dead_timeout / 3);
        let mut last_activity = Instant::now();

        loop {
            tokio::select! {
                frame = reader.next() => {
                    match frame {
                        Some(Ok(Message::Text(text))) => {
                            last_activity = Instant::now();
                            self.note_connection_id(&text);
                            for event in self.parse_envelope(&text) {
                                if let Err(err) = self.tx.try_send(event) {
                                    // Receiver gone or slow; never block the
                                    // websocket loop on it.
                                    warn!("dealer event dropped ({err}); is anything consuming the channel?");
                                }
                            }
                        }
                        Some(Ok(Message::Ping(_))) => { /* protocol-level pong is automatic */ }
                        Some(Ok(Message::Pong(_))) => last_activity = Instant::now(),
                        Some(Ok(Message::Close(_))) => {
                            return Err(JamError::DealerDisconnected("server closed the connection".into()));
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => return Err(e.into()),
                        None => return Err(JamError::DealerDisconnected("websocket stream ended".into())),
                    }
                }
                _ = heartbeat.tick() => {
                    if last_activity.elapsed() > self.dead_timeout {
                        return Err(JamError::DealerDisconnected(format!(
                            "no inbound traffic for {:?}",
                            self.dead_timeout
                        )));
                    }
                    if writer.send(Message::Ping(Vec::new().into())).await.is_err() {
                        return Err(JamError::DealerDisconnected("heartbeat ping send failed".into()));
                    }
                }
            }
        }
    }

    /// Picks the server-assigned `Spotify-Connection-Id` out of a frame, if it
    /// carries one.
    ///
    /// It arrives once per connection, on the `hm://pusher/v1/connections/...`
    /// message. Recorded for logging and as a fallback: Rustify overwrites the
    /// shared id with librespot's before every command, because that is the
    /// connection the Connect device in `local_device_id` is registered on.
    fn note_connection_id(&self, text: &str) {
        let Ok(frame) = serde_json::from_str::<serde_json::Value>(text) else {
            return;
        };
        let Some(id) = frame
            .get("headers")
            .and_then(|h| h.get("Spotify-Connection-Id"))
            .and_then(|v| v.as_str())
        else {
            return;
        };
        info!("dealer connection id assigned");
        if self.connection_id.current().is_empty() {
            self.connection_id.set(id);
        }
    }

    /// Decodes a dealer envelope. Control frames (`connect`, `pong`, ...) carry
    /// no jam payload and yield nothing; `message` frames contribute one event
    /// per payload entry.
    fn parse_envelope(&self, text: &str) -> Vec<JamEvent> {
        let Ok(frame) = serde_json::from_str::<Value>(text) else {
            warn!("dealer sent a non-JSON frame: {text}");
            return Vec::new();
        };
        match frame.get("type").and_then(|t| t.as_str()) {
            Some("message") => frame
                .get("payloads")
                .and_then(|p| p.as_array())
                .into_iter()
                .flatten()
                .map(|payload| self.decode_payload(payload))
                .collect(),
            Some(other) => {
                debug!("dealer control frame type={other}");
                Vec::new()
            }
            None => {
                debug!("dealer frame without a type: {frame}");
                Vec::new()
            }
        }
    }

    /// Maps a single payload to a [`JamEvent`]. The payload `uri` embeds the
    /// jam id (`hm://jam/v1/{id}`); the `type` string selects the variant.
    fn decode_payload(&self, payload: &Value) -> JamEvent {
        let kind = payload
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown")
            .to_owned();
        let body = payload
            .get("body")
            .cloned()
            .unwrap_or_else(|| payload.clone());
        let jam_id = payload
            .get("uri")
            .and_then(|u| u.as_str())
            .and_then(|u| u.strip_prefix("hm://jam/v1/"))
            .map(str::to_owned);

        match kind.as_str() {
            "track_added" | "sp_jam_track_added" => JamEvent::TrackAdded {
                jam_id: jam_id.clone().unwrap_or_default(),
                track: body,
            },
            "queue_reordered" | "sp_jam_queue_reordered" => JamEvent::QueueReordered {
                jam_id: jam_id.clone().unwrap_or_default(),
                queue: body,
            },
            "member_joined" | "sp_jam_member_joined" => JamEvent::MemberJoined {
                jam_id: jam_id.clone().unwrap_or_default(),
                member: body,
            },
            "member_left" | "sp_jam_member_left" => JamEvent::MemberLeft {
                jam_id: jam_id.clone().unwrap_or_default(),
                member_id: body
                    .get("user")
                    .and_then(|u| u.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or_default()
                    .to_owned(),
            },
            "vote_updated" | "sp_jam_vote_updated" => JamEvent::VoteUpdated {
                jam_id: jam_id.clone().unwrap_or_default(),
                vote: body,
            },
            "state" | "jam_state" => JamEvent::JamState(body),
            _ => JamEvent::Unknown {
                jam_id,
                kind,
                payload: body,
            },
        }
    }
}
