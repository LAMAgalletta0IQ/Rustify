use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::library::RecentActivityItem;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalActivity {
    item: RecentActivityItem,
    opens: u32,
    plays: u32,
    last_action_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn path(data_dir: &Path, account: &str) -> std::path::PathBuf {
    let safe = account
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    data_dir.join(format!("relevance-{safe}.json"))
}

fn read(path: &Path) -> Vec<LocalActivity> {
    std::fs::read(path)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn write(path: &Path, entries: &[LocalActivity]) -> AppResult<()> {
    let raw = serde_json::to_vec_pretty(entries)
        .map_err(|e| AppError::Other(format!("could not serialize relevance history: {e}")))?;
    std::fs::write(path, raw)?;
    Ok(())
}

pub fn record(
    data_dir: &Path,
    account: &str,
    item: RecentActivityItem,
    played: bool,
) -> AppResult<()> {
    let file = path(data_dir, account);
    let mut entries = read(&file);
    if let Some(entry) = entries.iter_mut().find(|entry| entry.item.uri == item.uri) {
        entry.item = item;
        entry.opens = entry.opens.saturating_add(1);
        entry.plays = entry.plays.saturating_add(u32::from(played));
        entry.last_action_ms = now_ms();
    } else {
        entries.push(LocalActivity {
            item,
            opens: 1,
            plays: u32::from(played),
            last_action_ms: now_ms(),
        });
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.last_action_ms));
    entries.truncate(100);
    write(&file, &entries)
}

pub fn rank(
    data_dir: &Path,
    account: &str,
    recent: Vec<RecentActivityItem>,
    limit: usize,
) -> Vec<RecentActivityItem> {
    let local = read(&path(data_dir, account));
    let now = now_ms();
    let mut scores: HashMap<String, (f64, RecentActivityItem)> = HashMap::new();

    for (index, item) in recent.into_iter().enumerate() {
        let score = 120.0 / (index + 2) as f64 + item.frequency.min(8) as f64 * 9.0;
        scores
            .entry(item.uri.clone())
            .and_modify(|entry| entry.0 += score)
            .or_insert((score, item));
    }
    for entry in local {
        let age_days = now.saturating_sub(entry.last_action_ms) as f64 / 86_400_000.0;
        let recency = 70.0 / (1.0 + age_days / 7.0);
        let score = recency + (entry.plays.min(20) * 8 + entry.opens.min(30) * 2) as f64;
        scores
            .entry(entry.item.uri.clone())
            .and_modify(|existing| {
                existing.0 += score;
                if entry.last_action_ms > 0 {
                    existing.1 = entry.item.clone();
                }
            })
            .or_insert((score, entry.item));
    }

    let mut ranked = scores.into_values().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.uri.cmp(&b.1.uri)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, item)| item)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(uri: &str, frequency: u32) -> RecentActivityItem {
        RecentActivityItem {
            kind: "album".into(),
            uri: uri.into(),
            id: uri.into(),
            name: uri.into(),
            subtitle: String::new(),
            image_url: None,
            last_played_at: String::new(),
            track_uri: None,
            frequency,
        }
    }

    #[test]
    fn repeated_recent_contexts_rank_above_single_older_items() {
        let dir = std::env::temp_dir().join(format!("rustify-relevance-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let ranked = rank(
            &dir,
            "account",
            vec![item("new", 1), item("frequent", 6)],
            2,
        );
        assert_eq!(ranked[0].uri, "frequent");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ranking_deduplicates_uris() {
        let dir = std::env::temp_dir().join(format!("rustify-relevance-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let ranked = rank(&dir, "account", vec![item("same", 1), item("same", 2)], 6);
        assert_eq!(ranked.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
