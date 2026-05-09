use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::blobbi_visual::BlobbiVisual;
use crate::components::sheet::{Sheet, SheetContent, SheetDescription, SheetHeader, SheetSide, SheetTitle};
use crate::stores::blobbi_store;

#[component]
pub fn BlobbiSelector(show: bool, on_close: EventHandler<bool>) -> Element {
    rsx! {
        Sheet {
            open: show,
            on_open_change: move |open: bool| {
                if !open {
                    on_close.call(false);
                }
            },
            SheetContent {
                side: SheetSide::Bottom,
                class: "border-t border-border bg-background max-h-[80vh]",
                SheetHeader {
                    SheetTitle { "Your Blobbis" }
                    SheetDescription { "Select a Blobbi to switch to" }
                }
                div { class: "p-4 space-y-2 max-h-[60vh] overflow-y-auto",
                    {render_blobbi_list()}
                }
            }
        }
    }
}

fn render_blobbi_list() -> Element {
    let store = blobbi_store::BLOBBI_COLLECTION.read();
    let selected_d = store.selected_d.clone();
    let blobbis = store.collection.clone();

    if blobbis.is_empty() {
        return rsx! {
            div { class: "text-center py-8 text-muted-foreground text-sm",
                "No Blobbis yet"
            }
        };
    }

    rsx! {
        for blobbi in &blobbis {
            {render_blobbi_card(blobbi, &selected_d)}
        }
    }
}

fn render_blobbi_card(blobbi: &BlobbiCompanion, selected_d: &Option<String>) -> Element {
    let is_selected = selected_d.as_ref() == Some(&blobbi.d);
    let d = blobbi.d.clone();

    rsx! {
        button {
            class: if is_selected {
                "w-full flex items-center gap-3 p-3 rounded-xl bg-primary/10 border-2 border-primary transition"
            } else {
                "w-full flex items-center gap-3 p-3 rounded-xl bg-card border border-border hover:bg-accent transition"
            },
            onclick: move |_| {
                blobbi_store::select_blobbi(d.clone());
            },

            div { class: "w-12 h-12 flex items-center justify-center",
                BlobbiVisual {
                    blobbi: blobbi.clone(),
                    size: Some("48".to_string()),
                }
            }

            div { class: "flex-1 text-left min-w-0",
                div { class: "flex items-center gap-2",
                    span { class: "text-sm font-medium truncate", "{blobbi.display_name()}" }
                    if blobbi.is_egg() {
                        span { class: "text-xs", "🥚" }
                    } else if blobbi.is_baby() {
                        span { class: "text-xs", "🐣" }
                    } else {
                        span { class: "text-xs", "🐦" }
                    }
                    if is_selected {
                        span { class: "text-xs text-primary", "●" }
                    }
                }
                div { class: "flex gap-1 mt-1",
                    {render_mini_stat(blobbi.stats.hunger)}
                    {render_mini_stat(blobbi.stats.happiness)}
                    {render_mini_stat(blobbi.stats.health)}
                    {render_mini_stat(blobbi.stats.hygiene)}
                    {render_mini_stat(blobbi.stats.energy)}
                }
            }

            if blobbi.is_sleeping() {
                span { class: "text-xs text-muted-foreground", "💤" }
            }
        }
    }
}

#[allow(dead_code)]
fn render_mini_stat(value: f64) -> Element {
    let color = if value >= 70.0 { "bg-green-500" } else if value >= 40.0 { "bg-yellow-500" } else { "bg-red-500" };
    let width = (value / 100.0 * 100.0).min(100.0);

    rsx! {
        div { class: "flex-1",
            div { class: "w-full h-1 bg-muted rounded-full overflow-hidden",
                div {
                    class: "h-full {color} rounded-full",
                    style: "width: {width:.0}%",
                }
            }
        }
    }
}
