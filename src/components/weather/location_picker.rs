use dioxus::prelude::*;
use crate::stores::weather::location_store::*;
use crate::services::weather::units::format_temperature;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;
use crate::stores::weather::weather_store::WEATHER_DATA;

#[component]
pub fn LocationPicker(on_close: EventHandler<()>, on_search: EventHandler<()>) -> Element {
    let locations = LOCATIONS.read();
    let selected = *SELECTED_LOCATION_INDEX.read();
    let settings = WEATHER_SETTINGS.read();

    rsx! {
        div { class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm",
            div { class: "absolute bottom-0 left-0 right-0 bg-background rounded-t-2xl max-h-[70vh] overflow-auto p-4",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-semibold", "Locations" }
                    button {
                        class: "p-2 hover:bg-accent rounded-lg",
                        onclick: move |_| on_close.call(()),
                        "\u{2715}"
                    }
                }
                button {
                    class: "w-full flex items-center gap-3 p-3 rounded-xl bg-accent/50 hover:bg-accent mb-2",
                    onclick: move |_| on_search.call(()),
                    div { class: "w-8 h-8 rounded-full bg-muted flex items-center justify-center text-lg", "+" }
                    span { class: "text-sm", "Search for a location" }
                }
                for (i, loc) in locations.iter().enumerate() {
                    {
                        let is_selected = i == selected;
                        let temp = WEATHER_DATA.read().get(&loc.id).map(|d| {
                            format_temperature(d.current.temperature, settings.temperature_unit)
                        }).unwrap_or_default();
                        let weather_emoji = WEATHER_DATA.read().get(&loc.id).map(|d| d.current.weather_code.emoji().to_string()).unwrap_or_default();
                        rsx! {
                            button {
                                key: "{loc.id}",
                                class: if is_selected { "w-full flex items-center gap-3 p-3 rounded-xl bg-accent" } else { "w-full flex items-center gap-3 p-3 rounded-xl hover:bg-accent" },
                                onclick: move |_| {
                                    select_location(i);
                                    on_close.call(());
                                },
                                div { class: "flex-1 text-left",
                                    div { class: "font-medium text-sm", "{loc.name}" }
                                    if loc.is_current_gps {
                                        div { class: "text-xs text-muted-foreground", "Current location" }
                                    }
                                }
                                if !weather_emoji.is_empty() {
                                    span { class: "text-xl", "{weather_emoji}" }
                                    span { class: "font-semibold", "{temp}" }
                                }
                                if locations.len() > 1 {
                                    {
                                        let loc_id = loc.id.clone();
                                        rsx! {
                                            button {
                                                class: "p-1 hover:bg-destructive/20 rounded-lg text-muted-foreground hover:text-destructive transition",
                                                onclick: move |e: Event<MouseData>| {
                                                    e.stop_propagation();
                                                    remove_location(&loc_id);
                                                },
                                                "\u{2715}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if locations.len() > 1 {
                    p { class: "text-xs text-muted-foreground mt-4 text-center", "Swipe on main page to switch locations" }
                }
            }
        }
    }
}
