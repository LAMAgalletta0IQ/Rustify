//! Optional Last.fm enrichment for the Listening DNA profile.
//!
//! # Why this exists, and why it is optional
//!
//! [`crate::dna`] builds its profile from Spotify's `genres` array on
//! `/me/top/artists`. That array is the weakest input in the whole feature:
//! Spotify leaves it **empty** for a large share of artists — reliably so for
//! smaller ones — so a listener with niche taste gets a flatter, less
//! interesting shape than a mainstream one, for reasons that have nothing to
//! do with their listening.
//!
//! Last.fm's `artist.getTopTags` fills that gap with community tags, which are
//! both denser and more descriptive. But it needs an API key the user has to
//! register themselves, so it can never be a requirement: with no key
//! configured this module is not called at all and DNA works exactly as
//! before, from Spotify tags alone. `ListeningDna::tag_source` reports which
//! happened, so the UI never implies enrichment that didn't occur.
//!
//! # What is deliberately *not* done
//!
//! - **No scrobbling, no authenticated calls, no session key.** Only the
//!   unauthenticated read method `artist.getTopTags` is used. That takes the
//!   API key alone; the shared secret and the auth handshake are not involved,
//!   so there is nothing here that could act on the user's Last.fm account.
//! - **No username is sent.** Only artist names the user already has on screen.
//! - **No tempo/mood.** Last.fm does not publish those either. Nothing here
//!   substitutes for the withdrawn Spotify audio-features endpoint; it only
//!   improves tag coverage.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use futures_util::{stream, StreamExt};
use serde::Deserialize;
use tokio::sync::RwLock;

const ENDPOINT: &str = "https://ws.audioscrobbler.com/2.0/";

/// Artists to enrich per profile build. The DNA sample is 50, but tag
/// distributions converge well before that and each artist is one request —
/// 20 keeps a cold profile inside a couple of seconds without materially
/// changing the result.
const MAX_ARTISTS: usize = 20;

/// Concurrent requests. Last.fm asks for no more than ~5 calls/second per key
/// averaged over five minutes; four in flight against a ~200 ms round trip
/// sits under that, and the cache means a warm profile issues none at all.
const CONCURRENCY: usize = 4;

/// Tags below this count are long-tail noise — a handful of users applying a
/// term to one artist. Last.fm normalises `count` to 0-100 within an artist.
const MIN_TAG_COUNT: u32 = 20;

/// Tags kept per artist. Without a cap, a heavily-tagged famous artist would
/// contribute ten times the weight of an obscure one to every share the DNA
/// computes, which is a popularity effect masquerading as a taste signal.
const MAX_TAGS_PER_ARTIST: usize = 8;

/// Whole request budget for one enrichment pass. Exceeding it yields whatever
/// came back in time; DNA must never sit waiting on an optional third party.
const TOTAL_TIMEOUT: Duration = Duration::from_secs(6);

const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);

/// Cached tags stay usable for a day. An artist's community tags move on a
/// scale of months, and the point of the cache is that reopening Profile
/// costs nothing.
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Last.fm's tag vocabulary is user-generated and full of terms that describe
/// the *listener's relationship* to the artist rather than the music. They are
/// among the most-applied tags on the whole site, so leaving them in would let
/// "seen live" outrank every real genre.
///
/// Matched against the whole normalised tag, not as substrings: "favourite"
/// must not eat "favourite-adjacent" genre names, and more importantly a
/// substring rule on "rock" or "pop" here would be catastrophic.
const NON_GENRE_TAGS: &[&str] = &[
    "seen live",
    "favourites",
    "favorites",
    "favourite",
    "favorite",
    "favourite songs",
    "favorite songs",
    "favourite artists",
    "favorite artists",
    "awesome",
    "amazing",
    "beautiful",
    "love",
    "loved",
    "love at first listen",
    "my music",
    "my favourites",
    "my favorites",
    "best",
    "the best",
    "good",
    "great",
    "cool",
    "epic",
    "masterpiece",
    "under 2000 listeners",
    "spotify",
    "albums i own",
    "vinyl",
    "check out",
    "to check out",
    "want to see live",
    "male vocalists",
    "female vocalists",
    "male vocalist",
    "female vocalist",
    "usa",
    "uk",
    "british",
    "american",
    "australian",
    "canadian",
    "swedish",
    "japanese",
    "german",
    "french",
    "italian",
    "norwegian",
    "finnish",
    "russian",
    "korean",
    "00s",
    "90s",
    "80s",
    "70s",
    "60s",
    "10s",
    "20s",
    "2000s",
    "1990s",
    "1980s",
];

#[derive(Debug, Deserialize)]
struct TopTagsResponse {
    #[serde(default)]
    toptags: Option<TopTags>,
}

#[derive(Debug, Deserialize)]
struct TopTags {
    #[serde(default)]
    tag: Vec<Tag>,
}

#[derive(Debug, Deserialize)]
struct Tag {
    #[serde(default)]
    name: String,
    #[serde(default)]
    count: u32,
}

