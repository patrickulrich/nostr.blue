//! Pinboards Explore Page
//! Browse and discover pinboards (Pinterest-style)

use dioxus::prelude::*;
use nostr_sdk::prelude::NostrDatabaseExt;
use nostr_sdk::PublicKey;
use std::time::Duration;

use crate::stores::pin_boards_store::{self, Pinboard};
use crate::stores::nostr_client::{self, get_client, HAS_SIGNER};
use crate::stores::auth_store;
use crate::components::{
    PinBoardMosaicGrid, BoardSlideover, ZapModal,
};
use crate::routes::Route;
use crate::utils::truncate_pubkey;

/// Number of boards to fetch per page
const PAGE_SIZE: usize = 30;

#[component]
pub fn PinBoardsHome() -> Element {
    // User's own boards
    let mut my_boards = use_signal(Vec::<Pinboard>::new);
    let mut my_boards_loading = use_signal(|| true);

    // Discover boards
    let mut discover_boards = use_signal(Vec::<Pinboard>::new);
    let mut discover_loading = use_signal(|| true);

    // Pagination state
    let mut loading_more = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Search state
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(|| None::<Vec<Pinboard>>);
    let mut search_loading = use_signal(|| false);
    let mut search_version = use_signal(|| 0u64);

    // Hashtag filter
    let mut selected_tag = use_signal(|| None::<String>);

    // Slideover state
    let mut selected_board = use_signal(|| None::<Pinboard>);
    let mut show_slideover = use_signal(|| false);

    // Zap modal state
    let mut zap_board = use_signal(|| None::<Pinboard>);
    let mut zap_author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut show_zap_modal = use_signal(|| false);

    // Get current user pubkey
    let current_user_pubkey = auth_store::get_pubkey();

    // Fetch all data on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        // Fetch user's own boards if logged in
        if auth_store::get_pubkey().is_some() {
            spawn(async move {
                match pin_boards_store::fetch_my_pinboards().await {
                    Ok(boards) => {
                        my_boards.set(boards);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch user boards: {}", e);
                    }
                }
                my_boards_loading.set(false);
            });
        } else {
            my_boards_loading.set(false);
        }

        // Fetch discover boards
        spawn(async move {
            match pin_boards_store::fetch_pinboards(PAGE_SIZE).await {
                Ok(boards) => {
                    // Track oldest timestamp for pagination
                    if let Some(oldest) = boards.last() {
                        oldest_timestamp.set(Some(oldest.created_at));
                    }

                    // Check if we have more to load
                    has_more.set(boards.len() >= PAGE_SIZE);

                    discover_boards.set(boards);
                }
                Err(e) => {
                    log::error!("Failed to fetch pinboards: {}", e);
                }
            }
            discover_loading.set(false);
        });
    });

    // Load more handler
    let handle_load_more = EventHandler::new(move |_: ()| {
        if *loading_more.read() || !*has_more.read() {
            return;
        }

        let until = *oldest_timestamp.read();
        loading_more.set(true);

        spawn(async move {
            match pin_boards_store::fetch_pinboards_page(PAGE_SIZE, until).await {
                Ok(new_boards) => {
                    // Track oldest timestamp for next page
                    if let Some(oldest) = new_boards.last() {
                        oldest_timestamp.set(Some(oldest.created_at));
                    }

                    // Check if we have more to load
                    has_more.set(new_boards.len() >= PAGE_SIZE);

                    // Append to existing boards
                    let mut current = discover_boards.read().clone();
                    current.extend(new_boards);
                    discover_boards.set(current);
                }
                Err(e) => {
                    log::error!("Failed to load more pinboards: {}", e);
                }
            }
            loading_more.set(false);
        });
    });

    // Debounced search effect
    use_effect(move || {
        let query = search_query.read().clone();

        if query.len() < 2 {
            search_results.set(None);
            search_loading.set(false);
            return;
        }

        let version = search_version.with_mut(|v| { *v += 1; *v });
        search_loading.set(true);

        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(300).await;

            if *search_version.peek() != version {
                return;
            }

            // Simple client-side search in discover boards
            let boards = discover_boards.read().clone();
            let query_lower = query.to_lowercase();
            let filtered: Vec<Pinboard> = boards
                .into_iter()
                .filter(|b| {
                    b.title.to_lowercase().contains(&query_lower) ||
                    b.description.as_ref().is_some_and(|d| d.to_lowercase().contains(&query_lower)) ||
                    b.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
                })
                .collect();

            if *search_version.peek() == version {
                search_results.set(Some(filtered));
                search_loading.set(false);
            }
        });
    });

    let is_searching = search_query.read().len() >= 2;
    let is_logged_in = current_user_pubkey.is_some();

    // Filter boards by hashtag
    let filtered_boards: Vec<Pinboard> = {
        let boards = if let Some(ref results) = *search_results.read() {
            results.clone()
        } else {
            discover_boards.read().clone()
        };

        if let Some(ref tag) = *selected_tag.read() {
            boards.into_iter()
                .filter(|b| b.tags.iter().any(|t| t.to_lowercase() == tag.to_lowercase()))
                .collect()
        } else {
            boards
        }
    };

    // Collect all unique tags from boards for filter
    let all_tags: Vec<String> = {
        let mut tags: Vec<String> = discover_boards.read()
            .iter()
            .flat_map(|b| b.tags.clone())
            .collect();
        tags.sort();
        tags.dedup();
        tags.into_iter().take(20).collect() // Limit to 20 most common
    };

    // Pre-compute zap modal data (must be before rsx!)
    #[allow(clippy::type_complexity)]
    let zap_modal_data: Option<(String, String, Option<String>, Option<String>, String)> = {
        if *show_zap_modal.read() {
            if let Some(ref board) = *zap_board.read() {
                let metadata_opt = zap_author_metadata.read().clone();
                let author_name = metadata_opt.as_ref()
                    .and_then(|m| m.display_name.clone().or(m.name.clone()))
                    .unwrap_or_else(|| truncate_pubkey(&board.pubkey));
                let lud16 = metadata_opt.as_ref().and_then(|m| m.lud16.clone());
                let lud06 = metadata_opt.as_ref().and_then(|m| m.lud06.clone());
                Some((board.pubkey.clone(), author_name, lud16, lud06, board.event_id.clone()))
            } else {
                None
            }
        } else {
            None
        }
    };

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3",
                    div {
                        class: "flex items-center justify-between mb-3",
                        h1 {
                            class: "text-xl font-bold flex items-center gap-2",
                            span { class: "text-2xl", "📌" }
                            "Pinboards"
                        }
                        // Create Board button (only if signed in)
                        if *HAS_SIGNER.read() {
                            Link {
                                to: Route::PinBoardNew {},
                                class: "px-4 py-2 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition",
                                "+ New Board"
                            }
                        }
                    }
                    p {
                        class: "text-sm text-muted-foreground mb-3",
                        "Collect and organize your favorite Nostr content"
                    }

                    // Search row
                    div {
                        class: "flex gap-2",

                        // Search
                        div {
                            class: "relative flex-1",
                            svg {
                                class: "absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-muted-foreground",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                circle { cx: "11", cy: "11", r: "8" }
                                line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                            }
                            input {
                                class: "w-full pl-10 pr-4 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "Search boards...",
                                value: "{search_query}",
                                oninput: move |evt| search_query.set(evt.value())
                            }
                            if *search_loading.read() {
                                div {
                                    class: "absolute right-3 top-1/2 -translate-y-1/2",
                                    span {
                                        class: "inline-block w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin"
                                    }
                                }
                            }
                        }
                    }

                    // Tag filter chips
                    if !all_tags.is_empty() {
                        div {
                            class: "flex flex-wrap gap-2 mt-3",
                            // "All" chip
                            button {
                                class: if selected_tag.read().is_none() {
                                    "px-3 py-1 text-sm rounded-full bg-primary text-primary-foreground"
                                } else {
                                    "px-3 py-1 text-sm rounded-full bg-muted hover:bg-accent"
                                },
                                onclick: move |_| selected_tag.set(None),
                                "All"
                            }
                            // Tag chips
                            for tag in all_tags.iter() {
                                {
                                    let tag_clone = tag.clone();
                                    let tag_for_check = tag.clone();
                                    let is_selected = selected_tag.read().as_ref() == Some(&tag_for_check);
                                    rsx! {
                                        button {
                                            key: "{tag}",
                                            class: if is_selected {
                                                "px-3 py-1 text-sm rounded-full bg-primary text-primary-foreground"
                                            } else {
                                                "px-3 py-1 text-sm rounded-full bg-muted hover:bg-accent"
                                            },
                                            onclick: move |_| {
                                                selected_tag.set(Some(tag_clone.clone()));
                                            },
                                            "#{tag}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Main content
            div {
                class: "px-4 py-4 space-y-8",

                // My Boards section (if logged in)
                if is_logged_in {
                    section {
                        div {
                            class: "flex items-center justify-between mb-4",
                            h2 {
                                class: "text-lg font-bold",
                                "My Boards"
                            }
                            Link {
                                to: Route::PinBoardNew {},
                                class: "text-sm text-primary hover:underline",
                                "Create new"
                            }
                        }

                        if *my_boards_loading.read() {
                            // Use masonry skeleton layout
                            PinBoardMosaicGrid {
                                boards: vec![],
                                loading: true,
                                skeleton_count: 6,
                                has_more: false,
                            }
                        } else if my_boards.read().is_empty() {
                            div {
                                class: "text-center py-8 text-muted-foreground",
                                p { "You haven't created any boards yet." }
                                Link {
                                    to: Route::PinBoardNew {},
                                    class: "inline-block mt-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                    "Create Your First Board"
                                }
                            }
                        } else {
                            PinBoardMosaicGrid {
                                boards: my_boards.read().clone(),
                                on_board_click: move |board: Pinboard| {
                                    selected_board.set(Some(board));
                                    show_slideover.set(true);
                                },
                                on_zap_request: move |board: Pinboard| {
                                    let pubkey_str = board.pubkey.clone();
                                    zap_board.set(Some(board));
                                    show_zap_modal.set(true);
                                    // Fetch author metadata for zap modal
                                    spawn(async move {
                                        if let Ok(pubkey) = PublicKey::from_hex(&pubkey_str) {
                                            if let Some(client) = get_client() {
                                                if let Ok(Some(metadata)) = client.database().metadata(pubkey).await {
                                                    zap_author_metadata.set(Some(metadata));
                                                } else if let Ok(Some(metadata)) = client.fetch_metadata(pubkey, Duration::from_secs(5)).await {
                                                    zap_author_metadata.set(Some(metadata));
                                                }
                                            }
                                        }
                                    });
                                },
                                loading: false,
                                has_more: false, // No pagination for personal boards
                            }
                        }
                    }

                    // Divider
                    hr { class: "border-border" }
                }

                // Discover section
                section {
                    h2 {
                        class: "text-lg font-bold mb-4",
                        if is_searching {
                            "Search Results"
                        } else if selected_tag.read().is_some() {
                            "Filtered Boards"
                        } else {
                            "Discover Boards"
                        }
                    }

                    if *discover_loading.read() {
                        // Use masonry skeleton layout
                        PinBoardMosaicGrid {
                            boards: vec![],
                            loading: true,
                            skeleton_count: 12,
                            has_more: false,
                        }
                    } else if filtered_boards.is_empty() {
                        div {
                            class: "text-center py-12 text-muted-foreground",
                            if is_searching {
                                p { "No boards found matching your search." }
                            } else {
                                p { "No public boards available yet." }
                                p { class: "text-sm mt-1", "Be the first to create one!" }
                            }
                        }
                    } else {
                        PinBoardMosaicGrid {
                            boards: filtered_boards.clone(),
                            on_board_click: move |board: Pinboard| {
                                selected_board.set(Some(board));
                                show_slideover.set(true);
                            },
                            on_zap_request: move |board: Pinboard| {
                                let pubkey_str = board.pubkey.clone();
                                zap_board.set(Some(board));
                                show_zap_modal.set(true);
                                // Fetch author metadata for zap modal
                                spawn(async move {
                                    if let Ok(pubkey) = PublicKey::from_hex(&pubkey_str) {
                                        if let Some(client) = get_client() {
                                            if let Ok(Some(metadata)) = client.database().metadata(pubkey).await {
                                                zap_author_metadata.set(Some(metadata));
                                            } else if let Ok(Some(metadata)) = client.fetch_metadata(pubkey, Duration::from_secs(5)).await {
                                                zap_author_metadata.set(Some(metadata));
                                            }
                                        }
                                    }
                                });
                            },
                            loading: false,
                            // Only show load more when not searching
                            on_load_more: Some(handle_load_more),
                            has_more: !is_searching && selected_tag.read().is_none() && *has_more.read(),
                            loading_more: *loading_more.read(),
                        }
                    }
                }
            }
        }

        // Board slideover modal
        if let Some(ref board) = *selected_board.read() {
            BoardSlideover {
                board: board.clone(),
                show: show_slideover,
                on_close: move |_| {
                    show_slideover.set(false);
                },
            }
        }

        // Zap modal
        if let Some((recipient_pubkey, recipient_name, lud16, lud06, event_id)) = zap_modal_data {
            ZapModal {
                recipient_pubkey: recipient_pubkey,
                recipient_name: recipient_name,
                lud16: lud16,
                lud06: lud06,
                event_id: Some(event_id),
                on_close: move |_| {
                    show_zap_modal.set(false);
                    zap_board.set(None);
                    zap_author_metadata.set(None);
                },
            }
        }
    }
}
