use crate::routes::Route;
use crate::stores::social::group_store::{encode_relay_url, Group};
use dioxus::prelude::*;

#[component]
pub fn GroupCard(group: Group) -> Element {
    let encoded_relay = encode_relay_url(&group.relay_url);
    let name = group.name.clone().unwrap_or_else(|| group.id.clone());
    let picture = group.picture.clone();
    let nav = use_navigator();
    let group_id = group.id.clone();

    rsx! {
        div {
            class: "bg-card border border-border rounded-lg p-4 cursor-pointer hover:bg-accent/50 transition",
            onclick: move |_| {
                nav.push(Route::GroupDetail {
                    encoded_relay: encoded_relay.clone(),
                    group_id: group_id.clone(),
                });
            },
            div { class: "flex items-center gap-3",
                if let Some(pic) = &picture {
                    img {
                        class: "w-12 h-12 rounded-full object-cover",
                        src: pic,
                        alt: "{name}",
                    }
                } else {
                    div { class: "w-12 h-12 rounded-full bg-muted flex items-center justify-center text-lg font-semibold text-muted-foreground",
                        "{name.chars().next().unwrap_or('?')}"
                    }
                }
                div { class: "flex-1 min-w-0",
                    h3 { class: "font-semibold text-foreground truncate", "{name}" }
                    if let Some(about) = &group.about {
                        p { class: "text-sm text-muted-foreground truncate", "{about}" }
                    }
                }
            }
            div { class: "flex gap-2 mt-2 flex-wrap",
                if group.is_private {
                    span { class: "text-xs px-2 py-0.5 rounded bg-yellow-500/20 text-yellow-600 dark:text-yellow-400", "Private" }
                }
                if group.is_closed {
                    span { class: "text-xs px-2 py-0.5 rounded bg-red-500/20 text-red-600 dark:text-red-400", "Closed" }
                }
                if group.is_restricted {
                    span { class: "text-xs px-2 py-0.5 rounded bg-orange-500/20 text-orange-600 dark:text-orange-400", "Restricted" }
                }
            }
        }
    }
}

#[component]
pub fn GroupCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 animate-pulse",
            div { class: "flex items-center gap-3",
                div { class: "w-12 h-12 rounded-full bg-muted" }
                div { class: "flex-1",
                    div { class: "h-4 bg-muted rounded w-1/2 mb-2" }
                    div { class: "h-3 bg-muted rounded w-3/4" }
                }
            }
        }
    }
}
