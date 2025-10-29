use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::components::NoteCard;
use crate::routes::Route;
use nostr_sdk::{Event as NostrEvent, Filter, Kind, Metadata, FromBech32};
use nostr::PublicKey;
use std::time::Duration;

#[derive(Clone, PartialEq)]
enum SearchType {
    All,
    Users,
    Notes,
    Hashtags,
}

#[component]
pub fn Search() -> Element {
    let mut search_query = use_signal(|| String::new());
    let mut search_type = use_signal(|| SearchType::All);
    let mut user_results = use_signal(|| Vec::<(PublicKey, Metadata)>::new());
    let mut note_results = use_signal(|| Vec::<NostrEvent>::new());
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut has_searched = use_signal(|| false);

    let perform_search = move |_| {
        let query = search_query.read().trim().to_string();

        if query.is_empty() {
            error.set(Some("Please enter a search term".to_string()));
            return;
        }

        if query.len() < 2 {
            error.set(Some("Search term must be at least 2 characters".to_string()));
            return;
        }

        has_searched.set(true);
        loading.set(true);
        error.set(None);
        user_results.set(Vec::new());
        note_results.set(Vec::new());

        let search_type_val = (*search_type.read()).clone();

        spawn(async move {
            match search_type_val {
                SearchType::Users => {
                    if let Err(e) = search_users(query.clone()).await {
                        error.set(Some(e));
                    }
                }
                SearchType::Notes => {
                    match search_notes(query.clone()).await {
                        Ok(notes) => {
                            note_results.set(notes);
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                }
                SearchType::Hashtags => {
                    match search_hashtags(query.clone()).await {
                        Ok(notes) => {
                            note_results.set(notes);
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                }
                SearchType::All => {
                    // Search everything
                    let _ = search_users(query.clone()).await;
                    match search_notes(query.clone()).await {
                        Ok(notes) => {
                            note_results.set(notes);
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                }
            }
            loading.set(false);
        });
    };

    rsx! {
        div {
            class: "min-h-screen",

            // Header with search input
            div {
                class: "sticky top-0 z-20 bg-background border-b border-border",
                div {
                    class: "p-4 space-y-4",

                    // Search input
                    div {
                        class: "flex gap-2",
                        input {
                            class: "flex-1 px-4 py-3 border border-input rounded-lg bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring",
                            r#type: "text",
                            placeholder: "Search for users, notes, or hashtags...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value())
                        }
                        button {
                            class: "px-6 py-3 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition disabled:opacity-50",
                            disabled: *loading.read(),
                            onclick: perform_search,
                            if *loading.read() {
                                "Searching..."
                            } else {
                                "🔍 Search"
                            }
                        }
                    }

                    // Filter tabs
                    div {
                        class: "flex gap-2 overflow-x-auto pb-1",
                        SearchTab {
                            label: "All",
                            is_active: matches!(*search_type.read(), SearchType::All),
                            on_click: move |_| search_type.set(SearchType::All)
                        }
                        SearchTab {
                            label: "👤 Users",
                            is_active: matches!(*search_type.read(), SearchType::Users),
                            on_click: move |_| search_type.set(SearchType::Users)
                        }
                        SearchTab {
                            label: "📝 Notes",
                            is_active: matches!(*search_type.read(), SearchType::Notes),
                            on_click: move |_| search_type.set(SearchType::Notes)
                        }
                        SearchTab {
                            label: "#️⃣ Hashtags",
                            is_active: matches!(*search_type.read(), SearchType::Hashtags),
                            on_click: move |_| search_type.set(SearchType::Hashtags)
                        }
                    }
                }
            }

            // Error message
            if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-4",
                    div {
                        class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "❌ {err}"
                    }
                }
            }

            // Loading state
            if *loading.read() {
                div {
                    class: "flex items-center justify-center p-12",
                    div {
                        class: "text-center",
                        div {
                            class: "animate-spin text-4xl mb-3",
                            "🔍"
                        }
                        p {
                            class: "text-muted-foreground",
                            "Searching relays..."
                        }
                    }
                }
            }

            // Results
            if !*loading.read() && *has_searched.read() {
                div {
                    class: "p-4 space-y-6",

                    // User results
                    if matches!(*search_type.read(), SearchType::All | SearchType::Users) && !user_results.read().is_empty() {
                        div {
                            class: "space-y-3",
                            h3 {
                                class: "text-lg font-bold mb-3",
                                "👤 Users ({user_results.read().len()})"
                            }
                            for (pubkey, metadata) in user_results.read().iter() {
                                UserSearchResult {
                                    pubkey: pubkey.clone(),
                                    metadata: metadata.clone()
                                }
                            }
                        }
                    }

                    // Note results
                    if matches!(*search_type.read(), SearchType::All | SearchType::Notes | SearchType::Hashtags) && !note_results.read().is_empty() {
                        div {
                            class: "space-y-3",
                            h3 {
                                class: "text-lg font-bold mb-3",
                                "📝 Notes ({note_results.read().len()})"
                            }
                            for note in note_results.read().iter() {
                                NoteCard {
                                    event: note.clone()
                                }
                            }
                        }
                    }

                    // Empty state
                    if user_results.read().is_empty() && note_results.read().is_empty() {
                        div {
                            class: "text-center py-12",
                            div {
                                class: "text-6xl mb-4",
                                "🔍"
                            }
                            h3 {
                                class: "text-xl font-semibold mb-2",
                                "No results found"
                            }
                            p {
                                class: "text-muted-foreground",
                                "Try a different search term or filter"
                            }
                        }
                    }
                }
            } else if !*has_searched.read() {
                // Initial state
                div {
                    class: "text-center py-12",
                    div {
                        class: "text-6xl mb-4",
                        "🔎"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2",
                        "Search Nostr"
                    }
                    p {
                        class: "text-muted-foreground max-w-md mx-auto",
                        "Find users, notes, and hashtags across the Nostr network. Enter your search term above to get started."
                    }
                }
            }
        }
    }
}

#[component]
fn SearchTab(label: &'static str, is_active: bool, on_click: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: if is_active {
                "px-4 py-2 bg-blue-600 text-white rounded-full font-medium whitespace-nowrap"
            } else {
                "px-4 py-2 bg-muted hover:bg-accent text-foreground rounded-full font-medium whitespace-nowrap transition"
            },
            onclick: move |_| on_click.call(()),
            "{label}"
        }
    }
}

#[component]
fn UserSearchResult(pubkey: PublicKey, metadata: Metadata) -> Element {
    let pubkey_str = pubkey.to_hex();

    rsx! {
        Link {
            to: Route::Profile { pubkey: pubkey_str.clone() },
            class: "block p-4 bg-card hover:bg-accent rounded-lg border border-border transition",
            div {
                class: "flex items-center gap-3",

                // Avatar
                if let Some(picture) = &metadata.picture {
                    img {
                        class: "w-12 h-12 rounded-full",
                        src: "{picture}",
                        alt: "Profile picture"
                    }
                } else {
                    div {
                        class: "w-12 h-12 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white font-bold text-lg",
                        if let Some(name) = &metadata.name {
                            "{name.chars().next().unwrap_or('?').to_uppercase()}"
                        } else {
                            "?"
                        }
                    }
                }

                // User info
                div {
                    class: "flex-1 min-w-0",
                    div {
                        class: "flex items-center gap-2",
                        if let Some(name) = &metadata.name {
                            h4 {
                                class: "font-bold text-foreground truncate",
                                "{name}"
                            }
                        }
                        if let Some(_nip05) = &metadata.nip05 {
                            span {
                                class: "text-xs text-blue-600 dark:text-blue-400",
                                "✓"
                            }
                        }
                    }
                    if let Some(display_name) = &metadata.display_name {
                        p {
                            class: "text-sm text-muted-foreground truncate",
                            "@{display_name}"
                        }
                    }
                    if let Some(about) = &metadata.about {
                        p {
                            class: "text-sm text-muted-foreground mt-1 line-clamp-2",
                            "{about}"
                        }
                    }
                }
            }
        }
    }
}

// Search functions
async fn search_users(query: String) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    log::info!("Searching for users: {}", query);

    // Try to parse as npub first
    if query.starts_with("npub") {
        if let Ok(pubkey) = PublicKey::from_bech32(&query) {
            // Fetch this specific user's metadata
            let filter = Filter::new()
                .author(pubkey)
                .kind(Kind::Metadata)
                .limit(1);

            match client.fetch_events(filter, Duration::from_secs(10)).await {
                Ok(events) => {
                    if let Some(event) = events.into_iter().next() {
                        if let Ok(_metadata) = serde_json::from_str::<Metadata>(&event.content) {
                            // TODO: Add to user_results
                            log::info!("Found user by npub");
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch user: {}", e);
                }
            }
        }
    }

    // Search by name/display_name in metadata
    // Note: This requires fetching recent metadata events and filtering client-side
    // since Nostr doesn't support content-based search natively
    let filter = Filter::new()
        .kind(Kind::Metadata)
        .limit(100);

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let query_lower = query.to_lowercase();
            let mut found_users = Vec::new();

            for event in events {
                if let Ok(metadata) = serde_json::from_str::<Metadata>(&event.content) {
                    let matches = metadata.name.as_ref().map(|n| n.to_lowercase().contains(&query_lower)).unwrap_or(false)
                        || metadata.display_name.as_ref().map(|n| n.to_lowercase().contains(&query_lower)).unwrap_or(false)
                        || metadata.nip05.as_ref().map(|n| n.to_lowercase().contains(&query_lower)).unwrap_or(false);

                    if matches {
                        found_users.push((event.pubkey, metadata));
                    }
                }
            }

            log::info!("Found {} users matching '{}'", found_users.len(), query);
            // TODO: Set user_results
        }
        Err(e) => {
            return Err(format!("Failed to search users: {}", e));
        }
    }

    Ok(())
}

