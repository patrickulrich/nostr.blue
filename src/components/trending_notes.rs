use crate::components::SensitiveContent;
use crate::routes::Route;
use crate::services::trending::{get_trending_notes, truncate_content, TrendingNote};
use crate::stores::{nostr_client, profiles};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
#[component]
pub fn TrendingNotes() -> Element {
    let mut trending_notes = use_signal(Vec::<TrendingNote>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| false);
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            error.set(false);
            match get_trending_notes(Some(15)).await {
                Ok(notes) => {
                    let blocked_users = nostr_client::get_blocked_users().await.unwrap_or_default();
                    let muted_posts = nostr_client::get_muted_posts().await.unwrap_or_default();
                    let filtered_notes: Vec<TrendingNote> = notes
                        .into_iter()
                        .filter(|note| {
                            if blocked_users.contains(&note.event.pubkey) {
                                return false;
                            }
                            if muted_posts.contains(&note.event.id) {
                                return false;
                            }
                            true
                        })
                        .take(10)
                        .collect();
                    trending_notes.set(filtered_notes.clone());
                    loading.set(false);
                    use crate::utils::profile_prefetch;
                    use nostr_sdk::PublicKey;
                    let pubkeys: Vec<PublicKey> = filtered_notes
                        .iter()
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
        div { class: "border border-border rounded-lg bg-card overflow-hidden flex flex-col h-full",
            div { class: "px-4 py-3 border-b border-border shrink-0",
                h3 { class: "text-xl font-bold flex items-center gap-2",
                    span { "📈" }
                    "Trending | nostr.wine"
                }
            }
            div { class: "flex-1 overflow-y-auto scrollbar-hide",
                if *loading.read() {
                    div { class: "flex items-center justify-center py-8",
                        span { class: "inline-block w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" }
                    }
                } else if *error.read() {
                    div { class: "px-4 py-8 text-center text-sm text-muted-foreground",
                        "Trending API currently unavailable"
                    }
                } else if trending_notes.read().is_empty() {
                    div { class: "px-4 py-8 text-center text-sm text-muted-foreground",
                        "No trending posts right now"
                    }
                } else {
                    for note in trending_notes.read().iter() {
                        TrendingNoteItem { key: "{note.event.id}", note: note.clone() }
                    }
                }
            }
            if !*loading.read() && !*error.read() && !trending_notes.read().is_empty() {
                div { class: "border-t border-border shrink-0",
                    Link {
                        to: Route::Trending { source: None },
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
    let profile = use_memo(move || profiles::get_profile(&author_pubkey_for_profile));
    let note_id = &note.event.id;
    let note_bech32 = match nostr_sdk::EventId::from_hex(note_id) {
        Ok(id) => {
            use nostr_sdk::ToBech32;
            id.to_bech32().unwrap_or_else(|_| note_id.clone())
        }
        Err(_) => note_id.clone(),
    };
    let author_name = {
        let p = profile.read();
        if let Some(ref prof) = *p {
            prof.display_name
                .clone()
                .or_else(|| prof.name.clone())
                .unwrap_or_else(|| truncate_pubkey(&author_pubkey))
        } else {
            truncate_pubkey(&author_pubkey)
        }
    };
    let picture = {
        let p = profile.read();
        p.as_ref()
            .and_then(|prof| prof.picture.clone())
            .unwrap_or_else(|| {
                format!(
                    "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                    author_pubkey,
                )
            })
    };
    let content_warning: Option<Option<String>> = note.event.tags.iter().find(|t| t.first().map(|s| s.as_str()) == Some("content-warning")).map(|t| t.get(1).cloned());
    let content = truncate_content(&note.event.content, 100);
    rsx! {
        Link {
            to: Route::AddressViewer {
                address: crate::utils::nip19_urls::note_route_id(&note_bech32, Some(&author_pubkey)),
            },
            class: "block px-4 py-3 hover:bg-accent/50 transition-colors border-b border-border last:border-0",
            div { class: "flex gap-3",
                img {
                    src: "{picture}",
                    alt: "{author_name}",
                    class: "w-10 h-10 rounded-full shrink-0 object-cover",
                    loading: "lazy",
                }
                div { class: "flex-1 min-w-0",
                    div { class: "text-sm font-semibold truncate mb-1", "{author_name}" }
                    {
                        let content_el = rsx! {
                            div { class: "text-sm mb-2 line-clamp-2", "{content}" }
                        };
                        if let Some(reason) = content_warning {
                            rsx! { SensitiveContent { reason, {content_el} } }
                        } else {
                            content_el
                        }
                    }
                    if let Some(stats) = &note.stats {
                        div { class: "flex items-center gap-3 text-xs text-muted-foreground",
                            if let Some(reactions) = stats.reactions {
                                if reactions > 0 {
                                    span { class: "flex items-center gap-1", "❤️ {reactions}" }
                                }
                            }
                            if let Some(replies) = stats.replies {
                                if replies > 0 {
                                    span { class: "flex items-center gap-1", "💬 {replies}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
