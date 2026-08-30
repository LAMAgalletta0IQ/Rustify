//! "Listening DNA" — a taste profile derived from the user's top artists and
//! tracks.
//!
//! # Why this is not Spotify's DNA
//!
//! Spotify's own feature is built on `GET /v1/audio-features`, which returns
//! per-track danceability / energy / valence / acousticness / tempo. That
//! endpoint is **not available to this app**: Spotify closed it to apps in
//! Development Mode, which is what every self-registered Client ID is until it
//! passes an extension review, and Rustify's whole auth design is that each
//! user registers their own app (see CLAUDE.md, "Two credentials, not one").
//! There is no request this app can make that returns those numbers, and
//! scraping the values out of the web player is both against the Developer
//! Terms and a reliable way to get an IP banned — so it is not attempted.
//!
//! What *is* still available on the ordinary Web API is enough to build an
//! honest profile without inventing anything:
//!
//! - `/me/top/artists` carries `genres` (free-text tags Spotify assigns to the
//!   artist) and `popularity` (0-100).
//! - `/me/top/tracks` carries the album's `release_date` and `explicit`.
//!
//! Every axis below is computed from those, and the axis names deliberately
//! describe *what was measured* (`Mainstream` = mean artist popularity) rather
//! than borrowing Spotify's psychoacoustic vocabulary. `Intensity` and
//! `Mellow` are genre-tag keyword matches — a coarse proxy for what
//! energy/acousticness measured directly, and labelled as such in the UI.
//!
//! Adding a third-party source (Last.fm tags, MusicBrainz, AcousticBrainz)
//! would sharpen this, but it means another API key for the user to register
//! on top of the Spotify one, so it is deliberately not a dependency of the
//! feature existing at all.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::webapi::WebApi;

/// How many top artists/tracks the profile is built from. 50 is the Web API's
/// per-request maximum for both endpoints, and the profile is noticeably noisy
/// below roughly 20 — small samples let one outlier artist dominate an axis.
const SAMPLE_SIZE: u32 = 50;

/// Unique genre tags that count as maximum variety. Chosen from what the
/// endpoint actually returns: a heavy single-genre listener lands around 5-8
/// distinct tags across 50 artists, a broad one comfortably passes 30.
const VARIETY_CEILING: f32 = 30.0;

/// A track counts as "fresh" if its album came out within this many years.
const FRESH_YEARS: i32 = 2;

/// Genre-tag substrings read as high-energy. Matched case-insensitively as
/// substrings because Spotify's tags are compounds ("melodic death metal",
/// "uk drill", "hardcore punk") rather than a closed vocabulary.
const INTENSE_TAGS: &[&str] = &[
    "metal", "punk", "hardcore", "rock", "rap", "drill", "trap", "techno", "house", "edm",
    "dubstep", "drum and bass", "breakcore", "hyperpop", "industrial", "grime", "electro",
    "hardstyle", "phonk",
];

/// Genre-tag substrings read as low-energy / acoustic.
const MELLOW_TAGS: &[&str] = &[
    "acoustic",
    "folk",
    "singer-songwriter",
    "ambient",
    "classical",
    "jazz",
    "soul",
    "bossa",
    "chill",
    "lo-fi",
    "lofi",
    "sleep",
    "piano",
    "choral",
    "new age",
    "shoegaze",
    "slowcore",
];

