use dioxus::prelude::*;

use crate::services::weather::rainviewer::{self, RainviewerMaps};
use std::sync::atomic::{AtomicU64, Ordering};

static RADAR_MAP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Props, Clone, PartialEq)]
pub struct WeatherRadarProps {
    pub lat: f64,
    pub lon: f64,
}

#[component]
pub fn WeatherRadar(props: WeatherRadarProps) -> Element {
    let container_id = use_signal(|| {
        format!(
            "radar-map-{}-{}",
            crate::platform::timestamp::now_millis(),
            RADAR_MAP_COUNTER.fetch_add(1, Ordering::Relaxed),
        )
    });
    #[allow(unused_mut)]
    let mut leaflet_loaded = use_signal(|| false);
    #[allow(unused_mut)]
    let mut map_initialized = use_signal(|| false);
    let mut radar_maps = use_signal(|| None::<RainviewerMaps>);
    let mut current_frame = use_signal(|| 0usize);
    let mut playing = use_signal(|| false);
    let mut loading_maps = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut unmounted = use_signal(|| false);
    let mut anim_gen = use_signal(|| 0u32);

    let lat = props.lat;
    let lon = props.lon;

    use_drop(move || {
        unmounted.set(true);
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
                const layers = window.leafletRadarLayers || new Map();
                layers.delete({id_json});
            }})()
            "#
        ));
    });

    use_effect(move || {
        if *leaflet_loaded.read() {
            return;
        }
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
                error_msg.set(Some(err_text.to_string()));
            } else {
                leaflet_loaded.set(true);
            }
        });
    });

    use_effect(move || {
        if *loading_maps.read() || radar_maps.read().is_some() {
            return;
        }
        loading_maps.set(true);
        spawn(async move {
            let result = rainviewer::fetch_radar_maps().await;
            if *unmounted.read() {
                return;
            }
            match result {
                Ok(maps) => {
                    radar_maps.set(Some(maps));
                }
                Err(e) => {
                    log::error!("RainViewer fetch failed: {}", e);
                }
            }
            loading_maps.set(false);
        });
    });

    use_effect(move || {
        if !*leaflet_loaded.read() || radar_maps.read().is_none() || *map_initialized.read() {
            return;
        }
        let id = container_id.read().clone();
        let maps = radar_maps.read().clone().unwrap();
        let path = maps
            .radar
            .past
            .first()
            .map(|f| f.path.clone())
            .unwrap_or_default();
        let host = maps.host.clone();
        let id_json = serde_json::to_string(&id).unwrap_or_default();
        let lat_json = serde_json::to_string(&lat).unwrap_or_default();
        let lon_json = serde_json::to_string(&lon).unwrap_or_default();
        let host_json = serde_json::to_string(&host).unwrap_or_default();
        let path_json = serde_json::to_string(&path).unwrap_or_default();
        spawn(async move {
            crate::platform::timer::sleep_ms(100).await;
            if *unmounted.read() {
                return;
            }
            let _ = dioxus::document::eval(&format!(
                r#"
                (() => {{
                    const container = document.getElementById({id_json});
                    if (!container) return;
                    const map = L.map({id_json}, {{
                        zoomControl: true,
                        attributionControl: false,
                        minZoom: 2,
                        maxZoom: 7
                    }}).setView([{lat_json}, {lon_json}], 7);
                    L.tileLayer('https://{{s}}.tile.openstreetmap.org/{{z}}/{{x}}/{{y}}.png', {{
                        maxZoom: 19
                    }}).addTo(map);
                    const radarUrl = {host_json} + {path_json} + '/256/{{z}}/{{x}}/{{y}}/2/1_1.png';
                    const radar = L.tileLayer(radarUrl, {{
                        opacity: 0.65,
                        zIndex: 10
                    }}).addTo(map);
                    window.leafletMaps = window.leafletMaps || new Map();
                    window.leafletMaps.set({id_json}, map);
                    window.leafletRadarLayers = window.leafletRadarLayers || new Map();
                    window.leafletRadarLayers.set({id_json}, radar);
                }})()
                "#
            )).await;
            map_initialized.set(true);
            let _ = (host_json, path_json);
        });
    });

    use_effect(move || {
        if !*playing.read() || !*map_initialized.read() {
            return;
        }
        let maps = radar_maps.read().clone();
        if maps.is_none() {
            return;
        }
        let frames = maps.unwrap().radar.past;
        if frames.is_empty() {
            return;
        }
        let this_gen = *anim_gen.peek() + 1;
        anim_gen.set(this_gen);
        let id = container_id.read().clone();
        let host = radar_maps.read().as_ref().map(|m| m.host.clone()).unwrap_or_default();
        spawn(async move {
            loop {
                crate::platform::timer::sleep_ms(600).await;
                if *unmounted.read() || !*playing.read() || *anim_gen.read() != this_gen {
                    break;
                }
                let next = (*current_frame.read() + 1) % frames.len();
                current_frame.set(next);
                let path = &frames[next].path;
                let id_json = serde_json::to_string(&id).unwrap_or_default();
                let host_json = serde_json::to_string(&host).unwrap_or_default();
                let path_json = serde_json::to_string(path).unwrap_or_default();
                let _ = dioxus::document::eval(&format!(
                    r#"
                    (() => {{
                        const maps = window.leafletMaps || new Map();
                        const layers = window.leafletRadarLayers || new Map();
                        const map = maps.get({id_json});
                        const old = layers.get({id_json});
                        if (old && map) map.removeLayer(old);
                        if (!map) return;
                        const radarUrl = {host_json} + {path_json} + '/256/{{z}}/{{x}}/{{y}}/2/1_1.png';
                        const radar = L.tileLayer(radarUrl, {{
                            opacity: 0.65,
                            zIndex: 10
                        }}).addTo(map);
                        layers.set({id_json}, radar);
                    }})()
                    "#
                )).await;
            }
        });
    });

    let maps = radar_maps.read().clone();
    let frame_count = maps
        .as_ref()
        .map(|m| m.radar.past.len())
        .unwrap_or(0);
    let frame_time = maps
        .as_ref()
        .and_then(|m| m.radar.past.get(*current_frame.read()))
        .map(|f| {
            let secs = f.time as i64;
            let utc: chrono::DateTime<chrono::Utc> = chrono::DateTime::from_timestamp(secs, 0)
                .unwrap_or_default();
            utc.format("%H:%M UTC").to_string()
        })
        .unwrap_or_default();

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-4",
            div { class: "flex items-center gap-2 mb-3",
                crate::components::icons::DropletIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                span { class: "font-semibold", "Radar" }
                if frame_count > 0 {
                    span { class: "text-xs text-muted-foreground ml-auto",
                        if !frame_time.is_empty() {
                            "{frame_time}"
                        }
                        span { class: "ml-2 opacity-60",
                            "{*current_frame.read() + 1}/{frame_count}"
                        }
                    }
                }
            }

            div { class: "relative rounded-lg overflow-hidden isolate z-0",
                div {
                    id: "{container_id}",
                    class: "bg-muted",
                    style: "height: 400px; width: 100%;",
                }

                if let Some(err) = error_msg.read().as_ref() {
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
                            p { class: "text-sm text-muted-foreground", "{err}" }
                        }
                    }
                } else if !*map_initialized.read() {
                    div { class: "absolute inset-0 flex items-center justify-center bg-background/80",
                        div { class: "flex flex-col items-center gap-2",
                            div { class: "w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                            span { class: "text-sm text-muted-foreground", "Loading radar..." }
                        }
                    }
                }
            }

            div { class: "flex items-center justify-between mt-3",
                div { class: "flex items-center gap-2",
                    if frame_count > 1 {
                        button {
                            class: if *playing.read() {
                                "p-2 rounded-lg bg-primary text-primary-foreground"
                            } else {
                                "p-2 rounded-lg bg-muted hover:bg-accent transition"
                            },
                            onclick: move |_| {
                                let next = !*playing.read();
                                playing.set(next);
                            },
                            if *playing.read() {
                                "Pause"
                            } else {
                                "Play"
                            }
                        }
                    }
                }
                p { class: "text-xs text-muted-foreground",
                    "Radar data from "
                    a {
                        href: "https://www.rainviewer.com/",
                        target: "_blank",
                        class: "underline",
                        "RainViewer"
                    }
                }
            }
        }
    }
}
