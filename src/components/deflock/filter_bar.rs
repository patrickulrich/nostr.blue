use crate::stores::deflock_store;
use dioxus::prelude::*;

#[component]
pub fn DeflockFilterBar() -> Element {
    let operators = deflock_store::get_unique_operators();
    let zones = vec!["traffic", "town", "parking", "other"];

    let current_filters = deflock_store::FILTERS.read().clone();
    let search = current_filters.search_query.clone();

    rsx! {
        div { class: "absolute bottom-20 left-4 right-4 z-[1000] bg-black/70 backdrop-blur-md rounded-xl p-3 max-w-md mx-auto pointer-events-auto",
            input {
                class: "w-full bg-white/5 border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder:text-white/40 mb-2",
                r#type: "text",
                placeholder: "Search cameras...",
                value: "{search}",
                oninput: move |e| {
                    let mut filters = deflock_store::FILTERS.read().clone();
                    filters.search_query = e.value().clone();
                    *deflock_store::FILTERS.write() = filters;
                }
            }

            div { class: "flex flex-wrap gap-1.5",
                for zone in zones {
                    {
                        let is_active = current_filters.zones.contains(zone);
                        let zone_label = match zone {
                            "traffic" => "Traffic",
                            "town" => "Town",
                            "parking" => "Parking",
                            _ => "Other",
                        };
                        rsx! {
                            button {
                                class: if is_active {
                                    "px-2.5 py-1 rounded-full text-xs font-medium bg-red-500 text-white"
                                } else {
                                    "px-2.5 py-1 rounded-full text-xs font-medium bg-white/5 text-white/60 border border-white/10"
                                },
                                onclick: move |_| {
                                    let mut filters = deflock_store::FILTERS.read().clone();
                                    if filters.zones.contains(zone) {
                                        filters.zones.remove(zone);
                                    } else {
                                        filters.zones.insert(zone.to_string());
                                    }
                                    *deflock_store::FILTERS.write() = filters;
                                },
                                "{zone_label}"
                            }
                        }
                    }
                }
            }

            if !operators.is_empty() {
                details { class: "mt-2",
                    summary { class: "text-xs text-white/60 cursor-pointer hover:text-white",
                        "Operators ({operators.len()})"
                    }
                    div { class: "mt-1 max-h-32 overflow-y-auto",
                        for op in operators {
                            {
                                let is_active = current_filters.operators.contains(&op);
                                rsx! {
                                    label { class: "flex items-center gap-2 text-xs text-white/70 py-0.5",
                                        input {
                                            r#type: "checkbox",
                                            checked: is_active,
                                            onclick: move |_| {
                                                let mut filters = deflock_store::FILTERS.read().clone();
                                                if filters.operators.contains(&op) {
                                                    filters.operators.remove(&op);
                                                } else {
                                                    filters.operators.insert(op.clone());
                                                }
                                                *deflock_store::FILTERS.write() = filters;
                                            }
                                        }
                                        "{op}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
