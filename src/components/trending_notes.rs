use dioxus::prelude::*;
use crate::services::trending::{TrendingNote, get_trending_notes, truncate_content};
use crate::stores::profiles;
use crate::routes::Route;
use crate::utils::truncate_pubkey;

#[component]
pub fn TrendingNotes() -> Element {
    let mut trending_notes = use_signal(|| Vec::<TrendingNote>::new());
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| false);

    // Fetch trending notes on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error.set(false);

            match get_trending_notes(Some(10)).await {
                Ok(notes) => {
                    trending_notes.set(notes.clone());
                    loading.set(false);

                    // Prefetch author metadata for trending notes
                    use crate::utils::profile_prefetch;
                    use nostr_sdk::PublicKey;
                    let pubkeys: Vec<PublicKey> = notes.iter()
                        .filter_map(|note| PublicKey::from_hex(&note.event.pubkey).ok())
                        .collect();

                    if !pubkeys.is_empty() {
                        spawn(async move {
                            profile_prefetch::prefetch_pubkeys(pubkeys).await;
                        });
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch trending notes: {}", e);
                    error.set(true);
                    loading.set(false);
                }
            }
        });
    });

    rsx! {
        div {
            class: "border border-border rounded-lg bg-card overflow-hidden flex flex-col h-full",

            // Header
            div {
                class: "px-4 py-3 border-b border-border flex-shrink-0",
                h3 {
                    class: "text-xl font-bold flex items-center gap-2",
                    span { "📈" }
                    "Trending | nostr.wine"
                }
            }

            // Content
            div {
                class: "flex-1 overflow-y-auto scrollbar-hide",

                if *loading.read() {
                    // Loading state
                    div {
                        class: "flex items-center justify-center py-8",
                        span {
                            class: "inline-block w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin"
                        }
                    }
                } else if *error.read() {
                    // Error state
                    div {
                        class: "px-4 py-8 text-center text-sm text-muted-foreground",
                        "Trending API currently unavailable"
                    }
                } else if trending_notes.read().is_empty() {
                    // Empty state
                    div {
                        class: "px-4 py-8 text-center text-sm text-muted-foreground",
                        "No trending posts right now"
                    }
                } else {
                    // Trending notes list
                    for note in trending_notes.read().iter() {
                        TrendingNoteItem {
                            key: "{note.event.id}",
                            note: note.clone()
                        }
                    }
                }
            }

            // Show more button - always visible at bottom
            if !*loading.read() && !*error.read() && !trending_notes.read().is_empty() {
                div {
                    class: "border-t border-border flex-shrink-0",
                    Link {
                        to: Route::Trending {},
                        class: "block w-full px-4 py-3 text-blue-500 hover:bg-accent/50 transition-colors text-left text-sm",
                        "Show more"
                    }
                }
            }
        }
    }
}

/// Individual trending note item that fetches its own profile from the store
#[component]
fn TrendingNoteItem(note: TrendingNote) -> Element {
    let author_pubkey = note.event.pubkey.clone();
    let author_pubkey_for_profile = author_pubkey.clone();

    // Get profile from centralized profiles store (reactive)
    let profile = use_memo(move || {
        profiles::get_profile(&author_pubkey_for_profile)
    });

    let note_id = &note.event.id;
    let note_bech32 = match nostr_sdk::EventId::from_hex(note_id) {
        Ok(id) => {
            use nostr_sdk::ToBech32;
            id.to_bech32().unwrap_or_else(|_| note_id.clone())
        },
        Err(_) => note_id.clone(),
    };

    // Get display name from profile or fallback to truncated pubkey
    let author_name = {
        let p = profile.read();
        if let Some(ref prof) = *p {
            prof.display_name.clone()
                .or_else(|| prof.name.clone())
                .unwrap_or_else(|| truncate_pubkey(&author_pubkey))
        } else {
            truncate_pubkey(&author_pubkey)
        }
    };

    // Get picture from profile or fallback to identicon
    let picture = {
        let p = profile.read();
        p.as_ref()
            .and_then(|prof| prof.picture.clone())
            .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", author_pubkey))
    };

    let content = truncate_content(&note.event.content, 100);

    rsx! {
        Link {
            to: Route::Note { note_id: note_bech32 },
            class: "block px-4 py-3 hover:bg-accent/50 transition-colors border-b border-border last:border-0",

            div {
                class: "flex gap-3",

                // Avatar
                img {
                    src: "{picture}",
                    alt: "{author_name}",
                    class: "w-10 h-10 rounded-full flex-shrink-0 object-cover",
                    loading: "lazy"
                }

                // Content
                div {
                    class: "flex-1 min-w-0",

                    // Author name
                    div {
                        class: "text-sm font-semibold truncate mb-1",
                        "{author_name}"
                    }

                    // Note content
                    div {
                        class: "text-sm mb-2 line-clamp-2",
                        "{content}"
                    }

                    // Stats
                    if let Some(stats) = &note.stats {
                        div {
                            class: "flex items-center gap-3 text-xs text-muted-foreground",

                            if let Some(reactions) = stats.reactions {
                                if reactions > 0 {
                                    span {
                                        class: "flex items-center gap-1",
                                        "❤️ {reactions}"
                                    }
                                }
                            }

                            if let Some(replies) = stats.replies {
                                if replies > 0 {
                                    span {
                                        class: "flex items-center gap-1",
                                        "💬 {replies}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
