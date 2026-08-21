//! Event-first Spotify Connect state projection.
//!
//! librespot 0.8 consumes the cluster protobuf internally but does not expose
//! it. Dealer supports fan-out subscriptions, so we observe the same message
//! as Spirc and project only UI-safe state. This is the application-level
//! equivalent of librespot PR #1704's later watch channels without carrying a
//! private librespot fork.

use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use librespot::core::dealer::protocol::Message as DealerMessage;
use librespot::core::session::Session;
use librespot::protocol::connect::{ClusterUpdate, DeviceInfo};
use librespot::protocol::player::PlayerState;
use tauri::{AppHandle, Emitter, Manager};

use crate::connect::Device;
use crate::error::{AppError, AppResult};
use crate::queue::{self, QueueView};
use crate::state::{events, AppState, ConnectionStatus, TrackInfo};

const CLUSTER_TOPIC: &str = "hm://connect-state/v1/cluster";

#[derive(Debug)]
struct ClusterSnapshot {
    devices: Vec<Device>,
    local_is_active: bool,
    player: Option<PlayerState>,
}

#[derive(Debug, Default)]
struct ApplyOutcome {
    playback_changed: bool,
    queue_changed: bool,
}

pub fn spawn(app: AppHandle, session: Session) -> AppResult<tauri::async_runtime::JoinHandle<()>> {
    let local_device_id = session.device_id().to_string();
    let mut updates = session
        .dealer()
        .add_listen_for(CLUSTER_TOPIC)
        .map_err(|error| AppError::Other(format!("subscribe to Connect state: {error}")))?;

    Ok(tauri::async_runtime::spawn(async move {
        let mut retry = std::time::Duration::from_secs(1);
        loop {
            while let Some(message) = updates.next().await {
                retry = std::time::Duration::from_secs(1);
                match DealerMessage::from_raw::<ClusterUpdate>(message) {
                    Ok(update) => {
                        let Some(snapshot) = snapshot(&update, &local_device_id) else {
                            log::debug!("spotify.connect: cluster update had no cluster");
                            continue;
                        };
                        let outcome = apply(&app, snapshot).await;
                        if outcome.playback_changed {
                            let playback = crate::player::snapshot(&app.state::<AppState>()).await;
                            if let Err(error) = app.emit(events::PLAYBACK, playback) {
                                log::warn!("spotify.connect: playback event failed: {error}");
                            }
                        }
                        if outcome.queue_changed {
                            let queue = app.state::<AppState>().queue.read().await.clone();
                            if let Err(error) = app.emit(events::QUEUE, queue) {
                                log::warn!("spotify.connect: queue event failed: {error}");
                            }
                        }
                    }
                    Err(error) => log::warn!("spotify.connect: invalid cluster protobuf: {error}"),
                }
            }

            mark_recovering(&app).await;
            log::warn!("spotify.connect: cluster subscription ended; resubscribing in {retry:?}");
            tokio::time::sleep(retry).await;
            match session.dealer().add_listen_for(CLUSTER_TOPIC) {
                Ok(next) => {
                    updates = next;
                }
                Err(error) => {
                    log::warn!("spotify.connect: cluster resubscribe failed: {error}");
                    retry = (retry * 2).min(std::time::Duration::from_secs(30));
                }
            }
        }
    }))
}

async fn mark_recovering(app: &AppHandle) {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut playback = state.playback.write().await;
        playback.connection_status = ConnectionStatus::Recovering;
        playback.clone()
    };
    let _ = app.emit(events::PLAYBACK, snapshot);
}

