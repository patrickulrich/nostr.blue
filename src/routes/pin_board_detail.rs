//! Pin Board Detail Page
//! View a pin board with its pins, engagement, and actions
//! Uses two-stage loading: board metadata first, then pins

use crate::components::pin_board_item_selector::PinToBoardModal;
use crate::components::{
    ConfirmModal, HashtagBadge, PinCardMosaicSkeleton, PinMosaicGrid, PinToBoardRequest,
    ShareModal, ZapModal,
};
use crate::routes::Route;
use crate::stores::auth_store;
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::pin_boards_store::{
    self, delete_pinboard, enrich_pins_metadata, fetch_pinboard_reaction_count,
    fetch_pinboard_zap_total, fetch_pins_for_board_filtered, has_user_reacted_to_pinboard,
    toggle_pinboard_reaction, Pin, PinMetadata, Pinboard,
};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use std::collections::HashMap;

#[component]
pub fn PinBoardDetail(naddr: String) -> Element {
    // Clone naddr for use in multiple closures
    let naddr_for_effect = naddr.clone();
    let naddr_for_edit = naddr.clone();

    let mut board = use_signal(|| None::<Pinboard>);
    let mut pins = use_signal(Vec::<Pin>::new);
    let mut loading = use_signal(|| true);
    let mut pins_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Enriched metadata for pins (includes content type, title, image, summary)
    let mut pin_metadata = use_signal(HashMap::<String, PinMetadata>::new);

    // Engagement state
    let mut reaction_count = use_signal(|| 0usize);
    let mut zap_total_msats = use_signal(|| 0u64);
    let mut has_reacted = use_signal(|| false);
    let mut reaction_loading = use_signal(|| false);

    // Modals
    let mut show_zap_modal = use_signal(|| false);
    let mut show_share_modal = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    // Pin to board modal (lifted from PinCard to avoid overflow clipping)
    let mut pin_to_board_request: Signal<Option<PinToBoardRequest>> = use_signal(|| None);

    // Check if current user owns this board
    let is_owner = use_memo(move || {
        if let Some(ref b) = *board.read() {
            if let Some(pubkey) = auth_store::get_pubkey() {
                return b.pubkey == pubkey;
            }
        }
        false
    });

    // Stage 1: Fetch board metadata on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        let naddr_clone = naddr_for_effect.clone();
        loading.set(true);
        error.set(None);

        spawn(async move {
            match pin_boards_store::fetch_pinboard_by_naddr(&naddr_clone).await {
                Ok(Some(b)) => {
                    board.set(Some(b));
                    loading.set(false);
                }
                Ok(None) => {
                    error.set(Some("Board not found".to_string()));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Stage 2: Fetch pins when board is loaded
    use_effect(move || {
        let board_ref = board.read();
        if let Some(ref b) = *board_ref {
            let a_tag = b.a_tag.clone();
            let a_tag_for_engagement = a_tag.clone();
            let owner_pubkey = b.pubkey.clone();
            let is_collaborative = b.collaborative;
            pins_loading.set(true);

            spawn(async move {
                // If collaborative, fetch all pins; otherwise, only owner's pins
                let result = if is_collaborative {
                    fetch_pins_for_board_filtered(&a_tag, None, None).await
                } else {
                    fetch_pins_for_board_filtered(&a_tag, Some(&owner_pubkey), None).await
                };

                match result {
                    Ok(fetched_pins) => {
                        // Enrich pins with metadata from referenced events (image, title, content type)
                        let metadata = enrich_pins_metadata(&fetched_pins).await;
                        pin_metadata.set(metadata);

                        pins.set(fetched_pins);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch pins: {}", e);
                    }
                }
                pins_loading.set(false);
            });

            // Also fetch engagement data
            spawn(async move {
                // Fetch reaction count
                if let Ok(count) = fetch_pinboard_reaction_count(&a_tag_for_engagement).await {
                    reaction_count.set(count);
                }
                // Fetch zap total
                if let Ok(total) = fetch_pinboard_zap_total(&a_tag_for_engagement).await {
                    zap_total_msats.set(total);
                }
                // Check if user has reacted (only if signed in)
                if *HAS_SIGNER.read() {
                    if let Ok(reacted) = has_user_reacted_to_pinboard(&a_tag_for_engagement).await {
                        has_reacted.set(reacted);
                    }
                }
            });
        }
    });

    // Handle pin deletion (called after successful delete from PinMenu)
    let handle_pin_deleted = move |event_id: String| {
        // Remove from local state
        pins.write().retain(|p| p.event_id != event_id);
        // Also clean up stale metadata
        pin_metadata.write().remove(&event_id);
    };

    // Handle board deletion
    let nav = navigator();
    let handle_delete = move |_| {
        let board_ref = board.read();
        if let Some(ref b) = *board_ref {
            let board_clone = b.clone();
            drop(board_ref);
            deleting.set(true);

            let nav = nav;
            spawn(async move {
                match delete_pinboard(&board_clone).await {
                    Ok(_) => {
                        nav.push(Route::PinBoardsHome {});
                    }
                    Err(e) => {
                        log::error!("Failed to delete board: {}", e);
                        deleting.set(false);
                        show_delete_confirm.set(false);
                    }
                }
            });
        }
    };

    // Handle reaction toggle
    let handle_toggle_reaction = move |_| {
        if !*HAS_SIGNER.read() {
            log::warn!("Cannot react: no signer");
            return;
        }
        let board_ref = board.read();
        if let Some(ref b) = *board_ref {
            let board_clone = b.clone();
            let currently_reacted = *has_reacted.peek();
            let current_count = *reaction_count.peek();
            drop(board_ref);

            // Optimistic update
            has_reacted.set(!currently_reacted);
            if currently_reacted {
                reaction_count.set(current_count.saturating_sub(1));
            } else {
                reaction_count.set(current_count + 1);
            }
            reaction_loading.set(true);

            spawn(async move {
                match toggle_pinboard_reaction(&board_clone, "+").await {
                    Ok(_) => {
                        // State already updated optimistically
                    }
                    Err(e) => {
                        log::error!("Failed to toggle reaction: {}", e);
                        // Rollback on error
                        has_reacted.set(currently_reacted);
                        reaction_count.set(current_count);
                    }
                }
                reaction_loading.set(false);
            });
        }
    };

    // Pin count
    let pin_count = pins.read().len();

    rsx! {
        div {
            class: "min-h-screen",

            // Header with back button and actions
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center justify-between",

                    // Back button
                    Link {
                        to: Route::PinBoardsHome {},
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition",
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7"
                            }
                        }
                        "Pinboards"
                    }

                    // Actions
                    if board.read().is_some() {
                        div {
                            class: "flex items-center gap-2",

                            // Share button
                            button {
                                class: "p-2 rounded-lg hover:bg-muted transition",
                                onclick: move |_| show_share_modal.set(true),
                                title: "Share",
                                svg {
                                    class: "w-5 h-5",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.367 2.684 3 3 0 00-5.367-2.684z"
                                    }
                                }
                            }

                            // Reaction button
                            if *HAS_SIGNER.read() {
                                {
                                    let is_reacted = *has_reacted.read();
                                    let is_loading = *reaction_loading.read();
                                    let heart_class = if is_reacted {
                                        "p-2 rounded-lg hover:bg-muted transition text-red-500"
                                    } else {
                                        "p-2 rounded-lg hover:bg-muted transition text-muted-foreground hover:text-red-500"
                                    };
                                    rsx! {
                                        button {
                                            class: "{heart_class}",
                                            disabled: is_loading,
                                            onclick: handle_toggle_reaction,
                                            title: if is_reacted { "Unlike" } else { "Like" },
                                            svg {
                                                class: "w-5 h-5",
                                                fill: if is_reacted { "currentColor" } else { "none" },
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                view_box: "0 0 24 24",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Zap button
                                button {
                                    class: "p-2 rounded-lg hover:bg-muted transition text-amber-500",
                                    onclick: move |_| show_zap_modal.set(true),
                                    title: "Zap",
                                    svg {
                                        class: "w-5 h-5",
                                        fill: "currentColor",
                                        view_box: "0 0 24 24",
                                        path { d: "M13 10V3L4 14h7v7l9-11h-7z" }
                                    }
                                }
                            }

                            // Edit button (if owner)
                            if *is_owner.read() {
                                Link {
                                    to: Route::PinBoardEdit { naddr: naddr_for_edit.clone() },
                                    class: "px-3 py-1.5 rounded-lg hover:bg-muted transition text-sm font-medium flex items-center gap-1",
                                    svg {
                                        class: "w-4 h-4",
                                        fill: "none",
                                        stroke: "currentColor",
                                        view_box: "0 0 24 24",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            stroke_width: "2",
                                            d: "M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                                        }
                                    }
                                    "Edit"
                                }

                                // Delete button
                                button {
                                    class: "px-3 py-1.5 rounded-lg hover:bg-red-100 dark:hover:bg-red-900/20 text-red-500 transition text-sm font-medium flex items-center gap-1",
                                    onclick: move |_| show_delete_confirm.set(true),
                                    svg {
                                        class: "w-4 h-4",
                                        fill: "none",
                                        stroke: "currentColor",
                                        view_box: "0 0 24 24",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            stroke_width: "2",
                                            d: "M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                                        }
                                    }
                                    "Delete"
                                }
                            }
                        }
                    }
                }
            }

            // Main content
            div {
                class: "px-4 py-4",

                if *loading.read() {
                    // Loading state
                    BoardDetailSkeleton {}
                } else if let Some(ref err) = *error.read() {
                    // Error state
                    div {
                        class: "text-center py-12",
                        p { class: "text-red-500 mb-4", "{err}" }
                        Link {
                            to: Route::PinBoardsHome {},
                            class: "text-primary hover:underline",
                            "Back to Pinboards"
                        }
                    }
                } else if let Some(ref b) = *board.read() {
                    // Board header
                    div {
                        class: "mb-6",

                        // Cover image
                        if let Some(ref img_url) = b.image {
                            div {
                                class: "relative w-full h-48 md:h-64 rounded-xl overflow-hidden mb-4",
                                img {
                                    src: "{img_url}",
                                    alt: "{b.title}",
                                    class: "w-full h-full object-cover"
                                }
                                // Gradient overlay
                                div {
                                    class: "absolute inset-0 bg-gradient-to-t from-black/40 to-transparent"
                                }
                            }
                        }

                        // Title and meta
                        div {
                            class: "flex flex-wrap items-start justify-between gap-4",
                            div {
                                h1 {
                                    class: "text-2xl font-bold mb-2",
                                    "{b.title}"
                                }
                                div {
                                    class: "flex flex-wrap items-center gap-2 text-sm",
                                    // Show hashtag badges
                                    for tag in b.tags.iter().take(3) {
                                        HashtagBadge { key: "{tag}", tag: tag.clone() }
                                    }
                                    span {
                                        class: "text-muted-foreground",
                                        if *pins_loading.read() {
                                            "Loading pins..."
                                        } else {
                                            {format!("{} pin{}", pin_count, if pin_count == 1 { "" } else { "s" })}
                                        }
                                    }
                                    // Engagement stats
                                    {
                                        let reactions = *reaction_count.read();
                                        let zap_sats = *zap_total_msats.read() / 1000;
                                        let zap_str = if zap_sats >= 1_000_000 {
                                            format!("{:.1}M", zap_sats as f64 / 1_000_000.0)
                                        } else if zap_sats >= 1_000 {
                                            format!("{:.1}k", zap_sats as f64 / 1_000.0)
                                        } else {
                                            format!("{}", zap_sats)
                                        };

                                        rsx! {
                                            if reactions > 0 {
                                                span {
                                                    class: "text-muted-foreground flex items-center gap-1",
                                                    svg {
                                                        class: "w-4 h-4 text-red-500",
                                                        fill: "currentColor",
                                                        view_box: "0 0 24 24",
                                                        path { d: "M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z" }
                                                    }
                                                    "{reactions}"
                                                }
                                            }
                                            if zap_sats > 0 {
                                                span {
                                                    class: "text-muted-foreground flex items-center gap-1",
                                                    svg {
                                                        class: "w-4 h-4 text-amber-500",
                                                        fill: "currentColor",
                                                        view_box: "0 0 24 24",
                                                        path { d: "M13 10V3L4 14h7v7l9-11h-7z" }
                                                    }
                                                    "{zap_str}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Description
                        if let Some(ref desc) = b.description {
                            p {
                                class: "mt-4 text-muted-foreground",
                                "{desc}"
                            }
                        }
                    }

                    // Pins grid (masonry layout)
                    if *pins_loading.read() {
                        PinMosaicGrid {
                            pins: vec![],
                            loading: true,
                            skeleton_count: 8,
                        }
                    } else if pins.read().is_empty() {
                        div {
                            class: "text-center py-12 text-muted-foreground",
                            p { "This board is empty." }
                            if *is_owner.read() {
                                p { class: "text-sm mt-2", "Add items from recipes, notes, communities, and more!" }
                            }
                        }
                    } else {
                        PinMosaicGrid {
                            pins: pins.read().clone(),
                            is_owner: *is_owner.read(),
                            on_delete: handle_pin_deleted,
                            metadata_map: pin_metadata.read().clone(),
                            on_pin_to_board: move |req: PinToBoardRequest| {
                                pin_to_board_request.set(Some(req));
                            },
                        }
                    }
                }
            }
        }

        // Modals
        if *show_share_modal.read() {
            if let Some(ref b) = *board.read() {
                ShareModal {
                    event: b.event.clone(),
                    on_close: move |_| show_share_modal.set(false),
                }
            }
        }

        if *show_zap_modal.read() {
            if let Some(ref b) = *board.read() {
                ZapModal {
                    recipient_pubkey: b.pubkey.clone(),
                    recipient_name: truncate_pubkey(&b.pubkey),
                    lud16: None,
                    lud06: None,
                    event_id: Some(b.event_id.clone()),
                    on_close: move |_| show_zap_modal.set(false),
                }
            }
        }

        if *show_delete_confirm.read() {
            ConfirmModal {
                title: "Delete Board".to_string(),
                message: "Are you sure you want to delete this board? This action cannot be undone. Note: Existing pins will become orphaned.".to_string(),
                confirm_text: Some("Delete".to_string()),
                cancel_text: Some("Cancel".to_string()),
                on_confirm: handle_delete,
                on_cancel: move |_| show_delete_confirm.set(false),
            }
        }

        // Pin to Board Modal (lifted from PinCard to avoid overflow clipping)
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

/// Skeleton loader for board detail
#[component]
fn BoardDetailSkeleton() -> Element {
    rsx! {
        div {
            class: "animate-pulse",

            // Cover skeleton
            div { class: "w-full h-48 md:h-64 rounded-xl bg-muted mb-4" }

            // Title skeleton
            div { class: "h-8 w-64 bg-muted rounded mb-2" }

            // Meta skeleton
            div { class: "flex gap-2 mb-4" }
            div { class: "h-5 w-20 bg-muted rounded-full" }
            div { class: "h-5 w-16 bg-muted rounded-full" }

            // Description skeleton
            div { class: "space-y-2 mb-6" }
            div { class: "h-4 w-full bg-muted rounded" }
            div { class: "h-4 w-3/4 bg-muted rounded" }

            // Pins grid skeleton (masonry style)
            div {
                class: "columns-1 sm:columns-2 md:columns-3 lg:columns-4 xl:columns-5 gap-3",
                for i in 0..8 {
                    PinCardMosaicSkeleton { key: "skeleton-{i}" }
                }
            }
        }
    }
}
