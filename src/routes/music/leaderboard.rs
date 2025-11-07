use dioxus::prelude::*;
use crate::routes::Route;
use crate::stores::{nostr_client, music_player};
use crate::services::wavlake::{WavlakeAPI, WavlakeTrack};
use nostr_sdk::{Filter, Kind, TagKind, Timestamp};
use std::collections::HashMap;
use std::time::Duration;

/// Vote data for a single track
#[derive(Clone, Debug, PartialEq)]
struct VoteData {
    track_id: String,
    track_title: String,
    track_artist: String,
    votes: usize,
    voters: Vec<String>,
}

/// Leaderboard entry with track details
#[derive(Clone, Debug, PartialEq)]
struct LeaderboardEntry {
    rank: usize,
    vote_data: VoteData,
    track: Option<WavlakeTrack>,
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
                    "🏆 Music Leaderboard"
                }
                Link {
                    to: Route::MusicHome {},
                    class: "px-4 py-2 bg-muted hover:bg-muted/80 rounded-full transition",
                    "← Back to Music"
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
                    "Votes are powered by Nostr (kind 30003). One vote per track per user."
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
                        {
                            let rank_class = match entry.rank {
                                1 => "w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold bg-yellow-500 text-yellow-900 flex-shrink-0",
                                2 => "w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold bg-gray-400 text-gray-900 flex-shrink-0",
                                3 => "w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold bg-amber-600 text-amber-100 flex-shrink-0",
                                _ => "w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold bg-primary/10 text-primary flex-shrink-0",
                            };
                            rsx! { div {
                            key: "{entry.vote_data.track_id}",
                            class: "bg-card p-4 rounded-lg border border-border hover:shadow-md transition-all duration-200",

                            div {
                                class: "flex items-center gap-4",

                                div {
                                    class: "{rank_class}",
                                    "{entry.rank}"
                                }

                                div {
                                    class: "w-14 h-14 bg-muted rounded-lg flex items-center justify-center overflow-hidden relative flex-shrink-0",
                                    if let Some(ref track) = entry.track {
                                        img {
                                            src: "{track.album_art_url}",
                                            alt: "{entry.vote_data.track_title}",
                                            class: "w-full h-full object-cover"
                                        }
                                    } else {
                                        div {
                                            class: "text-2xl",
                                            "🎵"
                                        }
                                    }
                                }

                                div {
                                    class: "flex-1 min-w-0",
                                    h3 {
                                        class: "font-medium truncate",
                                        "{entry.vote_data.track_title}"
                                    }
                                    p {
                                        class: "text-sm text-muted-foreground truncate",
                                        "{entry.vote_data.track_artist}"
                                    }
                                    div {
                                        class: "flex items-center gap-2 mt-1",
                                        span {
                                            class: "text-xs px-2 py-1 bg-red-500/10 text-red-500 rounded-full flex items-center gap-1",
                                            "❤️ {entry.vote_data.votes}"
                                        }
                                    }
                                }

                                if let Some(ref track) = entry.track {
                                    button {
                                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-full hover:bg-primary/90 transition",
                                        onclick: {
                                            let t = track.clone();
                                            move |_| {
                                                let music_track = music_player::MusicTrack::from(t.clone());
                                                music_player::play_track(music_track, None, None);
                                            }
                                        },
                                        "▶ Play"
                                    }
                                }
                            }
                        }}
                        }
                    }
                }
            }
        }
    }
}

/// Fetch and aggregate leaderboard data
async fn fetch_leaderboard_data() -> Result<Vec<LeaderboardEntry>, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;

    let one_week_ago = Timestamp::now() - Duration::from_secs(7 * 24 * 60 * 60);

    log::info!("Fetching vote events since {}", one_week_ago.as_secs());

    let filter = Filter::new()
        .kind(Kind::from(30003))
        .identifier("peachy-song-vote")
        .since(one_week_ago)
        .limit(1000);

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch vote events: {}", e))?;

    log::info!("Fetched {} vote events", events.len());

    let mut vote_map: HashMap<String, VoteData> = HashMap::new();

    for event in events {
        let track_id = event.tags
            .find(TagKind::custom("track_id"))
            .and_then(|t| t.content())
            .map(|s| s.to_string());

        let track_title = event.tags
            .find(TagKind::custom("track_title"))
            .and_then(|t| t.content())
            .map(|s| s.to_string());

        let track_artist = event.tags
            .find(TagKind::custom("track_artist"))
            .and_then(|t| t.content())
            .map(|s| s.to_string());

        if track_id.is_none() || track_title.is_none() || track_artist.is_none() {
            continue;
        }

        let track_id = track_id.unwrap();
        let track_title = track_title.unwrap();
        let track_artist = track_artist.unwrap();
        let voter_pubkey = event.pubkey.to_hex();

        vote_map.entry(track_id.clone())
            .and_modify(|data| {
                if !data.voters.contains(&voter_pubkey) {
                    data.votes += 1;
                    data.voters.push(voter_pubkey.clone());
                }
            })
            .or_insert(VoteData {
                track_id,
                track_title,
                track_artist,
                votes: 1,
                voters: vec![voter_pubkey],
            });
    }

    let mut sorted_votes: Vec<VoteData> = vote_map.into_values().collect();
    sorted_votes.sort_by(|a, b| b.votes.cmp(&a.votes));
    let top_10 = sorted_votes.into_iter().take(10).collect::<Vec<_>>();

    log::info!("Top 10 tracks identified, fetching details from Wavlake...");

    let api = WavlakeAPI::new();
    let mut entries = Vec::new();

    for (index, vote_data) in top_10.into_iter().enumerate() {
        let track = match api.get_track(&vote_data.track_id).await {
            Ok(t) => Some(t),
            Err(e) => {
                log::warn!("Failed to fetch track {}: {}", vote_data.track_id, e);
                None
            }
        };

        entries.push(LeaderboardEntry {
            rank: index + 1,
            vote_data,
            track,
        });
    }

    Ok(entries)
}
