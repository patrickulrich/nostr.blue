//! List Detail Page
//!
//! Displays the contents of any NIP-51 list type:
//! - People Lists (30000): Profile cards grid
//! - Relay Lists (30002): Relay URLs
//! - Bookmark Lists (30003): Event cards
//! - Curation Lists (30004): Event cards
use crate::hooks::{use_user_lists, UserList};
use crate::stores::{nostr_client, profiles};
use crate::utils::list_encryption::decrypt_private_tags;
use crate::utils::list_kinds::{NAMED_BOOKMARKS, NAMED_CURATIONS, NAMED_PEOPLE, NAMED_RELAYS};
use dioxus::prelude::*;
use nostr_sdk::{Event, EventId, PublicKey, ToBech32};
/// A member entry extracted from a list
#[derive(Clone, PartialEq)]
struct ListMember {
    pubkey: String,
    display_name: String,
    is_private: bool,
}
/// A relay entry extracted from a list
#[derive(Clone, PartialEq)]
struct RelayEntry {
    url: String,
    read: bool,
    write: bool,
}
#[component]
pub fn ListDetail(identifier: String) -> Element {
    let navigator = use_navigator();
    let (lists, loading, _error, _refresh) = use_user_lists();
    let mut list = use_signal(|| None::<UserList>);
    use_effect({
        let identifier = identifier.clone();
        move || {
            if !*loading.read() {
                let found = lists
                    .read()
                    .iter()
                    .find(|l| l.identifier == identifier)
                    .cloned();
                list.set(found);
            }
        }
    });
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    button {
                        class: "p-2 hover:bg-muted rounded-lg transition",
                        onclick: move |_| navigator.go_back(),
                        "← Back"
                    }
                    if let Some(l) = list.read().as_ref() {
                        h1 { class: "text-xl font-bold flex items-center gap-2",
                            span { "{get_list_icon(l.kind)}" }
                            "{l.name}"
                            if l.has_private_content {
                                span {
                                    class: "text-sm",
                                    title: "Contains encrypted private members",
                                    "🔒"
                                }
                            }
                        }
                    } else {
                        h1 { class: "text-xl font-bold", "List" }
                    }
                }
            }
            div { class: "p-4",
                if *loading.read() {
                    div { class: "flex flex-col items-center justify-center py-20",
                        div { class: "animate-spin text-4xl mb-4", "⏳" }
                        p { class: "text-muted-foreground", "Loading list..." }
                    }
                } else if list.read().is_none() {
                    div { class: "flex flex-col items-center justify-center py-20 text-center",
                        div { class: "text-6xl mb-4", "🔍" }
                        h2 { class: "text-2xl font-bold mb-2", "List not found" }
                        p { class: "text-muted-foreground max-w-sm mb-6",
                            "This list doesn't exist or you don't have access to it."
                        }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            onclick: move |_| {
                                navigator.push("/lists");
                            },
                            "Back to Lists"
                        }
                    }
                } else if let Some(l) = list.read().clone() {
                    match l.kind {
                        k if k == NAMED_PEOPLE => rsx! {
                            PeopleListView { list: l.clone() }
                        },
                        k if k == NAMED_RELAYS => rsx! {
                            RelayListView { list: l.clone() }
                        },
                        k if k == NAMED_BOOKMARKS || k == NAMED_CURATIONS => rsx! {
                            EventListView { list: l.clone() }
                        },
                        _ => rsx! {
                            div { class: "text-center py-10 text-muted-foreground", "Unsupported list type (kind {l.kind})" }
                        },
                    }
                }
            }
        }
    }
}
/// People List View - Shows profile cards for all members
#[component]
fn PeopleListView(list: UserList) -> Element {
    let mut members = use_signal(Vec::<ListMember>::new);
    let mut loading = use_signal(|| true);
    use_effect({
        let list_event = list.event.clone();
        move || {
            let list_event = list_event.clone();
            spawn(async move {
                loading.set(true);
                let mut all_members = Vec::new();
                for tag in list_event.tags.iter() {
                    if tag.kind() == nostr_sdk::TagKind::p() {
                        if let Some(pk_str) = tag.content() {
                            let display_name = get_member_display_name(pk_str);
                            all_members.push(ListMember {
                                pubkey: pk_str.to_string(),
                                display_name,
                                is_private: false,
                            });
                        }
                    }
                }
                match decrypt_private_tags(&list_event).await {
                    Ok(private_tags) => {
                        for tag in private_tags {
                            if tag.kind() == nostr_sdk::TagKind::p() {
                                if let Some(pk_str) = tag.content() {
                                    if !all_members.iter().any(|m| m.pubkey == pk_str) {
                                        let display_name = get_member_display_name(pk_str);
                                        all_members.push(ListMember {
                                            pubkey: pk_str.to_string(),
                                            display_name,
                                            is_private: true,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to decrypt private members: {}", e);
                    }
                }
                members.set(all_members);
                loading.set(false);
            });
        }
    });
    rsx! {
        div {
            if !list.description.is_empty() {
                p { class: "text-muted-foreground mb-4", "{list.description}" }
            }
            p { class: "text-sm text-muted-foreground mb-4", "{members.read().len()} members" }
            if *loading.read() {
                div { class: "flex items-center justify-center py-10",
                    div { class: "animate-spin text-4xl", "⏳" }
                }
            } else if members.read().is_empty() {
                div { class: "text-center py-10 text-muted-foreground",
                    div { class: "text-4xl mb-2", "👻" }
                    "No members in this list"
                }
            } else {
                div { class: "grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3",
                    for member in members.read().iter() {
                        ProfileCard {
                            key: "{member.pubkey}",
                            pubkey: member.pubkey.clone(),
                            display_name: member.display_name.clone(),
                            is_private: member.is_private,
                        }
                    }
                }
            }
        }
    }
}
/// Profile card for a list member
#[component]
fn ProfileCard(pubkey: String, display_name: String, is_private: bool) -> Element {
    let navigator = use_navigator();
    let npub = PublicKey::parse(&pubkey)
        .ok()
        .and_then(|pk| pk.to_bech32().ok())
        .unwrap_or_else(|| pubkey.clone());
    let pubkey_short = if pubkey.len() >= 16 {
        format!("{}...", &pubkey[..16])
    } else {
        pubkey.clone()
    };
    rsx! {
        div {
            class: "border border-border rounded-lg p-4 hover:bg-muted/50 transition cursor-pointer",
            onclick: move |_| {
                navigator.push(format!("/profile/{}", npub.clone()));
            },
            div { class: "flex items-center gap-3",
                div { class: "w-12 h-12 rounded-full bg-muted flex items-center justify-center text-xl",
                    "👤"
                }
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2",
                        span { class: "font-medium truncate", "{display_name}" }
                        if is_private {
                            span {
                                class: "text-xs px-1.5 py-0.5 bg-muted rounded text-muted-foreground",
                                title: "Private member",
                                "🔒"
                            }
                        }
                    }
                    p { class: "text-xs text-muted-foreground truncate", "{pubkey_short}" }
                }
            }
        }
    }
}
/// Relay List View - Shows relay URLs
#[component]
fn RelayListView(list: UserList) -> Element {
    let relays = use_memo({
        let tags = list.tags.clone();
        move || {
            let mut entries = Vec::new();
            for tag in tags.iter() {
                let tag_slice = tag.as_slice();
                if tag_slice.first().map(|s| s.as_str()) == Some("relay") {
                    if let Some(url) = tag_slice.get(1) {
                        let marker = tag_slice.get(2).map(|s| s.as_str());
                        let (read, write) = match marker {
                            Some("read") => (true, false),
                            Some("write") => (false, true),
                            _ => (true, true),
                        };
                        entries.push(RelayEntry {
                            url: url.to_string(),
                            read,
                            write,
                        });
                    }
                }
            }
            entries
        }
    });
    rsx! {
        div {
            if !list.description.is_empty() {
                p { class: "text-muted-foreground mb-4", "{list.description}" }
            }
            p { class: "text-sm text-muted-foreground mb-4", "{relays.read().len()} relays" }
            if relays.read().is_empty() {
                div { class: "text-center py-10 text-muted-foreground",
                    div { class: "text-4xl mb-2", "🔗" }
                    "No relays in this list"
                }
            } else {
                div { class: "space-y-2",
                    for relay in relays.read().iter() {
                        RelayCard {
                            key: "{relay.url}",
                            url: relay.url.clone(),
                            read: relay.read,
                            write: relay.write,
                        }
                    }
                }
            }
        }
    }
}
/// Relay card component
#[component]
fn RelayCard(url: String, read: bool, write: bool) -> Element {
    rsx! {
        div { class: "border border-border rounded-lg p-4 flex items-center gap-3",
            div { class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center text-xl",
                "🔗"
            }
            div { class: "flex-1 min-w-0",
                p { class: "font-mono text-sm truncate", "{url}" }
                div { class: "flex gap-2 mt-1",
                    if read {
                        span { class: "text-xs px-2 py-0.5 bg-green-500/10 text-green-600 rounded",
                            "Read"
                        }
                    }
                    if write {
                        span { class: "text-xs px-2 py-0.5 bg-blue-500/10 text-blue-600 rounded",
                            "Write"
                        }
                    }
                }
            }
        }
    }
}
/// Event List View - Shows bookmarked/curated events
#[component]
fn EventListView(list: UserList) -> Element {
    let mut events = use_signal(Vec::<Event>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    use_effect({
        let list_event = list.event.clone();
        move || {
            let list_event = list_event.clone();
            spawn(async move {
                loading.set(true);
                error_msg.set(None);
                let mut event_ids: Vec<EventId> = Vec::new();
                for tag in list_event.tags.iter() {
                    if tag.kind() == nostr_sdk::TagKind::e() {
                        if let Some(id_str) = tag.content() {
                            if let Ok(id) = EventId::parse(id_str) {
                                event_ids.push(id);
                            }
                        }
                    }
                }
                match decrypt_private_tags(&list_event).await {
                    Ok(private_tags) => {
                        for tag in private_tags {
                            if tag.kind() == nostr_sdk::TagKind::e() {
                                if let Some(id_str) = tag.content() {
                                    if let Ok(id) = EventId::parse(id_str) {
                                        if !event_ids.contains(&id) {
                                            event_ids.push(id);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to decrypt private events: {}", e);
                    }
                }
                if event_ids.is_empty() {
                    events.set(Vec::new());
                    loading.set(false);
                    return;
                }
                if let Some(client) = nostr_client::get_client() {
                    let filter = nostr_sdk::Filter::new().ids(event_ids.clone());
                    match client
                        .fetch_events(filter, std::time::Duration::from_secs(5))
                        .await
                    {
                        Ok(fetched) => {
                            events.set(fetched.into_iter().collect());
                        }
                        Err(e) => {
                            log::error!("Failed to fetch events: {}", e);
                            error_msg.set(Some(format!("Failed to load events: {}", e)));
                        }
                    }
                } else {
                    error_msg.set(Some("Client not initialized".to_string()));
                }
                loading.set(false);
            });
        }
    });
    rsx! {
        div {
            if !list.description.is_empty() {
                p { class: "text-muted-foreground mb-4", "{list.description}" }
            }
            p { class: "text-sm text-muted-foreground mb-4", "{events.read().len()} items" }
            if let Some(err) = error_msg.read().as_ref() {
                div { class: "mb-4 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-600",
                    "{err}"
                }
            }
            if *loading.read() {
                div { class: "flex items-center justify-center py-10",
                    div { class: "animate-spin text-4xl", "⏳" }
                }
            } else if events.read().is_empty() {
                div { class: "text-center py-10 text-muted-foreground",
                    div { class: "text-4xl mb-2", "📑" }
                    "No events in this list"
                }
            } else {
                div { class: "space-y-4",
                    for event in events.read().iter() {
                        EventCard { key: "{event.id}", event: event.clone() }
                    }
                }
            }
        }
    }
}
/// Simple event card for bookmarks/curations
#[component]
fn EventCard(event: Event) -> Element {
    let navigator = use_navigator();
    let route = crate::utils::route_for_kind::route_for_event(&event);
    let author = event.pubkey.to_hex();
    let author_name = get_member_display_name(&author);
    let content_preview = if event.content.len() > 200 {
        format!("{}...", &event.content[..200])
    } else {
        event.content.clone()
    };
    let kind_label = crate::utils::route_for_kind::content_label_for_kind(event.kind.as_u16());
    rsx! {
        div {
            class: "border border-border rounded-lg p-4 hover:bg-muted/50 transition cursor-pointer",
            onclick: move |_| {
                navigator.push(route.clone());
            },
            div { class: "flex items-center gap-2 mb-2",
                span { class: "text-xs px-2 py-0.5 bg-muted rounded text-muted-foreground",
                    "{kind_label}"
                }
                span { class: "text-sm text-muted-foreground", "by {author_name}" }
            }
            p { class: "text-sm line-clamp-3", "{content_preview}" }
        }
    }
}
/// Helper to get display name for a pubkey
fn get_member_display_name(pubkey: &str) -> String {
    profiles::get_profile(pubkey)
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            if pubkey.len() >= 12 {
                format!("{}...", &pubkey[..12])
            } else {
                pubkey.to_string()
            }
        })
}
/// Helper to get list icon based on kind
fn get_list_icon(kind: u16) -> &'static str {
    match kind {
        k if k == NAMED_PEOPLE => "👥",
        k if k == NAMED_RELAYS => "🔗",
        k if k == NAMED_BOOKMARKS => "🔖",
        k if k == NAMED_CURATIONS => "📚",
        _ => "📋",
    }
}
