use crate::components::icons;
use crate::stores::music_player::{self, MUSIC_PLAYER};
use dioxus::prelude::*;

#[component]
pub fn PlayerFloating() -> Element {
    let state = MUSIC_PLAYER.read().clone();
    let track = state.current_track.as_ref().unwrap();

    let mut is_dragging = use_signal(|| false);
    let mut pos = use_signal(|| state.floating_pos);
    let mut drag_offset = use_signal(|| (0.0_f64, 0.0_f64));
    let mut total_drag_distance = use_signal(|| 0.0_f64);
    let mut drag_start_pos = use_signal(|| (0.0_f64, 0.0_f64));

    let current_pos = pos();
    let (px, py) = current_pos;
    let dragging = *is_dragging.read();

    let transition_style = if dragging {
        "none".to_string()
    } else {
        "left 200ms ease-out, top 200ms ease-out, transform 200ms ease-out, opacity 200ms ease-out"
            .to_string()
    };
    let pill_style = format!(
        "left: {}px; top: max({}px, var(--safe-area-top)); transition: {};",
        px, py, transition_style
    );

    let dismiss_scale = if py > 200.0 {
        "transform: scale(1.2); border-color: rgba(239, 68, 68, 0.8);"
    } else {
        ""
    };

    rsx! {
        if dragging {
            div {
                class: "fixed bottom-8 left-1/2 -translate-x-1/2 z-[61] flex flex-col items-center gap-1 transition-opacity duration-200",
                div {
                    class: "w-16 h-16 rounded-full bg-destructive/20 border-2 border-destructive/50 flex items-center justify-center transition-all duration-200",
                    style: "{dismiss_scale}",
                    div {
                        class: "text-destructive",
                        dangerous_inner_html: icons::DISMISS,
                    }
                }
                span { class: "text-xs text-muted-foreground", "Drag here to close" }
            }
        }

        div {
            class: "fixed z-[59] cursor-grab active:cursor-grabbing touch-none",
            style: "{pill_style}",
            onpointerdown: move |evt: Event<PointerData>| {
                let (cx, cy) = (evt.client_coordinates().x, evt.client_coordinates().y);
                let (cur_x, cur_y) = pos();
                is_dragging.set(true);
                drag_offset.set((cur_x - cx, cur_y - cy));
                drag_start_pos.set((cur_x, cur_y));
                total_drag_distance.set(0.0);
            },
            onpointermove: move |evt: Event<PointerData>| {
                if *is_dragging.read() {
                    let (cx, cy) = (evt.client_coordinates().x, evt.client_coordinates().y);
                    let (ox, oy) = *drag_offset.read();
                    let new_x = cx + ox;
                    let new_y = cy + oy;

                    let (sx, sy) = *drag_start_pos.read();
                    let dist = ((new_x - sx).powi(2) + (new_y - sy).powi(2)).sqrt();
                    total_drag_distance.set(dist);

                    pos.set((new_x, new_y));
                }
            },
            onpointerup: move |_| {
                is_dragging.set(false);

                let distance = *total_drag_distance.read();
                if distance < 5.0 {
                    music_player::restore_from_floating();
                    return;
                }

                let (cx, cy) = pos();

                #[cfg(feature = "web")]
                let dismiss_threshold = {
                    let vh = web_sys::window()
                        .and_then(|w| w.inner_height().ok())
                        .and_then(|h| h.as_f64())
                        .unwrap_or(800.0);
                    vh - 120.0
                };
                #[cfg(not(feature = "web"))]
                let dismiss_threshold = 800.0 - 120.0;

                if cy > dismiss_threshold {
                    music_player::close_player();
                    return;
                }

                let final_y = cy;

                #[cfg(feature = "web")]
                let final_x = {
                    let mut x = cx;
                    if let Some(window) = web_sys::window() {
                        if let Ok(w) = window.inner_width() {
                            let view_w = w.as_f64().unwrap_or(375.0);
                            if cx < view_w / 2.0 {
                                x = 16.0;
                            } else {
                                x = cx.max(16.0);
                            }
                        }
                    }
                    x
                };
                #[cfg(not(feature = "web"))]
                let final_x = cx.max(16.0);

                pos.set((final_x, final_y));
                music_player::set_floating_pos(final_x, final_y);
            },
            onpointercancel: move |_| {
                is_dragging.set(false);
            },

            div {
                class: "flex items-center gap-2 bg-background/95 border border-border rounded-full shadow-lg px-1 py-1 backdrop-blur",
                style: "backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);",

                div { class: "w-10 h-10 rounded-full overflow-hidden bg-muted shrink-0",
                    if let Some(art_url) = &track.album_art_url {
                        img {
                            src: "{art_url}",
                            alt: "Album art",
                            class: "w-full h-full object-cover",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center text-muted-foreground",
                            div { dangerous_inner_html: icons::MUSIC_NOTE }
                        }
                    }
                }

                button {
                    class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-full hover:bg-accent transition-colors shrink-0",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        music_player::toggle_play();
                    },
                    dangerous_inner_html: if state.is_playing { icons::PAUSE } else { icons::PLAY },
                }

                button {
                    class: "h-6 w-6 p-0 inline-flex items-center justify-center rounded-full hover:bg-accent transition-colors shrink-0 mr-1",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        music_player::close_player();
                    },
                    dangerous_inner_html: icons::X,
                }
            }
        }
    }
}