fn snapshot(update: &ClusterUpdate, local_device_id: &str) -> Option<ClusterSnapshot> {
    let cluster = update.cluster.as_ref()?;
    let active_device_id = nonempty(&cluster.active_device_id);
    let mut devices: Vec<Device> = cluster
        .device
        .iter()
        .map(|(id, info)| device(id, info, active_device_id.as_deref()))
        .collect();
    devices.sort_by(|left, right| {
        right
            .is_active
            .cmp(&left.is_active)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    let local_is_active = devices
        .iter()
        .any(|device| device.is_active && device.id.as_deref() == Some(local_device_id));
    Some(ClusterSnapshot {
        local_is_active,
        devices,
        player: cluster.player_state.as_ref().cloned(),
    })
}

fn device(id: &str, info: &DeviceInfo, active_device_id: Option<&str>) -> Device {
    let device_id = if info.device_id.is_empty() {
        id.to_string()
    } else {
        info.device_id.clone()
    };
    let volume = info.volume.min(u16::MAX as u32);
    Device {
        id: Some(device_id.clone()),
        name: if info.name.is_empty() {
            "Spotify Connect device".to_string()
        } else {
            info.name.clone()
        },
        device_type: format!("{:?}", info.device_type.enum_value_or_default()).to_lowercase(),
        is_active: active_device_id == Some(id) || active_device_id == Some(device_id.as_str()),
        is_restricted: !info.can_play || !info.disallow_playback_reasons.is_empty(),
        volume_percent: Some(((volume * 100 + u16::MAX as u32 / 2) / u16::MAX as u32) as u8),
    }
}

async fn apply(app: &AppHandle, snapshot: ClusterSnapshot) -> ApplyOutcome {
    let state = app.state::<AppState>();
    let active_device = snapshot
        .devices
        .iter()
        .find(|device| device.is_active)
        .cloned();

    let before_queue = {
        let queue = state.queue.read().await;
        queue_identity(&queue)
    };
    let before_playback;
    {
        let mut playback = state.playback.write().await;
        before_playback = playback_identity(&playback);
        playback.connection_status = ConnectionStatus::Connected;
        playback.is_active_device = snapshot.local_is_active;
        playback.active_device = active_device.clone();
        playback.available_devices = snapshot.devices;
        state.active_device.set(snapshot.local_is_active);

        if !snapshot.local_is_active {
            if let Some(volume) = active_device
                .as_ref()
                .and_then(|device| device.volume_percent)
            {
                playback.volume = crate::player::percent_to_volume(volume);
            }
        }

        if let Some(player) = snapshot.player.as_ref() {
            playback.context_uri = nonempty(&player.context_uri);
            if let Some(options) = player.options.as_ref() {
                playback.shuffle = options.shuffling_context;
                playback.repeat_context = options.repeating_context;
                playback.repeat_track = options.repeating_track;
            }

            // Local PlayerEvents have sample-accurate timing and richer Web API
            // metadata. Cluster playback is authoritative only for another
            // active device (or before this device has started producing audio).
            if !snapshot.local_is_active {
                playback.is_playing = player.is_playing && !player.is_paused;
                playback.is_loading = player.is_buffering;
                playback.duration_ms = clamp_ms(player.duration);
                let position = remote_position(player);
                let duration_ms = playback.duration_ms;
                playback.set_position(if duration_ms > 0 {
                    position.min(duration_ms)
                } else {
                    position
                });
                if let Some(track) = player.track.as_ref() {
                    let next = track_info(track, playback.duration_ms);
                    if !next.uri.is_empty() && next.uri != "-" {
                        let changed = playback
                            .track
                            .as_ref()
                            .is_none_or(|current| current != &next);
                        if changed {
                            playback.track = Some(next);
                        }
                    }
                } else {
                    playback.track = None;
                }
            }
        }
    }

    if let Some(player) = snapshot.player.as_ref() {
        *state.queue.write().await = queue::from_player_state(player);
    }

    let after_playback = playback_identity(&*state.playback.read().await);
    let after_queue = queue_identity(&*state.queue.read().await);
    ApplyOutcome {
        playback_changed: before_playback != after_playback,
        queue_changed: before_queue != after_queue,
    }
}

fn track_info(track: &librespot::protocol::player::ProvidedTrack, duration_ms: u32) -> TrackInfo {
    let summary = queue::track_summary(track);
    TrackInfo {
        uri: summary.uri,
        name: summary.name,
        artists: summary.artists,
        album: summary.album,
        cover_url: summary.image_url,
        duration_ms: summary.duration_ms.max(duration_ms),
    }
}

fn remote_position(player: &PlayerState) -> u32 {
    let mut position = player.position_as_of_timestamp.max(player.position).max(0);
    if player.is_playing && !player.is_paused {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis() as i64);
        let elapsed = now.saturating_sub(player.timestamp);
        // A stale timestamp can come from a previous session. go-librespot
        // uses the same ten-minute sanity bound before advancing it.
        if (0..=600_000).contains(&elapsed) {
            position =
                position.saturating_add((elapsed as f64 * player.playback_speed.max(0.0)) as i64);
        }
    }
    clamp_ms(position)
}

fn clamp_ms(value: i64) -> u32 {
    value.clamp(0, u32::MAX as i64) as u32
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty() && value != "-").then(|| value.to_string())
}

#[derive(PartialEq, Eq)]
struct PlaybackIdentity {
    playing: bool,
    loading: bool,
    local_active: bool,
    position_ms: u32,
    duration_ms: u32,
    track: Option<TrackInfo>,
    context: Option<String>,
    volume: u16,
    shuffle: bool,
    repeat_context: bool,
    repeat_track: bool,
    active_device: Option<String>,
    devices: Vec<DeviceIdentity>,
    status: ConnectionStatus,
}

