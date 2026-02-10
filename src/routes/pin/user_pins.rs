//! User Pins Page
//! View and search your pinned content and boards
use dioxus::prelude::*;
use std::collections::HashMap;
use crate::components::board::item_selector::PinToBoardModal;
use crate::components::{PinBoardMosaicGrid, PinMosaicGrid, PinToBoardRequest};
use crate::routes::Route;
use crate::stores::auth_store;
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::pin_boards_store::{self, Pin, PinMetadata, Pinboard};
/// Tab selection for the user pins page
#[derive(Clone, PartialEq, Default)]
pub enum UserPinsTab {
    #[default]
    Pins,
    Boards,
}
#[component]
pub fn UserPins() -> Element {
    let mut active_tab = use_signal(|| UserPinsTab::Pins);
    let mut search_query = use_signal(String::new);
    let mut pins = use_signal(Vec::<Pin>::new);
    let mut boards = use_signal(Vec::<Pinboard>::new);
    let mut metadata_map = use_signal(HashMap::<String, PinMetadata>::new);
    let mut pins_loading = use_signal(|| true);
    let mut boards_loading = use_signal(|| true);
    let mut pin_to_board_request: Signal<Option<PinToBoardRequest>> = use_signal(|| {
        None
    });
    let is_authenticated = auth_store::get_pubkey().is_some();
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        if !is_authenticated {
            pins_loading.set(false);
            boards_loading.set(false);
            return;
        }
        spawn(async move {
            match pin_boards_store::fetch_my_pins().await {
                Ok(user_pins) => {
                    let enriched = pin_boards_store::enrich_pins_metadata(&user_pins)
                        .await;
                    metadata_map.set(enriched);
                    pins.set(user_pins);
                }
                Err(e) => {
                    log::error!("Failed to fetch user pins: {}", e);
                }
            }
            pins_loading.set(false);
        });
        spawn(async move {
            match pin_boards_store::fetch_my_pinboards().await {
                Ok(user_boards) => {
                    boards.set(user_boards);
                }
                Err(e) => {
                    log::error!("Failed to fetch user boards: {}", e);
                }
            }
            boards_loading.set(false);
        });
    });
    let filtered_pins: Vec<Pin> = {
        let query = search_query.read().to_lowercase();
        if query.len() < 2 {
            pins.read().clone()
        } else {
            let meta = metadata_map.read();
            pins.read()
                .iter()
                .filter(|p| {
                    p.title.as_ref().is_some_and(|t| t.to_lowercase().contains(&query))
                        || p.content.to_lowercase().contains(&query)
                        || p.tags.iter().any(|t| t.to_lowercase().contains(&query))
                        || meta
                            .get(&p.event_id)
                            .is_some_and(|m| {
                                m
                                    .title
                                    .as_ref()
                                    .is_some_and(|t| t.to_lowercase().contains(&query))
                                    || m
                                        .summary
                                        .as_ref()
                                        .is_some_and(|s| s.to_lowercase().contains(&query))
                            })
                })
                .cloned()
                .collect()
        }
    };
    let filtered_boards: Vec<Pinboard> = {
        let query = search_query.read().to_lowercase();
        if query.len() < 2 {
            boards.read().clone()
        } else {
            boards
                .read()
                .iter()
                .filter(|b| {
                    b.title.to_lowercase().contains(&query)
                        || b
                            .description
                            .as_ref()
                            .is_some_and(|d| d.to_lowercase().contains(&query))
                        || b.tags.iter().any(|t| t.to_lowercase().contains(&query))
                })
                .cloned()
                .collect()
        }
    };
    let pins_count = pins.read().len();
    let boards_count = boards.read().len();
    if !is_authenticated {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center",
                div { class: "text-center space-y-4",
                    div { class: "text-6xl", "📌" }
                    h1 { class: "text-2xl font-bold", "My Pins" }
                    p { class: "text-muted-foreground",
                        "Sign in to view your pinned content and boards"
                    }
                }
            }
        };
    }
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3",
                    div { class: "flex items-center justify-between mb-3",
                        h1 { class: "text-xl font-bold flex items-center gap-2",
                            span { class: "text-2xl", "📌" }
                            "My Pins"
                        }
                        if *HAS_SIGNER.read() {
                            Link {
                                to: Route::PinNew {},
                                class: "px-4 py-2 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition",
                                "+ New Pin"
                            }
                        }
                    }
                    div { class: "relative mb-3",
                        svg {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-muted-foreground",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            circle { cx: "11", cy: "11", r: "8" }
                            line {
                                x1: "21",
                                y1: "21",
                                x2: "16.65",
                                y2: "16.65",
                            }
                        }
                        input {
                            class: "w-full pl-10 pr-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search your pins...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                    }
                    div { class: "flex border-b border-border",
                        button {
                            class: if *active_tab.read() == UserPinsTab::Pins { "px-4 py-2 text-sm font-medium text-foreground border-b-2 border-primary -mb-px" } else { "px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground" },
                            onclick: move |_| active_tab.set(UserPinsTab::Pins),
                            "Pins"
                            span { class: "ml-2 px-2 py-0.5 text-xs rounded-full bg-muted",
                                "{pins_count}"
                            }
                        }
                        button {
                            class: if *active_tab.read() == UserPinsTab::Boards { "px-4 py-2 text-sm font-medium text-foreground border-b-2 border-primary -mb-px" } else { "px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground" },
                            onclick: move |_| active_tab.set(UserPinsTab::Boards),
                            "Boards"
                            span { class: "ml-2 px-2 py-0.5 text-xs rounded-full bg-muted",
                                "{boards_count}"
                            }
                        }
                    }
                }
            }
            div { class: "px-4 py-4",
                match *active_tab.read() {
                    UserPinsTab::Pins => rsx! {
                        if *pins_loading.read() {
                            PinMosaicGrid { pins: vec![], loading: true, skeleton_count: 12 }
                        } else if filtered_pins.is_empty() {
                            div { class: "text-center py-12 text-muted-foreground",
                                if search_query.read().len() >= 2 {
                                    p { "No pins found matching your search." }
                                } else {
                                    p { "You haven't pinned anything yet." }
                                    Link {
                                        to: Route::PinNew {},
                                        class: "inline-block mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                        "Create Your First Pin"
                                    }
                                }
                            }
                        } else {
                            PinMosaicGrid {
                                pins: filtered_pins.clone(),
                                metadata_map: metadata_map.read().clone(),
                                loading: false,
                                on_pin_to_board: move |req: PinToBoardRequest| {
                                    pin_to_board_request.set(Some(req));
                                },
                            }
                        }
                    },
                    UserPinsTab::Boards => rsx! {
                        if *boards_loading.read() {
                            PinBoardMosaicGrid {
                                boards: vec![],
                                loading: true,
                                skeleton_count: 12,
                                has_more: false,
                            }
                        } else if filtered_boards.is_empty() {
                            div { class: "text-center py-12 text-muted-foreground",
                                if search_query.read().len() >= 2 {
                                    p { "No boards found matching your search." }
                                } else {
                                    p { "You haven't created any boards yet." }
                                    Link {
                                        to: Route::PinBoardNew {},
                                        class: "inline-block mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                        "Create Your First Board"
                                    }
                                }
                            }
                        } else {
                            PinBoardMosaicGrid { boards: filtered_boards.clone(), loading: false, has_more: false }
                        }
                    },
                }
            }
        }
        if let Some(ref req) = *pin_to_board_request.read() {
            PinToBoardModal {
                reference: req.reference.clone(),
                content_type: req.content_type.clone(),
                title: req.title.clone(),
                on_close: move |_| pin_to_board_request.set(None),
            }
        }
    }
}
