use dioxus::prelude::*;
use crate::routes::Route;
use crate::stores::{nostr_client, music_player};
use crate::stores::music_player::{MusicTrack, KIND_MUSIC_VOTE};
use crate::stores::nostr_music::TrackSource;
use crate::services::wavlake::WavlakeAPI;
use nostr_sdk::{Filter, Kind, TagKind, Timestamp, SingleLetterTag, Alphabet};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Track reference extracted from vote event
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TrackRef {
    /// Nostr track coordinate (kind:pubkey:d-tag)
    Nostr(String),
    /// Wavlake track ID
    Wavlake(String),
}

/// Vote data for a single track
#[derive(Clone, Debug, PartialEq)]
struct VoteData {
    track_ref: TrackRef,
    title: String,
    artist: String,
    image: Option<String>,
    votes: usize,
    voters: HashSet<String>, // O(1) lookup for deduplication
}

/// Leaderboard entry with resolved track for playback
#[derive(Clone, Debug, PartialEq)]
struct LeaderboardEntry {
    rank: usize,
    vote_data: VoteData,
    /// Resolved MusicTrack for playback (None if we couldn't resolve it)
    music_track: Option<MusicTrack>,
}

#[component]
pub fn MusicLeaderboard() -> Element {
    let mut loading = use_signal(|| true);
    let mut leaderboard = use_signal(|| Vec::<LeaderboardEntry>::new());
    let mut error_msg = use_signal(|| None::<String>);

    // Fetch vote events and build leaderboard
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error_msg.set(None);

            match fetch_leaderboard_data().await {
                Ok(entries) => {
                    log::info!("Loaded {} leaderboard entries", entries.len());
                    leaderboard.set(entries);
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load leaderboard: {}", e);
                    error_msg.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    rsx! {
        div {
            class: "max-w-4xl mx-auto p-4 space-y-6",

            div {
                class: "flex items-center justify-between",
                h1 {
                    class: "text-3xl font-bold",
                    "Music Leaderboard"
                }
                Link {
                    to: Route::MusicHome {},
                    class: "px-4 py-2 bg-muted hover:bg-muted/80 rounded-full transition",
                    "Back to Music"
                }
            }

            div {
                class: "bg-card p-6 rounded-lg border border-border",
                p {
                    class: "text-muted-foreground",
                    "Top 10 most voted songs of the week from the community."
                }
                p {
                    class: "text-sm text-muted-foreground mt-2",
                    "One vote per person. Voting for a new song replaces your previous vote."
                }
            }

            if let Some(err) = error_msg.read().as_ref() {
                div {
                    class: "bg-destructive/10 border border-destructive text-destructive p-4 rounded-lg",
                    "Failed to load leaderboard: {err}"
                }
            }

            if *loading.read() {
                div {
                    class: "space-y-4",
                    for i in 0..10 {
                        div {
                            key: "{i}",
                            class: "bg-card p-4 rounded-lg border border-border animate-pulse",
                            div {
                                class: "flex items-center gap-4",
                                div { class: "w-8 h-8 bg-muted rounded-full" }
                                div { class: "w-14 h-14 bg-muted rounded-lg" }
                                div {
                                    class: "flex-1 space-y-2",
                                    div { class: "h-4 bg-muted rounded w-48" }
                                    div { class: "h-3 bg-muted rounded w-32" }
                                }
                            }
                        }
                    }
                }
            } else if leaderboard.read().is_empty() {
                div {
                    class: "text-center py-12",
                    div {
                        class: "text-6xl mb-4",
                        "🏆"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2",
                        "No Votes Yet"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Be the first to vote for your favorite songs!"
                    }
                }
            } else {
                div {
                    class: "space-y-3",
                    for entry in leaderboard.read().iter() {
                        LeaderboardCard {
                            key: "{entry.rank}",
                            entry: entry.clone()
                        }
                    }
                }
            }
        }
    }
}

/// Individual leaderboard card component
#[component]
fn LeaderboardCard(entry: LeaderboardEntry) -> Element {
    let rank_class = match entry.rank {
        1 => "w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold bg-yellow-500 text-yellow-900 flex-shrink-0",
        2 => "w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold bg-gray-400 text-gray-900 flex-shrink-0",
        3 => "w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold bg-amber-600 text-amber-100 flex-shrink-0",
        _ => "w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold bg-primary/10 text-primary flex-shrink-0",
    };

    let source_badge = match &entry.vote_data.track_ref {
        TrackRef::Nostr(_) => Some("Nostr"),
        TrackRef::Wavlake(_) => Some("Wavlake"),
    };

    rsx! {
        div {
            class: "bg-card p-4 rounded-lg border border-border hover:shadow-md transition-all duration-200",

            div {
                class: "flex items-center gap-4",

                // Rank badge
                div {
                    class: "{rank_class}",
                    "{entry.rank}"
                }

                // Album art
                div {
                    class: "w-14 h-14 bg-muted rounded-lg flex items-center justify-center overflow-hidden relative flex-shrink-0",
                    if let Some(ref image) = entry.vote_data.image {
                        img {
                            src: "{image}",
                            alt: "{entry.vote_data.title}",
                            class: "w-full h-full object-cover"
                        }
                    } else {
                        div {
                            class: "text-2xl",
                            "🎵"
                        }
                    }
                }

                // Track info
                div {
                    class: "flex-1 min-w-0",
                    h3 {
                        class: "font-medium truncate",
                        "{entry.vote_data.title}"
                    }
                    p {
                        class: "text-sm text-muted-foreground truncate",
                        "{entry.vote_data.artist}"
                    }
                    div {
                        class: "flex items-center gap-2 mt-1",
                        span {
                            class: "text-xs px-2 py-1 bg-red-500/10 text-red-500 rounded-full",
                            "❤️ {entry.vote_data.votes}"
                        }
                        if let Some(badge) = source_badge {
                            span {
                                class: "text-xs px-2 py-1 bg-muted text-muted-foreground rounded-full",
                                "{badge}"
                            }
                        }
                    }
                }

                // Play button (only if we have a resolved track)
                if let Some(ref track) = entry.music_track {
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-full hover:bg-primary/90 transition",
                        onclick: {
                            let t = track.clone();
                            move |_| {
                                music_player::play_track(t.clone(), None, None);
                            }
                        },
                        "▶ Play"
                    }
                }
            }
        }
    }
}

/// Fetch and aggregate leaderboard data from Kind 33169 vote events
async fn fetch_leaderboard_data() -> Result<Vec<LeaderboardEntry>, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;

    let one_week_ago = Timestamp::now() - Duration::from_secs(7 * 24 * 60 * 60);

    log::info!("Fetching vote events (kind {}) since {}", KIND_MUSIC_VOTE, one_week_ago.as_secs());

    let filter = Filter::new()
        .kind(Kind::from(KIND_MUSIC_VOTE))
        .identifier("music-vote")
        .since(one_week_ago)
        .limit(1000);

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch vote events: {}", e))?;

    log::info!("Fetched {} vote events", events.len());

    // Aggregate votes by track reference
    // Key: track reference string, Value: VoteData
    let mut vote_map: HashMap<String, VoteData> = HashMap::new();

    for event in events {
        // Extract cached metadata
        let title = event.tags
            .find(TagKind::custom("title"))
            .and_then(|t| t.content())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let artist = event.tags
            .find(TagKind::custom("artist"))
            .and_then(|t| t.content())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let image = event.tags
            .find(TagKind::custom("image"))
            .and_then(|t| t.content())
            .map(|s| s.to_string());

        let source_kind = event.tags
            .find(TagKind::custom("k"))
            .and_then(|t| t.content())
            .map(|s| s.to_string());

        // Determine track reference based on source
        let track_ref = if source_kind.as_deref() == Some("wavlake") {
            // Wavlake track - look for track_id or parse from r tag
            let track_id = event.tags
                .find(TagKind::custom("track_id"))
                .and_then(|t| t.content())
                .map(|s| s.to_string())
                .or_else(|| {
                    // Try to extract from r tag URL
                    event.tags
                        .find(TagKind::custom("r"))
                        .and_then(|t| t.content())
                        .and_then(|url| url.split('/').last())
                        .map(|s| s.to_string())
                });

            match track_id {
                Some(id) => TrackRef::Wavlake(id),
                None => continue, // Skip if we can't identify the track
            }
        } else {
            // Nostr track - look for 'a' tag (coordinate)
            let coordinate = event.tags
                .find(TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::A)))
                .and_then(|t| t.content())
                .map(|s| s.to_string());

            match coordinate {
                Some(coord) => TrackRef::Nostr(coord),
                None => continue, // Skip if we can't identify the track
            }
        };

        let track_key = match &track_ref {
            TrackRef::Nostr(coord) => coord.clone(),
            TrackRef::Wavlake(id) => format!("wavlake:{}", id),
        };

        let voter_pubkey = event.pubkey.to_hex();

        vote_map.entry(track_key)
            .and_modify(|data| {
                // Each pubkey only counts once (addressable events mean latest vote wins per user)
                // But we still dedupe in case of relay inconsistencies
                // HashSet provides O(1) contains check
                if !data.voters.contains(&voter_pubkey) {
                    data.votes += 1;
                    data.voters.insert(voter_pubkey.clone());
                }
            })
            .or_insert(VoteData {
                track_ref,
                title,
                artist,
                image,
                votes: 1,
                voters: HashSet::from([voter_pubkey]),
            });
    }

    // Sort by votes and take top 10
    let mut sorted_votes: Vec<VoteData> = vote_map.into_values().collect();
    sorted_votes.sort_by(|a, b| b.votes.cmp(&a.votes));
    let top_10 = sorted_votes.into_iter().take(10).collect::<Vec<_>>();

    log::info!("Top {} tracks identified, resolving for playback...", top_10.len());

    // Resolve tracks for playback
    let api = WavlakeAPI::new();
    let mut entries = Vec::new();

    for (index, vote_data) in top_10.into_iter().enumerate() {
        let music_track = match &vote_data.track_ref {
            TrackRef::Wavlake(track_id) => {
                // Fetch from Wavlake API
                match api.get_track(track_id).await {
                    Ok(wt) => Some(MusicTrack::from(wt)),
                    Err(e) => {
                        log::warn!("Failed to fetch Wavlake track {}: {}", track_id, e);
                        // Create a minimal track from cached data for display
                        Some(create_fallback_track(&vote_data, TrackSource::Wavlake {
                            artist_id: String::new(),
                            album_id: String::new(),
                        }))
                    }
                }
            }
            TrackRef::Nostr(coordinate) => {
                // Parse coordinate and create track source
                // Format: "36787:pubkey:d-tag"
                let parts: Vec<&str> = coordinate.split(':').collect();
                if parts.len() >= 3 {
                    let pubkey = parts[1].to_string();
                    let d_tag = parts[2..].join(":"); // Handle d-tags with colons

                    // Try to fetch the actual track first for proper playback
                    match crate::stores::nostr_music::fetch_nostr_track_by_coordinate(&pubkey, &d_tag).await {
                        Ok(Some(nostr_track)) => {
                            log::info!("Successfully fetched Nostr track: {}", coordinate);
                            Some(MusicTrack::from(nostr_track))
                        }
                        Ok(None) => {
                            log::warn!("Nostr track not found, using fallback: {}", coordinate);
                            Some(create_fallback_track(&vote_data, TrackSource::Nostr {
                                coordinate: coordinate.clone(),
                                pubkey,
                                d_tag,
                            }))
                        }
                        Err(e) => {
                            log::warn!("Failed to fetch Nostr track {}: {}, using fallback", coordinate, e);
                            Some(create_fallback_track(&vote_data, TrackSource::Nostr {
                                coordinate: coordinate.clone(),
                                pubkey,
                                d_tag,
                            }))
                        }
                    }
                } else {
                    log::warn!("Invalid Nostr coordinate: {}", coordinate);
                    None
                }
            }
        };

        entries.push(LeaderboardEntry {
            rank: index + 1,
            vote_data,
            music_track,
        });
    }

    Ok(entries)
}

/// Create a fallback MusicTrack from cached vote data
fn create_fallback_track(vote_data: &VoteData, source: TrackSource) -> MusicTrack {
    let id = match &vote_data.track_ref {
        TrackRef::Wavlake(id) => id.clone(),
        TrackRef::Nostr(coord) => coord.clone(),
    };

    MusicTrack {
        id,
        title: vote_data.title.clone(),
        artist: vote_data.artist.clone(),
        album: None,
        media_url: String::new(), // Will need to be resolved for playback
        album_art_url: vote_data.image.clone(),
        artist_art_url: vote_data.image.clone(),
        duration: None,
        artist_id: None,
        album_id: None,
        artist_npub: None,
        source,
        msat_total: None,
        created_at: None,
    }
}