/// One axis of the radar. `value` is 0-100.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnaAxis {
    /// Stable identifier, so the UI can order/style axes without matching on
    /// the label text.
    pub id: &'static str,
    pub label: &'static str,
    pub value: u8,
    /// One line explaining exactly what was measured. Shown on hover — the
    /// point of the feature is that it does not pretend to be Spotify's.
    pub basis: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnaGenre {
    pub name: String,
    /// Share of the sampled artists carrying this tag, 0-100.
    pub weight: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListeningDna {
    pub axes: Vec<DnaAxis>,
    pub genres: Vec<DnaGenre>,
    /// How many top artists and tracks the numbers came from, so the UI can
    /// say "not enough listening history yet" honestly rather than drawing a
    /// confident-looking shape from three tracks.
    pub artist_sample: u32,
    pub track_sample: u32,
    /// Present when the profile is too thin to be meaningful.
    pub sparse: bool,
}

#[derive(Debug, Deserialize)]
struct Page<T> {
    items: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct WireArtist {
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    popularity: u32,
}

#[derive(Debug, Deserialize)]
struct WireTrack {
    #[serde(default)]
    artists: Vec<WireTrackArtist>,
    #[serde(default)]
    album: Option<WireAlbum>,
}

#[derive(Debug, Deserialize)]
struct WireTrackArtist {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireAlbum {
    #[serde(default)]
    release_date: Option<String>,
}

fn clamp(value: f32) -> u8 {
    value.clamp(0.0, 100.0).round() as u8
}

/// Share of `tags` (already lowercased) that contain any of `needles`, as a
/// percentage of *all* tags seen. Not of matching tags — a listener whose
/// tags are 90% "ambient" should not score the same on Intensity as one whose
/// tags are 90% "metal" merely because both have one matching tag.
fn tag_share(tags: &[String], needles: &[&str]) -> f32 {
    if tags.is_empty() {
        return 0.0;
    }
    let hits = tags
        .iter()
        .filter(|tag| needles.iter().any(|needle| tag.contains(needle)))
        .count();
    (hits as f32 / tags.len() as f32) * 100.0
}

/// Leading four-digit year of a Spotify `release_date`, which may be
/// `YYYY`, `YYYY-MM` or `YYYY-MM-DD` depending on `release_date_precision`.
fn release_year(date: &str) -> Option<i32> {
    date.get(..4).and_then(|year| year.parse::<i32>().ok())
}

/// Builds the profile. Both requests are independent; either coming back empty
/// degrades the affected axes to 0 rather than failing the whole call, because
/// a brand-new account genuinely has no top tracks and that is not an error.
pub async fn listening_dna(api: &WebApi, token: &str, current_year: i32) -> AppResult<ListeningDna> {
    let artists: Page<WireArtist> = api
        .get(
            token,
            "/me/top/artists",
            &[
                ("limit", SAMPLE_SIZE.to_string()),
                ("time_range", "medium_term".to_string()),
            ],
        )
        .await?;
    let tracks: Page<WireTrack> = api
        .get(
            token,
            "/me/top/tracks",
            &[
                ("limit", SAMPLE_SIZE.to_string()),
                ("time_range", "medium_term".to_string()),
            ],
        )
        .await?;

    let artists = artists.items;
    let tracks = tracks.items;

    // Every genre tag, once per artist carrying it — so an artist with eight
    // tags counts eight times toward the intensity/mellow shares. That is
    // intentional: the tag count is itself a signal of how strongly the artist
    // sits in a scene.
    let tags: Vec<String> = artists
        .iter()
        .flat_map(|artist| artist.genres.iter().map(|genre| genre.to_lowercase()))
        .collect();

    let mainstream = if artists.is_empty() {
        0.0
    } else {
        artists.iter().map(|a| a.popularity as f32).sum::<f32>() / artists.len() as f32
    };

    let mut unique: HashMap<&str, u32> = HashMap::new();
    for tag in &tags {
        *unique.entry(tag.as_str()).or_default() += 1;
    }
    let variety = (unique.len() as f32 / VARIETY_CEILING) * 100.0;

    let years: Vec<i32> = tracks
        .iter()
        .filter_map(|track| track.album.as_ref()?.release_date.as_deref())
        .filter_map(release_year)
        .collect();
    let freshness = if years.is_empty() {
        0.0
    } else {
        let fresh = years
            .iter()
            .filter(|year| current_year - **year <= FRESH_YEARS)
            .count();
        (fresh as f32 / years.len() as f32) * 100.0
    };

    // Concentration: how much of the top-track list belongs to the five
    // artists the user listens to most. High means deep repeat listening, low
    // means the favourites are spread thin across many artists.
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for track in &tracks {
        for artist in &track.artists {
            if let Some(id) = artist.id.as_deref() {
                *counts.entry(id).or_default() += 1;
            }
        }
    }
    let mut top_counts: Vec<u32> = counts.values().copied().collect();
    top_counts.sort_unstable_by(|a, b| b.cmp(a));
    let attributed: u32 = top_counts.iter().sum();
    let loyalty = if attributed == 0 {
        0.0
    } else {
        (top_counts.iter().take(5).sum::<u32>() as f32 / attributed as f32) * 100.0
    };

    let axes = vec![
        DnaAxis {
            id: "mainstream",
            label: "Mainstream",
            value: clamp(mainstream),
            basis: "Average Spotify popularity score of your top artists.",
        },
        DnaAxis {
            id: "variety",
            label: "Variety",
            value: clamp(variety),
            basis: "Distinct genre tags across your top artists, against a ceiling of 30.",
        },
        DnaAxis {
            id: "freshness",
            label: "Freshness",
            value: clamp(freshness),
            basis: "Share of your top tracks released in the last two years.",
        },
        DnaAxis {
            id: "intensity",
            label: "Intensity",
            value: clamp(tag_share(&tags, INTENSE_TAGS)),
            basis: "Share of your genre tags naming a high-energy scene (rock, metal, rap, techno…).",
        },
        DnaAxis {
            id: "mellow",
            label: "Mellow",
            value: clamp(tag_share(&tags, MELLOW_TAGS)),
            basis: "Share of your genre tags naming an acoustic or ambient scene (folk, jazz, lo-fi…).",
        },
        DnaAxis {
            id: "loyalty",
            label: "Loyalty",
            value: clamp(loyalty),
            basis: "Share of your top tracks that belong to just your five most-played artists.",
        },
    ];

    let artist_total = artists.len().max(1) as f32;
    let mut genres: Vec<DnaGenre> = {
        // Weight by *artists* carrying the tag, not by raw tag count, so the
        // percentage means something a reader can check: "40% of your top
        // artists are tagged indie rock".
        let mut per_artist: HashMap<String, u32> = HashMap::new();
        for artist in &artists {
            for genre in &artist.genres {
                *per_artist.entry(genre.to_lowercase()).or_default() += 1;
            }
        }
        per_artist
            .into_iter()
            .map(|(name, count)| DnaGenre {
                name,
                weight: clamp((count as f32 / artist_total) * 100.0),
            })
            .collect()
    };
    genres.sort_by(|a, b| {
        b.weight
            .cmp(&a.weight)
            .then_with(|| a.name.cmp(&b.name))
    });
    genres.truncate(8);

    let artist_sample = artists.len() as u32;
    let track_sample = tracks.len() as u32;
    Ok(ListeningDna {
        axes,
        genres,
        artist_sample,
        track_sample,
        sparse: artist_sample < 5 || track_sample < 5,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_year_handles_every_precision() {
        assert_eq!(release_year("1998"), Some(1998));
        assert_eq!(release_year("1998-04"), Some(1998));
        assert_eq!(release_year("1998-04-21"), Some(1998));
        assert_eq!(release_year(""), None);
        assert_eq!(release_year("soon"), None);
    }

    #[test]
    fn tag_share_is_a_share_of_all_tags_not_of_matches() {
        let tags: Vec<String> = ["ambient", "ambient", "ambient", "death metal"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // One metal tag out of four is 25, not 100.
        assert_eq!(tag_share(&tags, INTENSE_TAGS).round(), 25.0);
        assert_eq!(tag_share(&tags, MELLOW_TAGS).round(), 75.0);
        assert_eq!(tag_share(&[], INTENSE_TAGS), 0.0);
    }
}