/// Per-artist tag cache, held on `AppState` so it outlives one Profile visit.
#[derive(Default)]
pub struct LastfmCache {
    /// Keyed by the lowercased artist name, because that is all Last.fm is
    /// given — there is no Spotify id on the other side to key by.
    tags: RwLock<HashMap<String, (Instant, Vec<String>)>>,
}

impl LastfmCache {
    async fn get(&self, artist: &str) -> Option<Vec<String>> {
        let cache = self.tags.read().await;
        let (stored_at, tags) = cache.get(artist)?;
        (stored_at.elapsed() < CACHE_TTL).then(|| tags.clone())
    }

    async fn put(&self, artist: String, tags: Vec<String>) {
        self.tags
            .write()
            .await
            .insert(artist, (Instant::now(), tags));
    }

    /// Dropped when the key changes or is cleared, so a bad key's empty
    /// results are not remembered as if they were real answers.
    pub async fn clear(&self) {
        self.tags.write().await.clear();
    }
}

fn is_genre_tag(tag: &str) -> bool {
    !NON_GENRE_TAGS.contains(&tag) && !tag.is_empty() && tag.len() <= 40
}

/// Top genre tags for one artist. `None` means "no usable answer" — a network
/// failure, a rejected key, an unknown artist and an artist with no tags are
/// deliberately not distinguished here, because the caller's response to all
/// four is identical: carry on with Spotify's tags for that artist.
async fn artist_tags(http: &reqwest::Client, api_key: &str, artist: &str) -> Option<Vec<String>> {
    let response = http
        .get(ENDPOINT)
        .query(&[
            ("method", "artist.gettoptags"),
            ("artist", artist),
            ("api_key", api_key),
            ("autocorrect", "1"),
            ("format", "json"),
        ])
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .ok()?;

    let status = response.status();
    if !status.is_success() {
        // Logged rather than surfaced: an invalid key must not turn the whole
        // Profile view into an error, and this is the only place it shows.
        log::debug!(target: "lastfm", "artist.getTopTags({artist}) -> {status}");
        return None;
    }
    let parsed: TopTagsResponse = response.json().await.ok()?;
    let mut tags: Vec<Tag> = parsed.toptags?.tag;
    tags.sort_unstable_by_key(|tag| std::cmp::Reverse(tag.count));

    let kept: Vec<String> = tags
        .into_iter()
        .filter(|tag| tag.count >= MIN_TAG_COUNT)
        .map(|tag| tag.name.trim().to_lowercase())
        .filter(|tag| is_genre_tag(tag))
        .take(MAX_TAGS_PER_ARTIST)
        .collect();
    Some(kept)
}

/// Tags for up to [`MAX_ARTISTS`] of the given names, keyed by the lowercased
/// name so the caller can match them back.
///
/// Never returns an error. Every failure mode — no network, a rejected key, a
/// slow response past the budget — yields fewer entries, and the caller treats
/// a missing entry as "use Spotify's tags for this artist".
pub async fn enrich_tags(
    http: &reqwest::Client,
    cache: &LastfmCache,
    api_key: &str,
    artists: &[String],
) -> HashMap<String, Vec<String>> {
    let wanted: Vec<String> = artists
        .iter()
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
        .take(MAX_ARTISTS)
        .collect();

    let mut resolved: HashMap<String, Vec<String>> = HashMap::new();
    let mut pending: Vec<String> = Vec::new();
    for name in wanted {
        match cache.get(&name).await {
            Some(tags) => {
                resolved.insert(name, tags);
            }
            None => pending.push(name),
        }
    }
    if pending.is_empty() {
        return resolved;
    }

    let fetched = stream::iter(pending)
        .map(|name| async move {
            let tags = artist_tags(http, api_key, &name).await;
            (name, tags)
        })
        .buffer_unordered(CONCURRENCY)
        .collect::<Vec<_>>();

    // Whatever arrives inside the budget is used; the rest is simply absent.
    let fetched = match tokio::time::timeout(TOTAL_TIMEOUT, fetched).await {
        Ok(results) => results,
        Err(_) => {
            log::debug!(target: "lastfm", "tag enrichment exceeded its {TOTAL_TIMEOUT:?} budget");
            Vec::new()
        }
    };

    for (name, tags) in fetched {
        if let Some(tags) = tags {
            cache.put(name.clone(), tags.clone()).await;
            resolved.insert(name, tags);
        }
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relationship_tags_but_keeps_genres() {
        assert!(!is_genre_tag("seen live"));
        assert!(!is_genre_tag("female vocalists"));
        assert!(!is_genre_tag("00s"));
        assert!(is_genre_tag("melodic death metal"));
        assert!(is_genre_tag("shoegaze"));
        // The blocklist matches whole tags, never substrings — a substring
        // rule on "uk" or "love" would remove "uk garage" and "lovers rock".
        assert!(is_genre_tag("uk garage"));
        assert!(is_genre_tag("lovers rock"));
    }

    #[test]
    fn rejects_empty_and_absurdly_long_tags() {
        assert!(!is_genre_tag(""));
        assert!(!is_genre_tag(&"a".repeat(41)));
    }
}
