//! CategorySelector component - category chips for filtering

use dioxus::prelude::*;

/// Common marketplace categories
/// Used by CategorySelector component for filtering products
pub const CATEGORIES: &[(&str, &str)] = &[
    ("electronics", "Electronics"),
    ("clothing", "Clothing"),
    ("home", "Home & Garden"),
    ("collectibles", "Collectibles"),
    ("art", "Art"),
    ("books", "Books"),
    ("music", "Music"),
    ("games", "Games"),
    ("sports", "Sports"),
    ("bitcoin", "Bitcoin"),
    ("nostr", "Nostr"),
    ("digital", "Digital"),
    ("services", "Services"),
    ("other", "Other"),
];

#[derive(Props, Clone, PartialEq)]
pub struct CategorySelectorProps {
    pub selected: Vec<String>,
    pub on_change: EventHandler<Vec<String>>,
    #[props(default = false)]
    pub multi_select: bool,
}

/// Category chip selector
#[component]
pub fn CategorySelector(props: CategorySelectorProps) -> Element {
    rsx! {
        div { class: "flex flex-wrap gap-2",
            for (id, label) in CATEGORIES.iter() {
                {
                    let id_str = id.to_string();
                    let is_selected = props.selected.contains(&id_str);
                    let selected_class = if is_selected {
                        "bg-blue-500 text-white border-blue-500"
                    } else {
                        "bg-card hover:bg-accent border-border"
                    };

                    rsx! {
                        button {
                            key: "{id}",
                            r#type: "button",
                            class: "px-3 py-1.5 rounded-full text-sm font-medium border transition {selected_class}",
                            onclick: {
                                let id_owned = id_str.clone();
                                let current = props.selected.clone();
                                let on_change = props.on_change;
                                let multi = props.multi_select;
                                move |_| {
                                    let mut new_selected = if multi {
                                        current.clone()
                                    } else {
                                        vec![]
                                    };

                                    if new_selected.contains(&id_owned) {
                                        new_selected.retain(|x| x != &id_owned);
                                    } else {
                                        new_selected.push(id_owned.clone());
                                    }

                                    on_change.call(new_selected);
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
