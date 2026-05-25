use dioxus::prelude::*;

use crate::stores::weather::location_store;

#[component]
pub fn WeatherSearch() -> Element {
    let query = use_signal(String::new);
    let results = use_signal(Vec::<crate::services::weather::LocationCandidate>::new);
    let is_searching = use_signal(|| false);
    let error = use_signal(|| None::<String>);

    rsx! {
        WeatherSearchInner {
            query: query,
            results: results,
            is_searching: is_searching,
            error: error,
        }
    }
}

#[component]
pub fn WeatherSearchInline(
    on_close: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<crate::services::weather::LocationCandidate>::new);
    let mut is_searching = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    rsx! {
        div { class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm",
            div { class: "absolute bottom-0 left-0 right-0 bg-background rounded-t-2xl max-h-[80vh] overflow-auto",
                div { class: "sticky top-0 bg-background p-4 border-b border-border",
                    div { class: "flex items-center gap-3",
                        button {
                            class: "p-2 hover:bg-accent rounded-lg",
                            onclick: move |_| on_close.call(()),
                            "\u{2190}"
                        }
                        input {
                            class: "flex-1 bg-muted rounded-lg px-4 py-2 text-sm outline-none focus:ring-2 focus:ring-ring",
                            r#type: "text",
                            placeholder: "Search for a city...",
                            value: "{query}",
                            autofocus: true,
                            oninput: move |e: Event<FormData>| {
                                let val = e.value().clone();
                                query.set(val.clone());
                                if val.len() >= 2 {
                                    is_searching.set(true);
                                    error.set(None);
                                    let q = val.clone();
                                    spawn(async move {
                                        match crate::services::weather::nws_alerts::search_locations(&q).await {
                                            Ok(locs) => {
                                                results.set(locs);
                                                is_searching.set(false);
                                            }
                                            Err(e) => {
                                                error.set(Some(e));
                                                is_searching.set(false);
                                            }
                                        }
                                    });
                                } else {
                                    results.set(vec![]);
                                }
                            },
                        }
                    }
                }

                div { class: "p-4",
                    button {
                        class: "w-full flex items-center gap-3 p-3 rounded-xl bg-accent/50 hover:bg-accent mb-3",
                        onclick: move |_| {
                            is_searching.set(true);
                            spawn(async move {
                                match location_store::init_gps_location().await {
                                    Ok(_) => {
                                        is_searching.set(false);
                                        on_close.call(());
                                    }
                                    Err(e) => {
                                        error.set(Some(e));
                                        is_searching.set(false);
                                    }
                                }
                            });
                        },
                        crate::components::icons::MapPinIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        span { class: "text-sm", "Use current location" }
                    }

                    if *is_searching.read() {
                        div { class: "flex justify-center py-8",
                            div { class: "w-6 h-6 border-2 border-current border-t-transparent rounded-full animate-spin" }
                        }
                    } else if let Some(err) = error.read().as_ref() {
                        p { class: "text-sm text-destructive text-center py-4", "{err}" }
                    } else {
                        for loc in results.read().iter() {
                            {
                                let loc_clone = loc.clone();
                                rsx! {
                                    button {
                                        key: "{loc.id}",
                                        class: "w-full text-left p-3 rounded-xl hover:bg-accent transition",
                                        onclick: move |_| {
                                            location_store::add_location_from_candidate(&loc_clone);
                                            on_close.call(());
                                        },
                                        div { class: "font-medium text-sm", "{loc.name}" }
                                        div { class: "text-xs text-muted-foreground",
                                            {match (&loc.admin1, &loc.country) {
                                                (Some(a), Some(c)) => format!("{}, {}", a, c),
                                                (None, Some(c)) => c.clone(),
                                                (Some(a), None) => a.clone(),
                                                _ => String::new(),
                                            }}
                                        }
                                    }
                                }
                            }
                        }
                        if !results.read().is_empty() {
                            p { class: "text-xs text-muted-foreground text-center mt-2", "{results.read().len()} results" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn WeatherSearchInner(
    query: Signal<String>,
    mut results: Signal<Vec<crate::services::weather::LocationCandidate>>,
    mut is_searching: Signal<bool>,
    mut error: Signal<Option<String>>,
) -> Element {
    let nav = navigator();
    rsx! {
        WeatherSearchInline { on_close: move |_| { nav.push(crate::routes::Route::WeatherHome {}); } }
    }
}