#[derive(PartialEq, Eq)]
struct DeviceIdentity {
    id: Option<String>,
    name: String,
    device_type: String,
    active: bool,
    restricted: bool,
    volume_percent: Option<u8>,
}

fn playback_identity(playback: &crate::state::PlaybackState) -> PlaybackIdentity {
    PlaybackIdentity {
        playing: playback.is_playing,
        loading: playback.is_loading,
        local_active: playback.is_active_device,
        position_ms: playback.position_ms,
        duration_ms: playback.duration_ms,
        track: playback.track.clone(),
        context: playback.context_uri.clone(),
        volume: playback.volume,
        shuffle: playback.shuffle,
        repeat_context: playback.repeat_context,
        repeat_track: playback.repeat_track,
        active_device: playback
            .active_device
            .as_ref()
            .and_then(|device| device.id.clone()),
        devices: playback
            .available_devices
            .iter()
            .map(|device| DeviceIdentity {
                id: device.id.clone(),
                name: device.name.clone(),
                device_type: device.device_type.clone(),
                active: device.is_active,
                restricted: device.is_restricted,
                volume_percent: device.volume_percent,
            })
            .collect(),
        status: playback.connection_status,
    }
}

#[derive(PartialEq, Eq)]
struct QueueIdentity {
    current: Option<String>,
    previous: Vec<String>,
    next: Vec<String>,
    autoplay: Vec<String>,
    revision: Option<String>,
}

fn queue_identity(queue: &QueueView) -> QueueIdentity {
    QueueIdentity {
        current: queue
            .currently_playing
            .as_ref()
            .map(|track| track.uri.clone()),
        previous: queue
            .previous
            .iter()
            .map(|track| track.uri.clone())
            .collect(),
        next: queue.queue.iter().map(|track| track.uri.clone()).collect(),
        autoplay: queue
            .autoplay
            .iter()
            .map(|track| track.uri.clone())
            .collect(),
        revision: queue.revision.clone(),
    }
}

#[cfg(test)]
mod tests {
    use librespot::protocol::connect::{Cluster, DeviceInfo};
    use librespot::protocol::devices::DeviceType;
    use librespot::protocol::player::{ContextPlayerOptions, PlayerState, ProvidedTrack};

    use super::*;

    #[test]
    fn projects_devices_context_and_queue_from_cluster() {
        let mut phone = DeviceInfo {
            can_play: true,
            volume: 32_768,
            name: "Phone".to_string(),
            device_id: "phone".to_string(),
            device_type: DeviceType::SMARTPHONE.into(),
            ..Default::default()
        };
        phone.metadata_map.insert("ignored".into(), "safe".into());
        let mut player = PlayerState {
            context_uri: "spotify:playlist:test".to_string(),
            is_playing: true,
            duration: 180_000,
            position_as_of_timestamp: 12_000,
            options: Some(ContextPlayerOptions {
                shuffling_context: true,
                ..Default::default()
            })
            .into(),
            ..Default::default()
        };
        player.next_tracks.push(ProvidedTrack {
            uri: "spotify:track:next".to_string(),
            provider: "queue".to_string(),
            ..Default::default()
        });
        let cluster = Cluster {
            active_device_id: "phone".to_string(),
            player_state: Some(player).into(),
            device: [("phone".to_string(), phone)].into(),
            ..Default::default()
        };
        let update = ClusterUpdate {
            cluster: Some(cluster).into(),
            ..Default::default()
        };

        let projected = snapshot(&update, "rustify").expect("cluster");
        assert!(!projected.local_is_active);
        assert_eq!(projected.devices[0].name, "Phone");
        assert!(projected.devices[0].is_active);
        assert_eq!(
            projected.player.expect("player").context_uri,
            "spotify:playlist:test"
        );
    }

    #[test]
    fn stale_remote_timestamp_does_not_advance_position() {
        let player = PlayerState {
            is_playing: true,
            position_as_of_timestamp: 42_000,
            timestamp: 1,
            playback_speed: 1.0,
            ..Default::default()
        };
        assert_eq!(remote_position(&player), 42_000);
    }

    #[test]
    fn resolves_active_device_by_cluster_map_key_and_rounds_volume() {
        let info = DeviceInfo {
            can_play: true,
            device_id: "reported-id".to_owned(),
            volume: 65_534,
            ..Default::default()
        };
        let projected = device("cluster-key", &info, Some("cluster-key"));
        assert!(projected.is_active);
        assert_eq!(projected.volume_percent, Some(100));
    }
}
