use crate::components::icons;
use crate::components::{RadioCard, RadioCardSkeleton};
use crate::hooks::use_infinite_scroll_with_generation;
use crate::routes::Route;
use crate::stores::audio::radio_cache;
use crate::stores::{auth_store, nostr_client};
use crate::utils::radio::{
    fetch_radio_stations, fetch_stations_by_coordinates, search_radio_stations,
    RadioStation as RadioStationData,
};
use dioxus::prelude::*;
use nostr::types::time::Timestamp;
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
/// Consecutive load_more failures before the sentinel is retired. One
/// transient relay timeout must leave pagination retryable; a genuinely
/// dead query must not re-fetch on every sentinel intersection forever.
const LOAD_MORE_MAX_CONSECUTIVE_FAILURES: u32 = 3;
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
/// Resolve one favorite coordinate from the local SDK database only
/// (instant — a favorited station was necessarily rendered before, so the
/// local copy almost always exists). Newest version wins (addressable
/// semantics).
async fn resolve_favorite_from_db(pubkey: &str, d_tag: &str) -> Option<RadioStationData> {
    let client = nostr_client::get_client()?;
    let pk = nostr::key::PublicKey::from_hex(pubkey).ok()?;
    let filter = nostr::filter::Filter::new()
        .kind(nostr::event::Kind::Custom(
            crate::utils::radio::KIND_RADIO_STATION,
        ))
        .author(pk)
        .identifier(d_tag.to_string())
        .limit(5);
    let db_events = client.database().query(filter).await.ok()?;
    let event = db_events.into_iter().max_by_key(|e| e.created_at)?;
    crate::utils::radio::RadioStation::from_event(&event).ok()
}
/// Resolve the favorites coordinate set to full stations: DB-first (each
/// coordinate is an instant local lookup), then ONE batched relay REQ for
/// whatever the DB misses — replacing the old per-coordinate network
/// ladders (up to 3×10s + sleeps each) with a single round trip.
/// Unresolvable coordinates are skipped so one dead station never blanks
/// the view.
async fn resolve_favorites(favorites: &std::collections::HashSet<String>) -> Vec<RadioStationData> {
    let mut by_coord: std::collections::HashMap<String, RadioStationData> =
        std::collections::HashMap::new();
    let mut missing: Vec<(String, String)> = Vec::new();
    for coordinate in favorites {
        let Some((pubkey, d_tag)) = parse_favorite_coordinate(coordinate) else {
            continue;
        };
        match resolve_favorite_from_db(&pubkey, &d_tag).await {
            Some(station) => {
                by_coord.insert(station.coordinate.clone(), station);
            }
            None => missing.push((pubkey, d_tag)),
        }
    }
    if !missing.is_empty() {
        match fetch_stations_by_coordinates(&missing, std::time::Duration::from_secs(8)).await {
            Ok(found) => {
                for station in found {
                    by_coord.insert(station.coordinate.clone(), station);
                }
            }
            Err(e) => log::warn!("Favorites batch resolve failed: {e}"),
        }
    }
    let mut resolved: Vec<RadioStationData> = by_coord.into_values().collect();
    resolved.sort_by_key(|s| std::cmp::Reverse(s.created_at));
    resolved
}
/// Apply a completed page to the list signals: `has_more` policy, the
/// pagination cursor, and the station list itself. Callers additionally
/// store the page in the session cache.
fn apply_station_page(
    fetched: Vec<RadioStationData>,
    is_favorites: bool,
    stations: &mut Signal<Vec<RadioStationData>>,
    oldest_timestamp: &mut Signal<Option<u64>>,
    has_more: &mut Signal<bool>,
) {
    if is_favorites {
        // Favorites is a single resolved page: there is no pagination
        // behind it. Force has_more false or the sentinel keeps firing
        // load_more, which early-returns for FAVORITES_GENRE — a
        // background no-op loop.
        has_more.set(false);
    } else {
        has_more.set(fetched.len() >= PAGE_SIZE);
    }
    let mut sorted = fetched.clone();
    sorted.sort_by_key(|s| std::cmp::Reverse(s.created_at));
    sorted.truncate(PAGE_SIZE);
    oldest_timestamp.set(
        sorted
            .iter()
            .map(|s| s.created_at)
            .min()
            .map(|t| t.saturating_sub(1)),
    );
    stations.set(fetched);
}
/// Parameters for one asynchronous /radio list load.
struct RadioFetchTask {
    /// Generation guard (invalidates on effect re-run).
    gen: u32,
    /// Stale-while-revalidate refresh behind an already-painted cached
    /// page: no skeletons, and failures keep the cached paint.
    background: bool,
    in_search_mode: bool,
    in_favorites_mode: bool,
    genre: String,
    query: String,
    cache_key: String,
}
/// The shared fetch task used by both the cache-miss (skeleton) path and
/// the stale-while-revalidate background refresh.
async fn run_radio_fetch(
    task: RadioFetchTask,
    mut stations: Signal<Vec<RadioStationData>>,
    mut is_loading: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut oldest_timestamp: Signal<Option<u64>>,
    mut has_more: Signal<bool>,
    fetch_gen: Signal<u32>,
) {
    let RadioFetchTask {
        gen,
        background,
        in_search_mode,
        in_favorites_mode,
        genre,
        query,
        cache_key,
    } = task;

    // DB-first instant paint (list modes, foreground only): station events
    // persist in the SDK database, so a warm app paints the newest page
    // immediately while the network fetch runs; the network result below
    // replaces it when it lands.
    if !background && !in_search_mode && !in_favorites_mode {
        let genre_filter = if genre.as_str() == "all" {
            None
        } else {
            Some(genre.clone())
        };
        if let Some(db_stations) =
            crate::utils::radio::fetch_radio_stations_from_db(genre_filter.as_deref(), PAGE_SIZE)
                .await
        {
            if *fetch_gen.peek() == gen {
                apply_station_page(
                    db_stations,
                    false,
                    &mut stations,
                    &mut oldest_timestamp,
                    &mut has_more,
                );
                is_loading.set(false);
            }
        }
    }

    // The favorites coordinate set the resolved page is based on (cache
    // snapshot — keeps the entry consistent with what was rendered).
    let mut favorites_snapshot: Option<std::collections::HashSet<String>> = None;
    let result = if in_search_mode {
        search_radio_stations(&query, PAGE_SIZE, None).await
    } else if in_favorites_mode {
        // `load()` coalesces concurrent callers, retries stale-client
        // races, and only fails when a fetch genuinely errored — "no
        // favorites yet" resolves as an empty Ok.
        match crate::stores::audio::radio_favorites::load().await {
            Err(err) => Err(err),
            Ok(()) => {
                let favorites = crate::stores::audio::radio_favorites::RADIO_FAVORITES
                    .read()
                    .favorites
                    .clone();
                favorites_snapshot = Some(favorites.clone());
                Ok(resolve_favorites(&favorites).await)
            }
        }
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
            if background && fetched_stations.is_empty() {
                // A background refresh must not wipe a cached page on an
                // empty result (relay flake): keep the paint as-is.
                log::debug!("Radio background refresh returned empty; keeping cached list");
                is_loading.set(false);
                return;
            }
            apply_station_page(
                fetched_stations.clone(),
                in_favorites_mode,
                &mut stations,
                &mut oldest_timestamp,
                &mut has_more,
            );
            radio_cache::store(&cache_key, fetched_stations, favorites_snapshot);
        }
        Err(e) => {
            if background {
                // Keep the cached paint; the next effect run retries.
                log::warn!("Radio background refresh failed (keeping cached list): {e}");
                is_loading.set(false);
                return;
            }
            let message = if in_favorites_mode {
                log::warn!("Favorites load failed: {e}");
                "Couldn't load your favorites from relays — try again".to_string()
            } else {
                log::error!("Failed to fetch radio stations: {e}");
                e
            };
            error.set(Some(message));
            // The sentinel rests: there is no pagination to attempt
            // behind a failed load.
            has_more.set(false);
        }
    }
    is_loading.set(false);
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
    // Consecutive failed load_more fetches. A single transient relay
    // timeout must not permanently retire the sentinel (the gen-guard
    // already makes retries race-safe) — retire only after this many
    // consecutive failures, reset by any success or a new list session.
    let mut load_more_failures = use_signal(|| 0u32);
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
        // Re-run when the client is rebuilt (login/logout swap): reading
        // NOSTR_CLIENT subscribes this effect to the swap, so the page
        // re-fetches on the new client instead of showing an error from a
        // fetch that raced the old (shutdown) pool — the "broken down
        // filters are empty" failure. Mirrors routes/music/artist.rs.
        if nostr_client::NOSTR_CLIENT.read().is_none() {
            return;
        }
        let gen = *fetch_gen.peek() + 1;
        fetch_gen.set(gen);
        let in_search_mode = !query.is_empty();
        let in_favorites_mode = !in_search_mode && genre.as_str() == FAVORITES_GENRE;
        // Reactive signer read: a parked favorites load (below) resumes when
        // session restore attaches the signer — reading `HAS_SIGNER` here
        // subscribes this effect to the flip. The cached pubkey behind
        // `is_logged_in` is NOT a readiness signal.
        let favorites_signer_ready = in_favorites_mode && *nostr_client::HAS_SIGNER.read();
        // The skeleton branch unmounts the sentinel: re-attach the observer to
        // the node that mounts with the fresh results.
        feed_reset_generation += 1;
        error.set(None);
        loading_more.set(false);
        load_more_failures.set(0);

        if in_favorites_mode && !favorites_signer_ready {
            // Session restore hasn't attached the signer yet (pubkey is
            // cached at boot, but that is not readiness). Park on the
            // skeleton state — the reactive `HAS_SIGNER` read above
            // re-runs this effect once login lands, and the post-login
            // init's favorites load is already running detached.
            is_loading.set(true);
            oldest_timestamp.set(None);
            // The sentinel rests: nothing to paginate behind a park.
            has_more.set(false);
            return;
        }

        // Session cache (stale-while-revalidate): paint the previous
        // result for this tab instantly — no skeletons, no
        // refetch-from-zero on every genre/favorites switch. All cache
        // reads are PEEKs: a tracked read would make the fetch-completion
        // cache write re-run this effect (write → effect → refetch loop).
        let cache_key = if in_search_mode {
            radio_cache::search_key(&query)
        } else if in_favorites_mode {
            radio_cache::FAVORITES_KEY.to_string()
        } else {
            radio_cache::genre_key(&genre)
        };
        let current_favorites = if in_favorites_mode {
            Some(crate::stores::audio::radio_favorites::favorites_snapshot())
        } else {
            None
        };
        if let Some(entry) = radio_cache::peek(&cache_key) {
            let usable = entry.usable_without_refresh(current_favorites.as_ref());
            apply_station_page(
                entry.stations.clone(),
                in_favorites_mode,
                &mut stations,
                &mut oldest_timestamp,
                &mut has_more,
            );
            is_loading.set(false);
            if usable {
                return;
            }
            // Stale (TTL expired or the favorites set changed): refresh in
            // the background WITHOUT flashing skeletons over the paint.
            let task = RadioFetchTask {
                gen,
                background: true,
                in_search_mode,
                in_favorites_mode,
                genre,
                query,
                cache_key,
            };
            spawn(async move {
                run_radio_fetch(
                    task,
                    stations,
                    is_loading,
                    error,
                    oldest_timestamp,
                    has_more,
                    fetch_gen,
                )
                .await;
            });
            return;
        }

        // Cache miss: skeletons while the task paints DB-first (list
        // modes) and then fetches from relays.
        is_loading.set(true);
        oldest_timestamp.set(None);
        has_more.set(true);
        let task = RadioFetchTask {
            gen,
            background: false,
            in_search_mode,
            in_favorites_mode,
            genre,
            query,
            cache_key,
        };
        spawn(async move {
            run_radio_fetch(
                task,
                stations,
                is_loading,
                error,
                oldest_timestamp,
                has_more,
                fetch_gen,
            )
            .await;
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
                        // Keep the session cache in step with pagination so
                        // the next tab switch paints the full list.
                        let cache_key = if in_search_mode {
                            radio_cache::search_key(&query)
                        } else {
                            radio_cache::genre_key(&genre)
                        };
                        radio_cache::append_page(&cache_key, &new_stations);
                    }
                    loading_more.set(false);
                    // A completed page (even an empty one) resets the
                    // failure streak — apply_station_page above decides
                    // has_more for empty/short pages.
                    load_more_failures.set(0);
                }
                Err(e) => {
                    log::error!("Failed to load more radio stations: {}", e);
                    loading_more.set(false);
                    // Transient failure: keep the sentinel armed so the
                    // next intersection retries — a single timeout used to
                    // retire pagination for the genre/search until the tab
                    // switched (and the 2-min session cache kept serving
                    // the truncated list as fresh). The gen-guard above
                    // already makes the retry race-safe. Retire only after
                    // a streak of consecutive failures.
                    let failures = *load_more_failures.read() + 1;
                    load_more_failures.set(failures);
                    if failures >= LOAD_MORE_MAX_CONSECUTIVE_FAILURES {
                        has_more.set(false);
                    }
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
