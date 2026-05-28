use crate::services::geocoding::{self, GeoLocation};
use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
pub struct LocationResult {
    pub lat: f64,
    pub lon: f64,
    pub display_name: String,
    pub city: Option<String>,
    pub state: Option<String>,
}

#[derive(Props, Clone, PartialEq)]
pub struct PlacesLocationSearchProps {
    pub on_select: EventHandler<LocationResult>,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn PlacesLocationSearch(props: PlacesLocationSearchProps) -> Element {
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<GeoLocation>::new);
    let mut is_searching = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    rsx! {
        div { class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm",
            div { class: "absolute bottom-0 left-0 right-0 bg-background rounded-t-2xl max-h-[80vh] overflow-auto",
                div { class: "sticky top-0 bg-background p-4 border-b border-border",
                    div { class: "flex items-center gap-3",
                        button {
                            class: "p-2 hover:bg-accent rounded-lg",
                            onclick: move |_| props.on_close.call(()),
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
                                    spawn(async move {
                                        match geocoding::geocode_suggestions(&val, 5).await {
                                            Ok(locs) => {
                                                results.set(locs);
                                                is_searching.set(false);
                                            }
                                            Err(err) => {
                                                error.set(Some(err));
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
                        onclick: {
                            let on_select = props.on_select;
                            move |_| {
                                is_searching.set(true);
                                let on_select = on_select;
                                spawn(async move {
                                    match crate::platform::geolocation::get_current_position().await {
                                        Ok((lat, lon)) => {
                                            let display = match geocoding::reverse_geocode_city(lat, lon).await {
                                                Ok(Some(loc)) => LocationResult {
                                                    lat,
                                                    lon,
                                                    display_name: loc.display_name,
                                                    city: loc.city,
                                                    state: loc.state,
                                                },
                                                _ => LocationResult {
                                                    lat,
                                                    lon,
                                                    display_name: format!("{:.2}, {:.2}", lat, lon),
                                                    city: None,
                                                    state: None,
                                                },
                                            };
                                            on_select.call(display);
                                        }
                                        Err(e) => {
                                            error.set(Some(e));
                                            is_searching.set(false);
                                        }
                                    }
                                });
                            }
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
                                let on_select = props.on_select;
                                rsx! {
                                    button {
                                        key: "{loc.display_name}",
                                        class: "w-full text-left p-3 rounded-xl hover:bg-accent transition",
                                        onclick: move |_| {
                                            on_select.call(LocationResult {
                                                lat: loc_clone.lat,
                                                lon: loc_clone.lon,
                                                display_name: loc_clone.display_name.clone(),
                                                city: loc_clone.city.clone(),
                                                state: loc_clone.state.clone(),
                                            });
                                        },
                                        div { class: "font-medium text-sm", "{loc_clone.display_name}" }
                                        if let Some(city) = &loc_clone.city {
                                            div { class: "text-xs text-muted-foreground",
                                                {match (&loc_clone.state, &loc_clone.country) {
                                                    (Some(s), _) => format!("{}, {}", city, s),
                                                    _ => city.clone(),
                                                }}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !results.read().is_empty() {
                            p { class: "text-xs text-muted-foreground text-center mt-2",
                                "{results.read().len()} results"
                            }
                        }
                    }
                }
            }
        }
    }
}
