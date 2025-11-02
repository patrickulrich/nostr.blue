use dioxus::prelude::*;
use crate::stores::gif_store::{GIF_RESULTS, GIF_LOADING, RECENT_GIFS, load_initial_gifs, load_more_gifs, add_recent_gif, search_gifs};

#[derive(Props, Clone, PartialEq)]
pub struct GifPickerProps {
    pub on_gif_selected: EventHandler<String>,
}

#[component]
pub fn GifPicker(props: GifPickerProps) -> Element {
    let mut show_picker = use_signal(|| false);
    let mut position_below = use_signal(|| false);
    let button_id = use_signal(|| format!("gif-picker-{}", uuid::Uuid::new_v4()));
    let mut initialized = use_signal(|| false);
    let mut search_query = use_signal(|| String::new());

    // Read GIF state from global store
    let gif_results = GIF_RESULTS.read();
    let gif_loading = GIF_LOADING.read();
    let recent_gifs = RECENT_GIFS.read();

    // Debounced search effect
    use_effect(move || {
        let query = search_query.read().clone();
        if initialized.read().clone() {
            spawn(async move {
                // Wait 300ms for debouncing
                #[cfg(target_family = "wasm")]
                {
                    gloo_timers::future::TimeoutFuture::new(300).await;
                }
                #[cfg(not(target_family = "wasm"))]
                {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                }

                search_gifs(query).await;
            });
        }
    });

    rsx! {
        div {
            class: "relative",

            // GIF button
            button {
                id: "{button_id}",
                class: "px-3 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg text-sm font-medium transition",
                onclick: move |_| {
                    let current = *show_picker.read();
                    show_picker.set(!current);

                    // Load GIFs on first open
                    if !current && !*initialized.read() {
                        initialized.set(true);
                        spawn(async move {
                            load_initial_gifs().await;
                        });
                    }

                    // Calculate position when opening
                    if !current {
                        #[cfg(target_family = "wasm")]
                        {
                            let btn_id = button_id.read().clone();
                            if let Some(window) = web_sys::window() {
                                if let Some(document) = window.document() {
                                    if let Some(element) = document.get_element_by_id(&btn_id) {
                                        let rect = element.get_bounding_client_rect();
                                        let viewport_height = window
                                            .inner_height()
                                            .ok()
                                            .and_then(|h| h.as_f64())
                                            .unwrap_or(800.0);

                                        let button_center_y = rect.top() + (rect.height() / 2.0);
                                        let is_in_top_half = button_center_y < (viewport_height / 2.0);

                                        // If button is in top half, show popup below; otherwise show above
                                        position_below.set(is_in_top_half);
                                    }
                                }
                            }
                        }
                    }
                },
                "🎬 GIF"
            }

            // GIF picker popover
            if *show_picker.read() {
                div {
                    class: if *position_below.read() {
                        "absolute top-full left-0 mt-2 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl shadow-2xl z-50 w-[700px]"
                    } else {
                        "absolute bottom-full left-0 mb-2 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl shadow-2xl z-50 w-[700px]"
                    },
                    onclick: move |e| e.stop_propagation(),

                    // Header
                    div {
                        class: "flex items-center justify-between p-4 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-750 rounded-t-xl",
                        h3 {
                            class: "text-base font-bold text-gray-900 dark:text-gray-100",
                            "🎬 Select GIF"
                        }
                        button {
                            class: "text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-full p-1 transition",
                            onclick: move |_| show_picker.set(false),
                            "✕"
                        }
                    }

                    // Search input
                    div {
                        class: "p-4 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-750",
                        div {
                            class: "relative",
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-2.5 pl-10 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 placeholder-gray-500 dark:placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent text-sm shadow-sm",
                                placeholder: "Search GIFs (powered by NIP-50)...",
                                value: "{search_query}",
                                oninput: move |evt| {
                                    search_query.set(evt.value().clone());
                                }
                            }
                            // Search icon
                            span {
                                class: "absolute left-3 top-1/2 transform -translate-y-1/2 text-gray-400",
                                "🔍"
                            }
                        }
                    }

                    // Content area with scrolling
                    div {
                        class: "overflow-y-auto",
                        style: "max-height: 500px;",

                        // Recent GIFs section (if any and no active search)
                        if !recent_gifs.is_empty() && search_query.read().is_empty() {
                            div {
                                class: "p-4 border-b border-gray-200 dark:border-gray-700 bg-gradient-to-r from-gray-50 to-white dark:from-gray-750 dark:to-gray-800",
                                h4 {
                                    class: "text-xs font-bold text-gray-600 dark:text-gray-300 uppercase tracking-wide mb-3 flex items-center gap-2",
                                    span { "⏱️" }
                                    "Recent"
                                }
                                div {
                                    class: "flex gap-3 overflow-x-auto pb-2",
                                    for (idx, gif) in recent_gifs.iter().take(10).enumerate() {
                                        {
                                            let gif_url = gif.url.clone();
                                            let gif_url_for_click = gif.url.clone();
                                            let thumb_url = gif.thumbnail.clone().unwrap_or_else(|| gif.url.clone());
                                            let alt_text = format!("Recent GIF {}", idx + 1);
                                            let gif_clone = gif.clone();
                                            rsx! {
                                                button {
                                                    key: "recent-{idx}",
                                                    class: "flex-shrink-0 relative group",
                                                    title: "{gif_url}",
                                                    onclick: move |_| {
                                                        props.on_gif_selected.call(gif_url_for_click.clone());
                                                        add_recent_gif(gif_clone.clone());
                                                        show_picker.set(false);
                                                    },
                                                    img {
                                                        src: "{thumb_url}",
                                                        alt: "{alt_text}",
                                                        class: "w-24 h-24 object-cover rounded-lg border-2 border-transparent group-hover:border-blue-500 group-hover:scale-105 transition-all duration-200 shadow-sm hover:shadow-md"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // GIF grid
                        div {
                            class: "p-4",

                            // Loading state
                            if *gif_loading && gif_results.is_empty() {
                                div {
                                    class: "flex flex-col items-center justify-center py-16 text-gray-500 dark:text-gray-400",
                                    div {
                                        class: "animate-spin rounded-full h-12 w-12 border-4 border-blue-500 border-t-transparent mb-4"
                                    }
                                    p {
                                        class: "text-base font-medium",
                                        "Loading GIFs..."
                                    }
                                    p {
                                        class: "text-xs mt-1 text-gray-400",
                                        "Searching Nostr relays"
                                    }
                                }
                            }

                            // Empty state
                            if !*gif_loading && gif_results.is_empty() {
                                div {
                                    class: "flex flex-col items-center justify-center py-16 text-gray-500 dark:text-gray-400",
                                    span {
                                        class: "text-5xl mb-4",
                                        "🔍"
                                    }
                                    p {
                                        class: "text-base font-medium",
                                        "No GIFs found"
                                    }
                                    p {
                                        class: "text-xs mt-2 text-gray-400 text-center max-w-xs",
                                        "Try a different search term or wait for GIFs to load from Nostr relays"
                                    }
                                }
                            }

                            // GIF grid
                            if !gif_results.is_empty() {
                                div {
                                    class: "grid grid-cols-6 gap-2",
                                    for (idx, gif) in gif_results.iter().enumerate() {
                                        {
                                            let gif_url = gif.url.clone();
                                            let gif_url_for_click = gif.url.clone();
                                            let thumb_url = gif.thumbnail.clone().unwrap_or_else(|| gif.url.clone());
                                            let alt_text = format!("GIF {}", idx + 1);
                                            let title_text = if let Some((w, h)) = gif.dimensions {
                                                format!("{}x{}", w, h)
                                            } else {
                                                gif_url.clone()
                                            };
                                            let gif_clone = gif.clone();
                                            rsx! {
                                                button {
                                                    key: "gif-{idx}",
                                                    class: "relative group aspect-square overflow-hidden rounded-lg bg-gray-100 dark:bg-gray-700",
                                                    title: "{title_text}",
                                                    onclick: move |_| {
                                                        props.on_gif_selected.call(gif_url_for_click.clone());
                                                        add_recent_gif(gif_clone.clone());
                                                        show_picker.set(false);
                                                    },
                                                    img {
                                                        src: "{thumb_url}",
                                                        alt: "{alt_text}",
                                                        class: "w-full h-full object-cover group-hover:scale-110 transition-transform duration-200"
                                                    }
                                                    // Hover overlay
                                                    div {
                                                        class: "absolute inset-0 bg-blue-500 bg-opacity-0 group-hover:bg-opacity-20 transition-all duration-200 pointer-events-none"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Footer with Load More button
                    if !gif_results.is_empty() {
                        div {
                            class: "p-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-750 rounded-b-xl",
                            button {
                                class: "w-full px-4 py-3 bg-gradient-to-r from-blue-500 to-blue-600 hover:from-blue-600 hover:to-blue-700 text-white rounded-lg text-sm font-semibold transition-all disabled:opacity-50 disabled:cursor-not-allowed shadow-sm hover:shadow-md flex items-center justify-center gap-2",
                                disabled: *gif_loading,
                                onclick: move |_| {
                                    spawn(async move {
                                        load_more_gifs().await;
                                    });
                                },
                                if *gif_loading {
                                    span {
                                        class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"
                                    }
                                    "Loading More GIFs..."
                                } else {
                                    span { "⬇️" }
                                    "Load More GIFs"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