async fn search_notes(query: String) -> Result<Vec<NostrEvent>, String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    log::info!("Searching for notes: {}", query);

    // Fetch recent notes and filter client-side
    // Note: Nostr relays don't support content search, so we fetch recent and filter
    let filter = Filter::new()
        .kind(Kind::TextNote)
        .limit(200);

    match client.fetch_events(filter, Duration::from_secs(15)).await {
        Ok(events) => {
            let query_lower = query.to_lowercase();
            let matching_notes: Vec<NostrEvent> = events
                .into_iter()
                .filter(|event| event.content.to_lowercase().contains(&query_lower))
                .collect();

            log::info!("Found {} notes matching '{}'", matching_notes.len(), query);
            Ok(matching_notes)
        }
        Err(e) => {
            Err(format!("Failed to search notes: {}", e))
        }
    }
}

async fn search_hashtags(query: String) -> Result<Vec<NostrEvent>, String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Remove # if present
    let tag = query.trim_start_matches('#');

    log::info!("Searching for hashtag: #{}", tag);

    // Search for notes with this hashtag
    let filter = Filter::new()
        .kind(Kind::TextNote)
        .hashtag(tag)
        .limit(100);

    match client.fetch_events(filter, Duration::from_secs(15)).await {
        Ok(events) => {
            let notes: Vec<NostrEvent> = events.into_iter().collect();
            log::info!("Found {} notes with #{}", notes.len(), tag);
            Ok(notes)
        }
        Err(e) => {
            Err(format!("Failed to search hashtag: {}", e))
        }
    }
}
