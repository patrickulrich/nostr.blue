//! NIP-51 named-list viewer (kinds 30000–30004). Playlist-shaped bookmark
//! sets (kind 30003 with `t: playlist` + a-tags to kind 36787 tracks — the
//! Plume pattern) render through the shared playlist body with playback;
//! everything else renders a generic member list (e/a/p tags + relay URLs).
use crate::components::viewers::playlist_viewer::PlaylistBody;
use crate::stores::{nostr_client, profiles};
use dioxus::prelude::*;
use nostr::event::Event;

#[component]
pub fn BookmarkSetViewer(naddr: String) -> Element {
    let mut event = use_signal(|| None::<Event>);
    let mut author_name = use_signal(|| String::from("Unknown"));
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    use_effect(use_reactive!(|naddr| {
        let naddr_clone = naddr.clone();
        loading.set(true);
        error_msg.set(None);
        spawn(async move {
            let parsed = match crate::utils::nip19::parse_naddr(&naddr_clone) {
                Ok(result) => result,
                Err(e) => {
                    error_msg.set(Some(e));
                    loading.set(false);
                    return;
                }
            };
            let fetched = nostr_client::fetch_event_by_coordinate_with_relays(
                parsed.kind,
                parsed.pubkey.clone(),
                parsed.identifier.clone(),
                parsed.relay_hints.clone(),
            )
            .await;
            match fetched {
                Ok(Some(ev)) => {
                    let pk = ev.pubkey.to_hex();
                    if let Ok(profile) = profiles::fetch_profile(pk).await {
                        author_name.set(profile.get_display_name());
                    }
                    event.set(Some(ev));
                }
                Ok(None) => {
                    error_msg.set(Some("List not found".to_string()));
                }
                Err(e) => {
                    error_msg.set(Some(format!("Failed to load list: {}", e)));
                }
            }
            loading.set(false);
        });
    }));

    if *loading.read() {
        return rsx! {
            div { class: "max-w-2xl mx-auto p-4",
                div { class: "animate-pulse space-y-4",
                    div { class: "h-8 bg-muted rounded w-1/2" }
                    div { class: "h-4 bg-muted rounded w-1/3" }
                    div { class: "h-20 bg-muted rounded" }
                }
            }
        };
    }

    if let Some(err) = error_msg.read().clone() {
        return rsx! {
            div { class: "max-w-2xl mx-auto p-4 text-center py-16",
                p { class: "text-muted-foreground", "{err}" }
            }
        };
    }

    let Some(ev) = event.read().clone() else {
        return rsx! {
            div { class: "max-w-2xl mx-auto p-4 text-center py-16",
                p { class: "text-muted-foreground", "List not found" }
            }
        };
    };

    // Playlist-shaped bookmark sets render (and play) like any playlist.
    if let Ok(playlist) = crate::stores::nostr_music::parse_playlist_event(&ev) {
        return rsx! { PlaylistBody { playlist } };
    }

    let kind_name = crate::utils::list_kinds::get_list_type_name(ev.kind.as_u16());
    let title = ev
        .tags
        .iter()
        .find(|t| {
            let s = t.as_slice();
            s.first().map(|k| k == "title" || k == "name") == Some(true)
        })
        .and_then(|t| t.as_slice().get(1).cloned())
        .unwrap_or_else(|| {
            ev.tags
                .iter()
                .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("d"))
                .and_then(|t| t.as_slice().get(1).cloned())
                .unwrap_or_else(|| "Untitled List".to_string())
        });
    let description = ev
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("description"))
        .and_then(|t| t.as_slice().get(1).cloned());
    let has_private_content = !ev.content.is_empty();

    let member_rows: Vec<(String, String, String)> = ev
        .tags
        .iter()
        .filter_map(|t| {
            let s = t.as_slice();
            match s.first().map(|v| v.as_str()) {
                Some("e") => s.get(1).map(|id| {
                    (
                        format!("e:{}", id),
                        "Note".to_string(),
                        id.chars().take(16).collect(),
                    )
                }),
                Some("a") => s.get(1).map(|coord| {
                    let label = match coord.split(':').next() {
                        Some("30023") => "Article",
                        Some("34139") => "Playlist",
                        Some("36787") => "Track",
                        _ => "Event",
                    };
                    (format!("a:{}", coord), label.to_string(), coord.clone())
                }),
                // Key must be unique among siblings: a constant "p"
                // collides for every profile member (Dioxus's keyed-diff
                // uniqueness assert panics in debug builds).
                Some("p") => s.get(1).map(|pk| {
                    (format!("p:{}", pk), "Profile".to_string(), pk.clone())
                }),
                _ => None,
            }
        })
        .collect();
    let relay_rows: Vec<String> = ev
        .tags
        .iter()
        .filter(|t| {
            let s = t.as_slice();
            s.first().map(|v| v.as_str()) == Some("relay")
        })
        .filter_map(|t| t.as_slice().get(1).cloned())
        .collect();

    rsx! {
        div { class: "max-w-2xl mx-auto p-4 space-y-6",
            div { class: "p-6 bg-card rounded-xl border border-border",
                span { class: "text-xs font-medium text-muted-foreground uppercase tracking-wide",
                    "{kind_name} List"
                }
                h1 { class: "text-2xl font-bold mt-1", "{title}" }
                if let Some(desc) = description {
                    p { class: "text-muted-foreground mt-2", "{desc}" }
                }
                div { class: "flex items-center gap-4 mt-4 text-sm text-muted-foreground",
                    span { "by {author_name}" }
                    span { "·" }
                    span { "{member_rows.len()} items" }
                    if has_private_content {
                        span {
                            title: "Contains private members only the author can see",
                            "· 🔒 private members"
                        }
                    }
                }
            }
            if member_rows.is_empty() && relay_rows.is_empty() {
                div { class: "text-center py-12 text-muted-foreground",
                    p { "No public items in this list." }
                }
            } else {
                div { class: "space-y-1",
                    for (key , kind_label , value) in member_rows.iter() {
                        div {
                            key: "{key}",
                            class: "flex items-center gap-3 p-3 border border-border rounded-lg bg-card",
                            span { class: "text-xs px-2 py-1 bg-muted rounded-full text-muted-foreground shrink-0",
                                "{kind_label}"
                            }
                            span { class: "text-sm text-muted-foreground truncate font-mono",
                                "{value}"
                            }
                        }
                    }
                    for url in relay_rows.iter() {
                        div {
                            key: "{url}",
                            class: "flex items-center gap-3 p-3 border border-border rounded-lg bg-card",
                            span { class: "text-xs px-2 py-1 bg-muted rounded-full text-muted-foreground shrink-0",
                                "Relay"
                            }
                            span { class: "text-sm truncate font-mono", "{url}" }
                        }
                    }
                }
            }
        }
    }
}
