use crate::components::places::amenity_filter::AmenityFilter;
use crate::components::places::location_prompt::{LocationResult, PlacesLocationSearch};
use crate::components::places::place_card::PlaceCard;
use crate::routes::Route;
use crate::services::geocoding;
use crate::services::places;
use crate::stores::nostr_client::CLIENT_INITIALIZED;
use crate::stores::places_store;
use dioxus::prelude::*;

const PLACES_LOCATION_KEY: &str = "nostr_blue_places_location";
const PLACES_CITY_KEY: &str = "nostr_blue_places_city";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CachedLocation {
    lat: f64,
    lon: f64,
    display_name: String,
    city: Option<String>,
    state: Option<String>,
}

fn load_cached_location() -> Option<CachedLocation> {
    crate::platform::storage::get(PLACES_LOCATION_KEY).ok()
}

fn save_cached_location(loc: &CachedLocation) {
    let _ = crate::platform::storage::set(PLACES_LOCATION_KEY, loc);
    if let Some(city) = &loc.city {
        let _ = crate::platform::storage::set(PLACES_CITY_KEY, city);
    }
}

#[component]
pub fn PlacesHome() -> Element {
    if !*CLIENT_INITIALIZED.read() {
        return rsx! {
            div { class: "flex items-center justify-center py-16",
                span { class: "inline-block h-8 w-8 rounded-full border-4 border-muted-foreground/30 border-t-muted-foreground animate-spin" }
            }
        };
    }

    let mut user_location = use_signal(|| None::<(f64, f64)>);
    let mut city_name = use_signal(|| None::<String>);
    let mut selected_amenity = use_signal(|| None::<String>);
    let mut loading_places = use_signal(|| false);
    let mut places_loaded = use_signal(|| false);
    let mut fetched_geohashes = use_signal(std::collections::HashSet::<String>::new);
    let mut show_location_search = use_signal(|| false);

    use_hook(|| {
        if let Some(cached) = load_cached_location() {
            user_location.set(Some((cached.lat, cached.lon)));
            city_name.set(Some(cached.display_name));
        } else if let Some((lat, lon)) = *places_store::USER_LOCATION.read() {
            user_location.set(Some((lat, lon)));
            let lat_c = lat;
            let lon_c = lon;
            spawn(async move {
                if let Ok(Some(loc)) = geocoding::reverse_geocode_city(lat_c, lon_c).await {
                    city_name.set(Some(loc.display_name));
                }
            });
        } else {
            show_location_search.set(true);
        }
    });

    let on_location_selected = move |loc: LocationResult| {
        let cached = CachedLocation {
            lat: loc.lat,
            lon: loc.lon,
            display_name: loc.display_name.clone(),
            city: loc.city.clone(),
            state: loc.state.clone(),
        };
        save_cached_location(&cached);
        user_location.set(Some((loc.lat, loc.lon)));
        city_name.set(Some(loc.display_name));
        show_location_search.set(false);
        places_loaded.set(false);
    };

    use_effect(move || {
        let loc = *user_location.read();
        let already = *places_loaded.read();
        if let Some((lat, lon)) = loc {
            if !already {
                loading_places.set(true);
                let mut fetched = fetched_geohashes.write().clone();
                let prefix = places::geohash_prefix(lat, lon, 3);
                let prefixes = vec![prefix.clone()];
                spawn(async move {
                    for p in &prefixes {
                        if !fetched.contains(p) {
                            match places::fetch_places_for_geohash(p).await {
                                Ok(new_places) => {
                                    for pl in new_places {
                                        places_store::merge_place(pl);
                                    }
                                }
                                Err(e) => {
                                    log::warn!("Failed to fetch places: {}", e);
                                }
                            }
                            fetched.insert(p.clone());
                        }
                    }
                    let prefix2 = places::geohash_prefix(lat, lon, 4);
                    if !fetched.contains(&prefix2) {
                        match places::fetch_places_for_geohash(&prefix2).await {
                            Ok(new_places) => {
                                for pl in new_places {
                                    places_store::merge_place(pl);
                                }
                            }
                            Err(e) => {
                                log::warn!("Failed to fetch places (zoom): {}", e);
                            }
                        }
                        fetched.insert(prefix2);
                    }
                    *fetched_geohashes.write() = fetched;
                    loading_places.set(false);
                    places_loaded.set(true);
                });
            }
        }
    });

    let loc = *user_location.read();
    let has_location = loc.is_some();
    let (u_lat, u_lon) = loc.unwrap_or((0.0, 0.0));
    let city = city_name.read().clone();

    let filtered_sorted = use_memo(move || {
        let all_places = places_store::PLACES.read();
        let loc = *user_location.read();
        let filter = selected_amenity.read().clone();

        let Some((lat, lon)) = loc else {
            return vec![];
        };

        let mut results: Vec<_> = all_places
            .iter()
            .filter(|p| !p.deleted)
            .filter(|p| {
                let dist = places::haversine_km(lat, lon, p.coordinates[1], p.coordinates[0]);
                dist <= 80.0
            })
            .filter(|p| {
                match &filter {
                    None => true,
                    Some(a) => p.amenity.as_deref() == Some(a.as_str()),
                }
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| {
            let da = places::haversine_km(lat, lon, a.coordinates[1], a.coordinates[0]);
            let db = places::haversine_km(lat, lon, b.coordinates[1], b.coordinates[0]);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    });

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center justify-between px-4 py-3",
                    h1 { class: "text-xl font-bold", "Places" }
                }

                if let Some(city_display) = &*city_name.read() {
                    div { class: "px-4 pb-2 flex items-center gap-2 text-sm text-muted-foreground",
                        span { class: "text-base", "📍" }
                        span { "{city_display}" }
                        button {
                            class: "text-blue-500 hover:text-blue-400 text-xs",
                            onclick: move |_| show_location_search.set(true),
                            "Change"
                        }
                    }
                }
            }

            if !has_location {
                div { class: "text-center py-20",
                    div { class: "text-6xl mb-4", "📍" }
                    h2 { class: "text-xl font-semibold mb-2", "Find places near you" }
                    p { class: "text-muted-foreground mb-6", "Search for a city or use your current location" }
                    button {
                        class: "px-6 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition",
                        onclick: move |_| show_location_search.set(true),
                        "Search locations"
                    }
                }
            } else {
                div { class: "px-4 py-3 border-b border-border",
                    AmenityFilter {
                        selected: selected_amenity.read().clone(),
                        on_change: move |a| selected_amenity.set(a),
                    }
                }

                div { class: "pb-20",
                    if *loading_places.read() {
                        div { class: "p-4 space-y-4",
                            for _ in 0..5 {
                                div { class: "bg-card border border-border rounded-xl p-4 animate-pulse",
                                    div { class: "flex gap-4",
                                        div { class: "w-20 h-20 bg-muted rounded-lg flex-shrink-0" }
                                        div { class: "flex-1 space-y-2",
                                            div { class: "h-4 bg-muted rounded w-2/3" }
                                            div { class: "h-3 bg-muted rounded w-1/3" }
                                            div { class: "h-3 bg-muted rounded w-1/2" }
                                        }
                                    }
                                }
                            }
                        }
                    } else if filtered_sorted.read().is_empty() {
                        div { class: "flex flex-col items-center justify-center py-16 px-4 text-center",
                            div { class: "text-6xl mb-4", "📍" }
                            h2 { class: "text-lg font-bold mb-2",
                                "No places found"
                            }
                            {
                                let msg = match city.as_ref() {
                                    Some(c) => format!("Be the first to add a place near {}", c),
                                    None => "Be the first to add a place in this area".to_string(),
                                };
                                rsx! {
                                    p { class: "text-muted-foreground text-sm mb-6", "{msg}" }
                                }
                            }
                            Link {
                                to: Route::PlacesMap {},
                                class: "px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-xl font-medium transition",
                                "Add a Place"
                            }
                        }
                    } else {
                        div { class: "divide-y divide-border",
                            for place in filtered_sorted.read().clone() {
                                PlaceCard {
                                    key: "{place.id}",
                                    place,
                                    user_lat: u_lat,
                                    user_lon: u_lon,
                                }
                            }
                        }
                    }
                }
            }

            if has_location {
                Link {
                    to: Route::PlacesMap {},
                    class: "fixed bottom-20 right-4 z-50 w-14 h-14 bg-blue-500 hover:bg-blue-600 text-white rounded-full shadow-lg flex items-center justify-center transition",
                    crate::components::icons::MapPinIcon { class: "w-6 h-6".to_string() }
                }
            }

            if *show_location_search.read() {
                PlacesLocationSearch {
                    on_select: on_location_selected,
                    on_close: move |_| show_location_search.set(false),
                }
            }
        }
    }
}
