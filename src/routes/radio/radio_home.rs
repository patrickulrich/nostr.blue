use crate::components::icons;
use crate::components::{RadioCard, RadioCardSkeleton};
use crate::hooks::use_infinite_scroll_with_generation;
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client};
use crate::utils::radio::{
    fetch_radio_stations, search_radio_stations, RadioStation as RadioStationData,
};
use dioxus::prelude::*;
use futures::StreamExt;
use nostr_sdk::Timestamp;
/// Common radio genres for filtering
const GENRES: &[&str] = &[
    "all",
    "rock",
    "pop",
    "jazz",
    "electronic",
    "classical",
    "hip-hop",
    "country",
    "ambient",
    "metal",
    "folk",
    "indie",
    "blues",
    "reggae",
    "latin",
    "world",
    "news",
    "talk",
];
/// Number of stations fetched per page (initial load + each load_more batch).
const PAGE_SIZE: usize = 50;
/// Pseudo-genre selecting the user's favorites list
const FAVORITES_GENRE: &str = "favorites";
/// Parse a favorite station coordinate (`31237:pubkey:d`) into its parts.
/// Pure; testable without a Dioxus runtime.
fn parse_favorite_coordinate(coordinate: &str) -> Option<(String, String)> {
    let mut parts = coordinate.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("31237"), Some(pubkey), Some(d_tag), None) if !pubkey.is_empty() && !d_tag.is_empty() => {
            Some((pubkey.to_string(), d_tag.to_string()))
        }
        _ => None,
    }
}
/// Resolve a favorite station coordinate to a full station.
///
/// DB-first: the SDK auto-saves every received event, and a favorited
/// station was necessarily rendered first, so the local database copy
/// almost always exists (instant). Newest version wins (addressable
/// semantics). Falls back to the unchanged network loader
/// (`fetch_station_by_naddr`) for DB misses. Failures resolve to `None`
/// (logged) so one dead station never blanks the view.
async fn resolve_favorite(coordinate: &str) -> Option<RadioStationData> {
    let (pubkey, d_tag) = parse_favorite_coordinate(coordinate)?;
    if let Some(client) = nostr_client::get_client() {
        let pk = nostr_sdk::PublicKey::from_hex(&pubkey).ok()?;
        let filter = nostr_sdk::Filter::new()
            .kind(nostr_sdk::Kind::Custom(
                crate::utils::radio::KIND_RADIO_STATION,
            ))
            .author(pk)
            .identifier(d_tag.clone())
            .limit(5);
        if let Ok(db_events) = client.database().query(filter).await {
            if let Some(event) = db_events.into_iter().max_by_key(|e| e.created_at) {
                if let Ok(station) = crate::utils::radio::RadioStation::from_event(&event) {
                    return Some(station);
                }
            }
        }
    }
    // Network fallback via the unchanged baseline loader.
    let naddr = crate::utils::radio::build_station_naddr(&pubkey, &d_tag)?;
    crate::utils::radio::fetch_station_by_naddr(&naddr).await.ok()
}
#[component]
pub fn RadioHome() -> Element {
    let mut selected_genre = use_signal(|| "all".to_string());
    let mut stations = use_signal(Vec::<RadioStationData>::new);
    let mut is_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut search_query = use_signal(String::new);
    let mut search_input = use_signal(String::new);
    let mut is_searching = use_signal(|| false);
    let mut refetch_trigger = use_signal(|| 0u32);
    let mut fetch_gen = use_signal(|| 0u32);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut loading_more = use_signal(|| false);
    let mut feed_reset_generation = use_signal(|| 0u64);
    let is_logged_in = auth_store::get_pubkey().is_some();
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let _ = refetch_trigger.read();
        let genre = selected_genre.read().clone();
        let query = search_query.read().clone();
        if !client_initialized {
            return;
        }
        let gen = *fetch_gen.peek() + 1;
        fetch_gen.set(gen);
        let in_search_mode = !query.is_empty();
        let in_favorites_mode = !in_search_mode && genre.as_str() == FAVORITES_GENRE;
        is_loading.set(true);
        // The skeleton branch unmounts the sentinel: re-attach the observer to
        // the node that mounts with the fresh results.
        feed_reset_generation += 1;
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);
        loading_more.set(false);
        spawn(async move {
            let result = if in_search_mode {
                search_radio_stations(&query, PAGE_SIZE, None).await
            } else if in_favorites_mode {
                crate::stores::audio::radio_favorites::load().await;
                // Distinguish "no favorites" from "relays flaked out": load()
                // keeps loaded = false on failure, and the empty vector below
                // would otherwise render the plain empty state with no retry.
                if is_logged_in
                    && !crate::stores::audio::radio_favorites::RADIO_FAVORITES.read().loaded
                {
                    if *fetch_gen.peek() == gen {
                        error.set(Some(
                            "Couldn't load your favorites from relays — try again".to_string(),
                        ));
                        is_loading.set(false);
                        // The sentinel rests: there is no pagination to attempt
                        // behind a failed load.
                        has_more.set(false);
                    }
                    return;
                }
                let favorites = crate::stores::audio::radio_favorites::RADIO_FAVORITES
                    .read()
                    .favorites
                    .clone();
                // Concurrent resolve (bounded) so a long favorites list fills
                // the page in a few seconds instead of serially.
                let resolved: Vec<RadioStationData> = futures::stream::iter(favorites)
                    .map(|coord| async move { resolve_favorite(&coord).await })
                    .buffer_unordered(4)
                    .filter_map(std::future::ready)
                    .collect()
                    .await;
                let mut seen = std::collections::HashSet::new();
                let mut resolved: Vec<RadioStationData> = resolved
                    .into_iter()
                    .filter(|s| seen.insert(s.coordinate.clone()))
                    .collect();
                resolved.sort_by_key(|s| std::cmp::Reverse(s.created_at));
                Ok(resolved)
            } else {
                let genre_filter = if genre.as_str() == "all" {
                    None
                } else {
                    Some(genre.clone())
                };
                fetch_radio_stations(genre_filter.as_deref(), PAGE_SIZE, None).await
            };
            if *fetch_gen.peek() != gen {
                return;
            }
            match result {
                Ok(fetched_stations) => {
                    if in_favorites_mode {
                        // Favorites is a single resolved page: there is no
                        // pagination behind it. Force has_more false or the
                        // sentinel keeps firing load_more, which early-returns
                        // for FAVORITES_GENRE — a background no-op loop.
                        has_more.set(false);
                    } else if fetched_stations.len() >= PAGE_SIZE {
                        has_more.set(true);
                    } else {
                        has_more.set(false);
                    }
                    let mut sorted = fetched_stations.clone();
                    sorted.sort_by_key(|s| std::cmp::Reverse(s.created_at));
                    sorted.truncate(PAGE_SIZE);
                    oldest_timestamp.set(
                        sorted
                            .iter()
                            .map(|s| s.created_at)
                            .min()
                            .map(|t| t.saturating_sub(1)),
                    );
                    stations.set(fetched_stations);
                }
                Err(e) => {
                    log::error!("Failed to fetch radio stations: {}", e);
                    error.set(Some(e));
                }
            }
            is_loading.set(false);
        });
    });
    let load_more = move || {
        if *loading_more.read() || !*has_more.read() {
            return;
        }
        if selected_genre.read().as_str() == FAVORITES_GENRE {
            // Favorites is a single resolved page; no pagination.
            return;
        }
        let until = match *oldest_timestamp.read() {
            Some(ts) => ts,
            None => return,
        };
        let gen = *fetch_gen.peek();
        let genre = selected_genre.read().clone();
        let query = search_query.read().clone();
        loading_more.set(true);
        spawn(async move {
            let in_search_mode = !query.is_empty();
            let result = if in_search_mode {
                search_radio_stations(&query, PAGE_SIZE, Some(Timestamp::from_secs(until))).await
            } else {
                let genre_filter = if genre.as_str() == "all" {
                    None
                } else {
                    Some(genre.clone())
                };
                fetch_radio_stations(
                    genre_filter.as_deref(),
                    PAGE_SIZE,
                    Some(Timestamp::from_secs(until)),
                )
                .await
            };
            if *fetch_gen.peek() != gen {
                return;
            }
            match result {
                Ok(new_stations) => {
                    let existing: std::collections::HashSet<String> =
                        stations.read().iter().map(|s| s.coordinate.clone()).collect();
                    let unique: Vec<RadioStationData> = new_stations
                        .iter()
                        .filter(|s| !existing.contains(&s.coordinate))
                        .cloned()
                        .collect();
                    if !new_stations.is_empty() {
                        let mut sorted = new_stations.clone();
                        sorted.sort_by_key(|s| std::cmp::Reverse(s.created_at));
                        sorted.truncate(PAGE_SIZE);
                        oldest_timestamp.set(
                            sorted
                                .iter()
                                .map(|s| s.created_at)
                                .min()
                                .map(|t| t.saturating_sub(1)),
                        );
                    }
                    has_more.set(new_stations.len() >= PAGE_SIZE && !unique.is_empty());
                    if !unique.is_empty() {
                        let mut updated = stations.read().clone();
                        updated.extend(unique);
                        stations.set(updated);
                    }
                    loading_more.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more radio stations: {}", e);
                    loading_more.set(false);
                    has_more.set(false);
                }
            }
        });
    };
    let sentinel_id =
        use_infinite_scroll_with_generation(load_more, has_more, loading_more, feed_reset_generation);
    let on_search_submit = move |e: Event<FormData>| {
        e.prevent_default();
        let query = search_input.read().trim().to_string();
        if !query.is_empty() {
            is_searching.set(true);
            search_query.set(query);
        }
    };
    let clear_search = move |_| {
        search_input.set(String::new());
        search_query.set(String::new());
        is_searching.set(false);
    };
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-30 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3",
                    div { class: "flex items-center justify-between",
                        div { class: "flex items-center gap-3",
                            div { class: "w-10 h-10 rounded-full bg-red-500/10 flex items-center justify-center",
                                span {
                                    class: "text-red-500",
                                    dangerous_inner_html: icons::RADIO,
                                }
                            }
                            div {
                                h1 { class: "text-xl font-bold", "Internet Radio" }
                                p { class: "text-xs text-muted-foreground",
                                    "Live streaming stations from the Nostr network"
                                }
                            }
                        }
                        if is_logged_in {
                            Link {
                                to: Route::RadioStationNew { edit_naddr: None },
                                class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                span {
                                    class: "w-4 h-4",
                                    dangerous_inner_html: icons::PLUS,
                                }
                                "Add Station"
                            }
                        }
                    }
                }
                div { class: "px-4 py-2",
                    form { class: "flex gap-2", onsubmit: on_search_submit,
                        div { class: "relative flex-1",
                            input {
                                r#type: "text",
                                placeholder: "Search stations (NIP-50)...",
                                class: "w-full px-4 py-2 pl-10 bg-muted border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                value: "{search_input}",
                                oninput: move |e| search_input.set(e.value()),
                            }
                            span {
                                class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
                                dangerous_inner_html: icons::SEARCH,
                            }
                        }
                        if *is_searching.read() {
                            button {
                                r#type: "button",
                                class: "px-3 py-2 bg-muted hover:bg-muted/80 rounded-lg transition flex items-center gap-1",
                                onclick: clear_search,
                                span {
                                    class: "w-4 h-4",
                                    dangerous_inner_html: icons::X,
                                }
                                "Clear"
                            }
                        }
                    }
                }
                if *is_searching.read() {
                    div { class: "px-4 pb-3",
                        div { class: "flex items-center gap-2 text-sm text-muted-foreground",
                            span { "Searching for: " }
                            span { class: "font-medium text-foreground", "{search_query}" }
                            if !*is_loading.read() {
                                {
                                    let count = stations.read().len();
                                    rsx! {
                                        span { class: "text-xs", "({count} results)" }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    div { class: "px-4 pb-3 overflow-x-auto scrollbar-hide",
                        div { class: "flex gap-2",
                            if is_logged_in {
                                button {
                                    class: if *selected_genre.read() == FAVORITES_GENRE { "px-3 py-1.5 rounded-full text-sm font-medium bg-primary text-primary-foreground whitespace-nowrap transition" } else { "px-3 py-1.5 rounded-full text-sm font-medium bg-muted hover:bg-muted/80 whitespace-nowrap transition" },
                                    onclick: move |_| selected_genre.set(FAVORITES_GENRE.to_string()),
                                    "♥ Favorites"
                                }
                            }
                            for genre in GENRES.iter() {
                                button {
                                    key: "{genre}",
                                    class: if *selected_genre.read() == *genre { "px-3 py-1.5 rounded-full text-sm font-medium bg-primary text-primary-foreground whitespace-nowrap transition" } else { "px-3 py-1.5 rounded-full text-sm font-medium bg-muted hover:bg-muted/80 whitespace-nowrap transition" },
                                    onclick: {
                                        let g = genre.to_string();
                                        move |_| selected_genre.set(g.clone())
                                    },
                                    "{genre}"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "p-4",
                if !*nostr_client::CLIENT_INITIALIZED.read() || *is_loading.read() {
                    div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4",
                        for _ in 0..10 {
                            RadioCardSkeleton {}
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        div { class: "w-16 h-16 rounded-full bg-destructive/10 flex items-center justify-center mb-4",
                            span { class: "text-destructive text-2xl", "!" }
                        }
                        p { class: "text-lg font-medium text-destructive", "Failed to load stations" }
                        p { class: "text-sm text-muted-foreground mt-1", "{err}" }
                        button {
                            class: "mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            onclick: move |_| {
                                refetch_trigger.set(refetch_trigger() + 1);
                            },
                            "Try Again"
                        }
                    }
                } else if stations.read().is_empty() {
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        div { class: "w-16 h-16 rounded-full bg-muted flex items-center justify-center mb-4",
                            span {
                                class: "text-muted-foreground",
                                dangerous_inner_html: icons::RADIO,
                            }
                        }
                        p { class: "text-lg font-medium", "No stations found" }
                        p { class: "text-sm text-muted-foreground mt-1",
                            if *selected_genre.read() == FAVORITES_GENRE {
                                "No favorites yet. Tap the heart on a station to save it here!"
                            } else if *selected_genre.read() == "all" {
                                "Be the first to add a radio station!"
                            } else {
                                "No stations found for this genre. Try another genre or add one!"
                            }
                        }
                        if is_logged_in {
                            Link {
                                to: Route::RadioStationNew { edit_naddr: None },
                                class: "mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                "Add a Station"
                            }
                        }
                    }
                } else {
                    div {
                        div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4",
                            for station in stations.read().iter() {
                                RadioCard {
                                    key: "{station.coordinate}",
                                    station: station.clone(),
                                    compact: true,
                                }
                            }
                        }
                        div {
                            id: "{sentinel_id}",
                            class: "p-8 flex justify-center",
                            if *loading_more.read() {
                                span { class: "animate-pulse text-muted-foreground text-sm",
                                    "Loading more stations..."
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_favorite_coordinate() {
        let pubkey = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        assert_eq!(
            parse_favorite_coordinate(&format!("31237:{pubkey}:my-station")),
            Some((pubkey.to_string(), "my-station".to_string()))
        );
        // wrong kind
        assert_eq!(parse_favorite_coordinate(&format!("30078:{pubkey}:x")), None);
        // too many parts
        assert_eq!(
            parse_favorite_coordinate(&format!("31237:{pubkey}:x:y")),
            None
        );
        // empty pieces
        assert_eq!(parse_favorite_coordinate("31237::x"), None);
        assert_eq!(parse_favorite_coordinate("31237:abc:"), None);
        // garbage
        assert_eq!(parse_favorite_coordinate("naddr1xyz"), None);
    }
}
