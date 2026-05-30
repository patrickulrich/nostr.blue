use crate::services::places::AMENITY_CATEGORIES;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct AmenityFilterProps {
    pub selected: Option<String>,
    pub on_change: EventHandler<Option<String>>,
}

#[component]
pub fn AmenityFilter(props: AmenityFilterProps) -> Element {
    let selected = props.selected.clone();

    rsx! {
        div { class: "flex gap-2 overflow-x-auto pb-2 scrollbar-styled",
            button {
                class: if selected.is_none() {
                    "px-3 py-1.5 rounded-full text-sm font-medium whitespace-nowrap bg-blue-500 text-white border border-blue-500"
                } else {
                    "px-3 py-1.5 rounded-full text-sm font-medium whitespace-nowrap bg-card hover:bg-accent border border-border transition"
                },
                onclick: {
                    let on_change = props.on_change;
                    move |_| on_change.call(None)
                },
                "All"
            }

            for (key, label) in AMENITY_CATEGORIES {
                {
                    let is_active = selected.as_deref() == Some(*key);
                    let key_owned = key.to_string();
                    let on_change = props.on_change;
                    rsx! {
                        button {
                            key: "{key}",
                            class: if is_active {
                                "px-3 py-1.5 rounded-full text-sm font-medium whitespace-nowrap bg-blue-500 text-white border border-blue-500"
                            } else {
                                "px-3 py-1.5 rounded-full text-sm font-medium whitespace-nowrap bg-card hover:bg-accent border border-border transition"
                            },
                            onclick: move |_| {
                                if is_active {
                                    on_change.call(None);
                                } else {
                                    on_change.call(Some(key_owned.clone()));
                                }
                            },
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}
