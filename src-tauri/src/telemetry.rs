//! Privacy-preserving playback telemetry boundary.
//!
//! Spotify's current account-history transport is the private Gabo receiver.
//! Public references only make it accept third-party traffic by impersonating
//! desktop-client context and anti-fraud fields. Rustify deliberately does not
//! do that. This module still records the exact, genuine local playback facts
//! needed by a future authorized transport, in memory and with a fixed bound.

use std::collections::{HashMap, VecDeque};

use librespot::playback::player::PlayerEvent;
use serde::Serialize;
use tokio::sync::Mutex;

const MAX_RECENT_EVENTS: usize = 100;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackAudit {
    pub uri: String,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub played_ms: u64,
    pub final_position_ms: u32,
    pub completed: bool,
    pub end_reason: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryStatus {
    pub delivery_available: bool,
    pub delivery_transport: Option<String>,
    pub delivery_blocker: String,
    pub active_playbacks: usize,
    pub locally_recorded: usize,
    pub recent: Vec<PlaybackAudit>,
}

#[derive(Debug)]
struct ActivePlayback {
    uri: String,
    started_at_ms: u64,
    last_position_ms: u32,
    played_ms: u64,
    playing_since_ms: Option<u64>,
}

impl ActivePlayback {
    fn new(uri: String, position_ms: u32, now_ms: u64) -> Self {
        Self {
            uri,
            started_at_ms: now_ms,
            last_position_ms: position_ms,
            played_ms: 0,
            playing_since_ms: None,
        }
    }

    fn start(&mut self, position_ms: u32, now_ms: u64) {
        self.last_position_ms = position_ms;
        self.playing_since_ms.get_or_insert(now_ms);
    }

    fn checkpoint(&mut self, position_ms: u32, now_ms: u64) {
        if let Some(started) = self.playing_since_ms.take() {
            // Wall-clock listening time prevents a seek from manufacturing a
            // large listened interval. A backwards clock simply adds zero.
            self.played_ms = self
                .played_ms
                .saturating_add(now_ms.saturating_sub(started));
        }
        self.last_position_ms = position_ms;
    }

    fn seek(&mut self, position_ms: u32, now_ms: u64) {
        let was_playing = self.playing_since_ms.is_some();
        self.checkpoint(position_ms, now_ms);
        if was_playing {
            self.playing_since_ms = Some(now_ms);
        }
    }

    fn finish(
        mut self,
        position_ms: u32,
        now_ms: u64,
        completed: bool,
        reason: &str,
    ) -> PlaybackAudit {
        self.checkpoint(position_ms, now_ms);
        PlaybackAudit {
            uri: self.uri,
            started_at_ms: self.started_at_ms,
            ended_at_ms: now_ms,
            played_ms: self.played_ms,
            final_position_ms: position_ms,
            completed,
            end_reason: reason.to_string(),
        }
    }
}

#[derive(Default, Debug)]
struct Inner {
    active: HashMap<u64, ActivePlayback>,
    recent: VecDeque<PlaybackAudit>,
    total_recorded: usize,
}

#[derive(Default)]
pub struct TelemetryTracker {
    inner: Mutex<Inner>,
}

impl TelemetryTracker {
    pub async fn observe(&self, event: &PlayerEvent) {
        self.observe_at(event, unix_ms()).await;
    }

    async fn observe_at(&self, event: &PlayerEvent, now_ms: u64) {
        let mut inner = self.inner.lock().await;
        match event {
            PlayerEvent::Loading {
                play_request_id,
                track_id,
                position_ms,
            } => {
                inner.active.entry(*play_request_id).or_insert_with(|| {
                    ActivePlayback::new(track_id.to_uri(), *position_ms, now_ms)
                });
            }
            PlayerEvent::Playing {
                play_request_id,
                track_id,
                position_ms,
            } => {
                inner
                    .active
                    .entry(*play_request_id)
                    .or_insert_with(|| ActivePlayback::new(track_id.to_uri(), *position_ms, now_ms))
                    .start(*position_ms, now_ms);
            }
            PlayerEvent::Paused {
                play_request_id,
                position_ms,
                ..
            } => {
                if let Some(active) = inner.active.get_mut(play_request_id) {
                    active.checkpoint(*position_ms, now_ms);
                }
            }
            PlayerEvent::PositionChanged {
                play_request_id,
                position_ms,
                ..
            }
            | PlayerEvent::PositionCorrection {
                play_request_id,
                position_ms,
                ..
            } => {
                if let Some(active) = inner.active.get_mut(play_request_id) {
                    active.last_position_ms = *position_ms;
                }
            }
            PlayerEvent::Seeked {
                play_request_id,
                position_ms,
                ..
            } => {
                if let Some(active) = inner.active.get_mut(play_request_id) {
                    active.seek(*position_ms, now_ms);
                }
            }
            PlayerEvent::EndOfTrack {
                play_request_id, ..
            } => finish(&mut inner, *play_request_id, now_ms, true, "track_done"),
            PlayerEvent::Stopped {
                play_request_id, ..
            } => finish(&mut inner, *play_request_id, now_ms, false, "stopped"),
            PlayerEvent::Unavailable {
                play_request_id, ..
            } => finish(&mut inner, *play_request_id, now_ms, false, "unavailable"),
            _ => {}
        }
    }

    pub async fn finish_all(&self, reason: &str) {
        let now_ms = unix_ms();
        let mut inner = self.inner.lock().await;
        let active = std::mem::take(&mut inner.active);
        for (_, playback) in active {
            let position = playback.last_position_ms;
            push_recent(&mut inner, playback.finish(position, now_ms, false, reason));
        }
    }

    pub async fn status(&self) -> TelemetryStatus {
        let inner = self.inner.lock().await;
        TelemetryStatus {
            delivery_available: false,
            delivery_transport: None,
            delivery_blocker: "Spotify exposes no authorized third-party playback telemetry transport; Gabo requires first-party client impersonation, which Rustify will not perform.".to_string(),
            active_playbacks: inner.active.len(),
            locally_recorded: inner.total_recorded,
            recent: inner.recent.iter().rev().take(20).cloned().collect(),
        }
    }
}

fn finish(inner: &mut Inner, play_request_id: u64, now_ms: u64, completed: bool, reason: &str) {
    let Some(playback) = inner.active.remove(&play_request_id) else {
        return;
    };
    let position = playback.last_position_ms;
    push_recent(inner, playback.finish(position, now_ms, completed, reason));
}

fn push_recent(inner: &mut Inner, event: PlaybackAudit) {
    inner.total_recorded = inner.total_recorded.saturating_add(1);
    inner.recent.push_back(event);
    while inner.recent.len() > MAX_RECENT_EVENTS {
        inner.recent.pop_front();
    }
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use librespot::core::SpotifyUri;

    use super::*;

    fn track() -> SpotifyUri {
        SpotifyUri::from_uri("spotify:track:4uLU6hMCjMI75M1A2tKUQC").unwrap()
    }

    #[tokio::test]
    async fn records_only_elapsed_listening_not_seek_distance() {
        let tracker = TelemetryTracker::default();
        tracker
            .observe_at(
                &PlayerEvent::Playing {
                    play_request_id: 1,
                    track_id: track(),
                    position_ms: 0,
                },
                1_000,
            )
            .await;
        tracker
            .observe_at(
                &PlayerEvent::Seeked {
                    play_request_id: 1,
                    track_id: track(),
                    position_ms: 180_000,
                },
                6_000,
            )
            .await;
        tracker
            .observe_at(
                &PlayerEvent::EndOfTrack {
                    play_request_id: 1,
                    track_id: track(),
                },
                9_000,
            )
            .await;

        let status = tracker.status().await;
        assert_eq!(status.locally_recorded, 1);
        assert_eq!(status.recent[0].played_ms, 8_000);
        assert_eq!(status.recent[0].final_position_ms, 180_000);
        assert!(status.recent[0].completed);
    }

    #[tokio::test]
    async fn pause_time_is_not_counted_and_status_is_explicitly_blocked() {
        let tracker = TelemetryTracker::default();
        tracker
            .observe_at(
                &PlayerEvent::Playing {
                    play_request_id: 9,
                    track_id: track(),
                    position_ms: 10_000,
                },
                100,
            )
            .await;
        tracker
            .observe_at(
                &PlayerEvent::Paused {
                    play_request_id: 9,
                    track_id: track(),
                    position_ms: 12_000,
                },
                2_100,
            )
            .await;
        tracker
            .observe_at(
                &PlayerEvent::Stopped {
                    play_request_id: 9,
                    track_id: track(),
                },
                50_000,
            )
            .await;

        let status = tracker.status().await;
        assert!(!status.delivery_available);
        assert_eq!(status.recent[0].played_ms, 2_000);
        assert_eq!(status.recent[0].end_reason, "stopped");
    }

    #[tokio::test]
    async fn finish_all_closes_active_playbacks_once() {
        let tracker = TelemetryTracker::default();
        tracker
            .observe_at(
                &PlayerEvent::Loading {
                    play_request_id: 3,
                    track_id: track(),
                    position_ms: 42,
                },
                1,
            )
            .await;
        tracker.finish_all("logout").await;
        tracker.finish_all("logout").await;

        let status = tracker.status().await;
        assert_eq!(status.active_playbacks, 0);
        assert_eq!(status.locally_recorded, 1);
        assert_eq!(status.recent[0].end_reason, "logout");
    }
}
