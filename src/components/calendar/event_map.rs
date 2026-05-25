use crate::services::geocoding::GeoLocation;
use crate::services::geocoding::{geocode, geohash_to_coords};
use crate::stores::calendar_store::UnifiedEvent;
use crate::utils::validation::validate_css_dimension;
use chrono::{Datelike, Timelike};
use dioxus::prelude::*;
use dioxus_core::use_drop;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

static EVENT_MAP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MarkerData {
    lat: f64,
    lng: f64,
    popup: String,
    event_id: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct GeocodedEvent {
    pub event: UnifiedEvent,
    pub location: GeoLocation,
}

#[derive(Props, Clone, PartialEq)]
pub struct EventMapProps {
    pub events: Vec<UnifiedEvent>,
    #[props(default = "400px".to_string())]
    pub height: String,
}

#[component]
pub fn EventMap(props: EventMapProps) -> Element {
    let container_id = use_signal(|| {
        format!(
            "event-map-{}-{}",
            crate::platform::timestamp::now_millis(),
            EVENT_MAP_COUNTER.fetch_add(1, Ordering::Relaxed),
        )
    });
    #[allow(unused_mut)]
    let mut leaflet_loaded = use_signal(|| false);
    #[allow(unused_variables, unused_mut)]
    let mut leaflet_loading = use_signal(|| false);
    #[allow(unused_mut)]
    let mut leaflet_error = use_signal(|| None::<String>);
    #[allow(unused_mut)]
    let mut map_initialized = use_signal(|| false);
    #[allow(unused_mut)]
    let mut geocoded_events = use_signal(Vec::<GeocodedEvent>::new);
    #[allow(unused_mut)]
    let mut loading_geo = use_signal(|| false);
    #[allow(unused_mut)]
    let mut geocode_error_message = use_signal(|| None::<String>);
    #[allow(unused_mut)]
    let mut unresolved_locations = use_signal(Vec::<String>::new);
    let mut processed_event_ids = use_signal(String::new);
    let mut geocode_cancelled = use_signal(|| false);
    let mut unmounted = use_signal(|| false);
    let mut geocode_gen = use_signal(|| 0u32);
    use_drop(move || {
        geocode_cancelled.set(true);
        unmounted.set(true);
    });
    let events_key = use_memo(use_reactive((&props.events,), |(events,)| {
        let mut hasher = DefaultHasher::new();
        for e in events.iter() {
            e.coordinate().hash(&mut hasher);
            e.naddr().hash(&mut hasher);
            e.title().hash(&mut hasher);
            e.start_timestamp().hash(&mut hasher);
            e.is_all_day().hash(&mut hasher);
            e.is_livestream().hash(&mut hasher);
            e.location().hash(&mut hasher);
            if let Some(geohash) = e.geohash() {
                geohash.hash(&mut hasher);
            }
            for loc in e.locations() {
                loc.hash(&mut hasher);
            }
        }
        hasher.finish().to_string()
    }));
    let events_for_geocode = props.events.clone();
    let events_count = props.events.len();

    use_effect(move || {
        if *leaflet_loaded.read() || *leaflet_loading.read() || leaflet_error.read().is_some() {
            return;
        }
        leaflet_loading.set(true);
        spawn(async move {
            let mut eval = dioxus::document::eval(
                r#"
                return await new Promise((resolve, reject) => {
                    if (window.L) { dioxus.send("ok"); return; }
                    if (window.leafletLoadingPromise) {
                        window.leafletLoadingPromise.then(() => dioxus.send("ok")).catch(e => dioxus.send("error:" + e));
                        return;
                    }
                    window.leafletLoadingPromise = new Promise((res, rej) => {
                        let cssLoaded = false, jsLoaded = false, settled = false;
                        const done = () => { if (!settled && cssLoaded && jsLoaded) { settled = true; res(); } };
                        const fail = (msg) => { if (!settled) { settled = true; window.leafletLoadingPromise = null; rej(msg); } };
                        const link = document.createElement('link');
                        link.rel = 'stylesheet';
                        link.href = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.css';
                        link.onload = () => { cssLoaded = true; done(); };
                        link.onerror = () => fail('Leaflet CSS failed');
                        document.head.appendChild(link);
                        const script = document.createElement('script');
                        script.src = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.js';
                        script.onload = () => { jsLoaded = true; done(); };
                        script.onerror = () => fail('Leaflet JS failed');
                        document.head.appendChild(script);
                    });
                    window.leafletLoadingPromise.then(() => dioxus.send("ok")).catch(e => dioxus.send("error:" + e));
                });
                "#,
            );
            let result: String = eval.recv().await.unwrap_or_default();
            if *unmounted.read() {
                return;
            }
            if let Some(err_text) = result.strip_prefix("error:") {
                log::error!("Failed to load Leaflet: {}", err_text);
                leaflet_error.set(Some(
                    "Failed to load map. Please refresh the page.".to_string(),
                ));
            } else {
                leaflet_loaded.set(true);
            }
            leaflet_loading.set(false);
        });
    });

    use_effect(move || {
        if !*leaflet_loaded.read() || *map_initialized.read() {
            return;
        }
        let id = container_id.read().clone();
        let id_json = serde_json::to_string(&id).unwrap_or_default();
        spawn(async move {
            crate::platform::timer::sleep(std::time::Duration::from_millis(100)).await;
            if *unmounted.read() {
                return;
            }
            let result: String = dioxus::document::eval(&format!(
                r#"
                if (!window.L) {{ return "false"; }}
                const maps = window.leafletMaps || new Map();
                if (maps.has({id_json})) {{ maps.get({id_json}).remove(); }}
                const container = document.getElementById({id_json});
                if (!container) {{ return "false"; }}
                const map = L.map({id_json}).setView([20.0, 0.0], 2);
                L.tileLayer('https://{{s}}.tile.openstreetmap.org/{{z}}/{{x}}/{{y}}.png', {{
                    attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
                    maxZoom: 19
                }}).addTo(map);
                window.leafletMaps = maps;
                maps.set({id_json}, map);
                return "true";
                "#
            ))
            .join()
            .await
            .unwrap_or_default();
            if result == "true" {
                map_initialized.set(true);
                log::info!("Map initialized: {}", id);
            } else {
                log::error!("Failed to initialize map container: {}", id);
                leaflet_error.set(Some(
                    "Failed to initialize map. Please refresh the page.".to_string(),
                ));
            }
        });
    });

    use_effect({
        move || {
            let key = events_key.read().clone();
            if !*map_initialized.read() {
                return;
            }
            if leaflet_error.read().is_some() {
                loading_geo.set(false);
                processed_event_ids.set(key);
                return;
            }
            if key == *processed_event_ids.read() {
                return;
            }
            if events_for_geocode.is_empty() {
                geocode_gen.with_mut(|g| *g = g.wrapping_add(1));
                processed_event_ids.set(key);
                geocoded_events.set(Vec::new());
                geocode_error_message.set(None);
                unresolved_locations.set(Vec::new());
                loading_geo.set(false);
                return;
            }
            let invalidated_running_lookup =
                *loading_geo.peek() && key != *processed_event_ids.peek();
            if invalidated_running_lookup {
                geocoded_events.set(Vec::new());
            }
            if *loading_geo.peek() && !invalidated_running_lookup {
                return;
            }
            processed_event_ids.set(key.clone());
            geocode_gen.with_mut(|g| *g = g.wrapping_add(1));
            let this_gen = *geocode_gen.peek();
            loading_geo.set(true);
            geocode_error_message.set(None);
            unresolved_locations.set(Vec::new());
            let key_to_store = key.clone();
            let events_to_process = events_for_geocode.clone();
            spawn(async move {
                let mut results = Vec::new();
                let mut unresolved = Vec::new();
                let mut geocode_cache =
                    HashMap::<String, Result<Option<GeoLocation>, String>>::new();
                let mut geocode_error_count = 0usize;
                let mut last_geocode_error = None::<String>;
                const BATCH_SIZE: usize = 5;
                const BATCH_DELAY_MS: u32 = 200;
                for (idx, event) in events_to_process.iter().enumerate() {
                    if *geocode_gen.read() != this_gen {
                        log::debug!("Geocoding generation superseded before processing batch");
                        return;
                    }
                    if *geocode_cancelled.read() {
                        log::debug!("Geocoding cancelled, stopping processing");
                        if *geocode_gen.read() == this_gen {
                            loading_geo.set(false);
                        }
                        return;
                    }
                    if idx > 0 && idx % BATCH_SIZE == 0 {
                        crate::platform::timer::sleep_ms(BATCH_DELAY_MS).await;
                        if *geocode_gen.read() != this_gen {
                            log::debug!("Geocoding generation superseded after batch delay");
                            return;
                        }
                    }
                    if let Some(geohash) = event.geohash() {
                        if let Some((lat, lon)) = geohash_to_coords(geohash) {
                            results.push(GeocodedEvent {
                                event: event.clone(),
                                location: GeoLocation {
                                    lat,
                                    lon,
                                    display_name: event.location().unwrap_or("").to_string(),
                                    city: None,
                                    state: None,
                                    country: None,
                                    country_code: None,
                                    place_type: None,
                                },
                            });
                            continue;
                        }
                    }
                    if let Some(location_str) = event.location() {
                        if crate::utils::nips::nip52::is_online_location(location_str) {
                            continue;
                        }
                        let lookup = if let Some(cached) = geocode_cache.get(location_str) {
                            cached.clone()
                        } else {
                            let result = geocode(location_str).await.map_err(|e| e.to_string());
                            geocode_cache.insert(location_str.to_string(), result.clone());
                            result
                        };
                        match lookup {
                            Ok(Some(loc)) => {
                                results.push(GeocodedEvent {
                                    event: event.clone(),
                                    location: loc,
                                });
                            }
                            Ok(None) => {
                                log::debug!(
                                    "Geocoding returned no results for: {}",
                                    location_str
                                );
                                unresolved.push(location_str.to_string());
                            }
                            Err(e) => {
                                log::warn!("Geocoding failed for '{}': {}", location_str, e);
                                geocode_error_count = geocode_error_count.saturating_add(1);
                                last_geocode_error = Some(format!("{} ({})", location_str, e));
                            }
                        }
                    }
                }
                if *geocode_cancelled.read() {
                    log::debug!("Geocoding cancelled, not updating signals");
                    if *geocode_gen.read() == this_gen {
                        loading_geo.set(false);
                    }
                    return;
                }
                if *geocode_gen.read() != this_gen {
                    log::debug!("Geocoding generation stale, discarding results");
                    return;
                }
                geocoded_events.set(results);
                unresolved_locations.set(unresolved.clone());
                let unresolved_count = unresolved.len();
                let unresolved_message = if unresolved_count == 0 {
                    None
                } else {
                    Some(format!(
                        "No geocoding results were found for {} event location{}.",
                        unresolved_count,
                        if unresolved_count == 1 { "" } else { "s" }
                    ))
                };
                geocode_error_message.set(if geocode_error_count == 0 {
                    unresolved_message
                } else if let Some(last_error) = last_geocode_error {
                    Some(match unresolved_message {
                        Some(unresolved_summary) => format!(
                            "{} We also hit {} geocoding error{} while building the map. Last error: {}",
                            unresolved_summary,
                            geocode_error_count,
                            if geocode_error_count == 1 { "" } else { "s" },
                            last_error
                        ),
                        None => format!(
                            "We couldn't geocode {} event location{} while building the map. Last error: {}",
                            geocode_error_count,
                            if geocode_error_count == 1 { "" } else { "s" },
                            last_error
                        ),
                    })
                } else {
                    Some(match unresolved_message {
                        Some(unresolved_summary) => format!(
                            "{} We also hit {} geocoding error{} while building the map.",
                            unresolved_summary,
                            geocode_error_count,
                            if geocode_error_count == 1 { "" } else { "s" }
                        ),
                        None => format!(
                            "We couldn't geocode {} event location{} while building the map.",
                            geocode_error_count,
                            if geocode_error_count == 1 { "" } else { "s" }
                        ),
                    })
                });
                processed_event_ids.set(key_to_store);
                loading_geo.set(false);
            });
        }
    });

    use_effect(move || {
        if !*map_initialized.read() {
            return;
        }
        let events = geocoded_events.read().clone();
        let id = container_id.read().clone();
        let id_json = serde_json::to_string(&id).unwrap_or_default();
        spawn(async move {
            let clear_js = format!(
                r#"
                (() => {{
                    const maps = window.leafletMaps || new Map();
                    const map = maps.get({id_json});
                    if (!map) return;
                    map.eachLayer(layer => {{
                        if (layer instanceof L.Marker) map.removeLayer(layer);
                    }});
                }})()
                "#
            );
            let _ = dioxus::document::eval(&clear_js).await;
            if events.is_empty() {
                return;
            }
            let markers: Vec<MarkerData> = events
                .iter()
                .map(|ge| MarkerData {
                    lat: ge.location.lat,
                    lng: ge.location.lon,
                    popup: format_popup(&ge.event, &ge.location),
                    event_id: ge.event.naddr().to_string(),
                })
                .collect();
            match serde_json::to_string(&markers) {
                Ok(json) => {
                    let json_escaped = json.replace('\\', "\\\\").replace('\'', "\\'");
                    let add_js = format!(
                        r#"
                        (() => {{
                            const maps = window.leafletMaps || new Map();
                            const map = maps.get({id_json});
                            if (!map) return;
                            const markers = JSON.parse('{json_escaped}');
                            if (markers.length === 0) return;
                            const bounds = L.latLngBounds();
                            markers.forEach(m => {{
                                L.marker([m.lat, m.lng]).addTo(map).bindPopup(m.popup);
                                bounds.extend([m.lat, m.lng]);
                            }});
                            map.fitBounds(bounds, {{ padding: [50, 50] }});
                        }})()
                        "#
                    );
                    let _ = dioxus::document::eval(&add_js).await;
                }
                Err(e) => {
                    log::error!(
                        "Failed to serialize {} map markers for container {}: {}",
                        markers.len(),
                        id,
                        e
                    );
                }
            }
        });
    });

    use_drop(move || {
        let id = container_id.read().clone();
        let id_json = serde_json::to_string(&id).unwrap_or_default();
        let _ = dioxus::document::eval(&format!(
            r#"
            (() => {{
                const maps = window.leafletMaps || new Map();
                if (maps.has({id_json})) {{
                    maps.get({id_json}).remove();
                    maps.delete({id_json});
                }}
            }})()
            "#
        ));
    });

    let safe_height = validate_css_dimension(&props.height).unwrap_or("400px");
    let container_style = format!("height: {}; width: 100%;", safe_height);
    let show_full_overlay = leaflet_error.read().is_some()
        || !*leaflet_loaded.read()
        || !*map_initialized.read()
        || *loading_geo.read()
        || (*map_initialized.read()
            && !*loading_geo.read()
            && events_count > 0
            && geocoded_events.read().is_empty()
            && !unresolved_locations.read().is_empty())
        || (*map_initialized.read()
            && !*loading_geo.read()
            && events_count > 0
            && geocoded_events.read().is_empty()
            && unresolved_locations.read().is_empty()
            && geocode_error_message.read().is_some())
        || (*map_initialized.read()
            && !*loading_geo.read()
            && events_count > 0
            && geocoded_events.read().is_empty()
            && unresolved_locations.read().is_empty()
            && geocode_error_message.read().is_none());
    rsx! {
        div { class: "event-map-container relative rounded-lg overflow-hidden border border-border isolate z-0",
            div {
                id: "{container_id}",
                style: "{container_style}",
                class: "bg-muted",
            }
            if let Some(ref err) = *leaflet_error.read() {
                div { class: "absolute inset-0 flex items-center justify-center bg-background/80",
                    div { class: "text-center p-4",
                        svg {
                            class: "w-12 h-12 mx-auto text-red-500 mb-2",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "1.5",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z",
                            }
                        }
                        p { class: "text-red-600 dark:text-red-400", "{err}" }
                    }
                }
            } else if !*leaflet_loaded.read() || !*map_initialized.read() || *loading_geo.read() {
                div { class: "absolute inset-0 flex items-center justify-center bg-background/80",
                    div { class: "flex flex-col items-center gap-2",
                        div { class: "w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        span { class: "text-sm text-muted-foreground",
                            if !*leaflet_loaded.read() {
                                "Loading map..."
                            } else if !*map_initialized.read() {
                                "Initializing map..."
                            } else {
                                "Geocoding events..."
                            }
                        }
                    }
                }
            }
            if *map_initialized.read()
                && !*loading_geo.read()
                && events_count > 0
                && geocoded_events.read().is_empty()
                && !unresolved_locations.read().is_empty()
            {
                div { class: "absolute inset-0 flex items-center justify-center bg-background/80",
                    div { class: "text-center p-4",
                        svg {
                            class: "w-12 h-12 mx-auto text-amber-500 mb-2",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "1.5",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z",
                            }
                        }
                        p { class: "text-sm font-medium text-foreground", "We couldn't map these event locations" }
                        if let Some(message) = geocode_error_message.read().as_ref() {
                            p { class: "mt-1 text-sm text-muted-foreground", "{message}" }
                        }
                    }
                }
            }
            if *map_initialized.read()
                && !*loading_geo.read()
                && events_count > 0
                && geocoded_events.read().is_empty()
                && unresolved_locations.read().is_empty()
                && geocode_error_message.read().is_some()
            {
                div { class: "absolute inset-0 flex items-center justify-center bg-background/80",
                    div { class: "text-center p-4",
                        if let Some(message) = geocode_error_message.read().as_ref() {
                            svg {
                                class: "w-12 h-12 mx-auto text-amber-500 mb-2",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "1.5",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z",
                                }
                            }
                            p { class: "text-sm font-medium text-foreground", "Map data is temporarily incomplete" }
                            p { class: "mt-1 text-sm text-muted-foreground", "{message}" }
                        }
                    }
                }
            }
            if *map_initialized.read()
                && !*loading_geo.read()
                && !geocoded_events.read().is_empty()
                && (geocode_error_message.read().is_some()
                    || !unresolved_locations.read().is_empty())
            {
                div { class: "absolute right-2 top-2 max-w-sm rounded-md border border-amber-500/30 bg-background/95 px-3 py-2 shadow-sm",
                    p { class: "text-xs font-medium text-amber-600 dark:text-amber-400", "Some event locations could not be mapped" }
                    if let Some(message) = geocode_error_message.read().as_ref() {
                        p { class: "mt-1 text-xs text-muted-foreground", "{message}" }
                    }
                }
            }
            if *map_initialized.read()
                && !*loading_geo.read()
                && events_count > 0
                && geocoded_events.read().is_empty()
                && unresolved_locations.read().is_empty()
                && geocode_error_message.read().is_none()
            {
                div { class: "absolute inset-0 flex items-center justify-center bg-background/80",
                    div { class: "text-center p-4",
                        svg {
                            class: "w-12 h-12 mx-auto text-muted-foreground mb-2",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "1.5",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M15 10.5a3 3 0 11-6 0 3 3 0 016 0z",
                            }
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M19.5 10.5c0 7.142-7.5 11.25-7.5 11.25S4.5 17.642 4.5 10.5a7.5 7.5 0 1115 0z",
                            }
                        }
                        p { class: "text-muted-foreground", "No events with physical locations found" }
                    }
                }
            }
            if !show_full_overlay {
                div { class: "absolute bottom-2 left-2 bg-background/90 rounded px-2 py-1 text-xs text-muted-foreground",
                    "{geocoded_events.read().len()} events on map"
                }
            }
        }
    }
}

fn format_popup(event: &UnifiedEvent, location: &GeoLocation) -> String {
    let title = event.title();
    let time = format_popup_time(event);
    let loc = &location.display_name;
    let naddr = event.naddr().to_string();
    let href = format!("/{}", naddr);
    format!(
        r#"<div style="min-width: 200px;">
            <a href="{}" style="font-size: 14px; font-weight: bold; color: #3b82f6; text-decoration: none;">{}</a>
            <div style="color: #666; margin-top: 4px;">{}</div>
            <div style="color: #888; margin-top: 2px; font-size: 12px;">{}</div>
        </div>"#,
        html_escape(&href),
        html_escape(title),
        html_escape(&time),
        html_escape(loc),
    )
}

fn format_popup_time(event: &UnifiedEvent) -> String {
    let ts = event.start_timestamp().clamp(0, 253_402_300_799) as i64;
    if ts == 0 {
        return "Date TBD".to_string();
    }
    let utc: chrono::DateTime<chrono::Utc> = chrono::DateTime::from_timestamp(ts, 0)
        .unwrap_or_default();
    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = utc.format("%m").to_string().parse::<usize>().unwrap_or(1).saturating_sub(1);
    let day = utc.day();
    let month_str = month_names.get(month).unwrap_or(&"");
    if event.is_all_day() {
        format!("{} {}", month_str, day)
    } else {
        let hours = utc.hour();
        let minutes = utc.minute();
        let am_pm = if hours >= 12 { "PM" } else { "AM" };
        let hour_12 = if hours == 0 {
            12
        } else if hours > 12 {
            hours - 12
        } else {
            hours
        };
        format!(
            "{} {} at {}:{:02} {} UTC",
            month_str, day, hour_12, minutes, am_pm
        )
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[component]
pub fn EventMapSkeleton() -> Element {
    rsx! {
        div {
            class: "event-map-skeleton rounded-lg overflow-hidden border border-border bg-muted animate-pulse",
            style: "height: 400px;",
            div { class: "h-full flex items-center justify-center",
                span { class: "text-muted-foreground", "Loading map..." }
            }
        }
    }
}
