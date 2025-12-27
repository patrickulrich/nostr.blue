//! Board Slideover Modal Component
//! Pinterest-style right-sliding panel for viewing board details
//! Uses two-stage loading: board metadata first, then pins

use dioxus::prelude::*;

use crate::hooks::use_author_metadata;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::pin_boards_store::{Pinboard, Pin, delete_pinboard, fetch_pins_for_board_filtered, delete_pin};
use crate::stores::auth_store;
use crate::utils::truncate_pubkey;
use crate::components::pin_board_card::HashtagBadge;
use crate::components::pin_board_item_card::PinCard;
use crate::components::{ZapModal, ShareModal, ConfirmModal};
use crate::components::icons::PinIcon;
use crate::routes::Route;

// ============================================================================
// Board Slideover Component
// ============================================================================

/// Right-side sliding panel for viewing board details
/// Opens over the current page, can be closed to return
#[component]
pub fn BoardSlideover(
    board: Pinboard,
    show: Signal<bool>,
    on_close: EventHandler<()>,
) -> Element {
    let title = board.title.clone();
    let description = board.description.clone();
    let cover_image = board.image.clone();
    let tags = board.tags.clone();
    let naddr = board.naddr.clone();
    let author_pubkey = board.pubkey.clone();
    let board_for_delete = board.clone();
    let board_a_tag = board.a_tag.clone();

    // State for pins (fetched separately)
    let mut pins = use_signal(Vec::<Pin>::new);
    let mut pins_loading = use_signal(|| true);

    // Author profile metadata - uses shared hook for database-first, network-fallback pattern
    let author_metadata = use_author_metadata(author_pubkey.clone());

    // Modals
    let mut show_zap_modal = use_signal(|| false);
    let mut show_share_modal = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut pending_pin_removal = use_signal(|| None::<Pin>);

    // Check if current user owns this board - reactive to auth state changes
    let author_pubkey_for_owner = author_pubkey.clone();
    let is_owner = use_memo(move || {
        auth_store::get_pubkey()
            .map(|pk| pk == author_pubkey_for_owner)
            .unwrap_or(false)
    });

    // Memoize derived display values to avoid recalculating on every render
    let display_name = use_memo(move || {
        author_metadata.read().as_ref()
            .and_then(|m| m.display_name.clone().or(m.name.clone()))
            .unwrap_or_else(|| truncate_pubkey(&author_pubkey))
    });

    let profile_picture = use_memo(move || {
        author_metadata.read().as_ref()
            .and_then(|m| m.picture.clone())
    });

    let avatar_letter = use_memo(move || {
        display_name().chars().next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string()
    });

    // Get collaborative status for pin fetching
    let is_collaborative = board.collaborative;
    let owner_pubkey_for_pins = board.pubkey.clone();

    // Fetch pins when slideover opens
    // Re-run when show, board, or collaborative status changes
    let show_signal = show;
    use_effect(use_reactive!(|(show_signal, board_a_tag, owner_pubkey_for_pins, is_collaborative)| {
        let shown = *show_signal.read();
        if !shown {
            return;
        }

        let a_tag = board_a_tag.clone();
        let owner_pk = owner_pubkey_for_pins.clone();
        pins_loading.set(true);

        spawn(async move {
            // If collaborative, fetch all pins; otherwise, only owner's pins
            let result = if is_collaborative {
                fetch_pins_for_board_filtered(&a_tag, None, None).await
            } else {
                fetch_pins_for_board_filtered(&a_tag, Some(&owner_pk), None).await
            };

            match result {
                Ok(fetched_pins) => {
                    pins.set(fetched_pins);
                }
                Err(e) => {
                    log::error!("Failed to fetch pins: {}", e);
                }
            }
            pins_loading.set(false);
        });
    }));

    // Handle board delete
    let nav = navigator();
    let on_close_for_delete = on_close;
    let handle_delete = move |_| {
        deleting.set(true);
        let board_clone = board_for_delete.clone();
        let on_close = on_close_for_delete;

        spawn(async move {
            match delete_pinboard(&board_clone).await {
                Ok(_) => {
                    on_close.call(());
                    nav.push(Route::PinBoardsHome {});
                }
                Err(e) => {
                    log::error!("Failed to delete board: {}", e);
                    deleting.set(false);
                    show_delete_confirm.set(false);
                }
            }
        });
    };

    // Handle pin removal - show confirmation modal
    let handle_pin_remove = move |pin: Pin| {
        pending_pin_removal.set(Some(pin));
    };

    // Handle confirmed pin removal
    let handle_pin_remove_confirmed = move |_| {
        if let Some(pin) = pending_pin_removal.take() {
            let event_id = pin.event_id.clone();
            spawn(async move {
                match delete_pin(&event_id).await {
                    Ok(_) => {
                        // Remove from local state
                        pins.write().retain(|p| p.event_id != event_id);
                    }
                    Err(e) => {
                        log::error!("Failed to delete pin: {}", e);
                    }
                }
            });
        }
    };

    // Don't render if not shown (early return before expensive signal reads)
    if !*show.read() {
        return rsx! {};
    }

    let pin_count = pins.read().len();

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 z-40 bg-black/50 backdrop-blur-sm transition-opacity duration-200",
            onclick: move |_| on_close.call(()),
        }

        // Sliding panel
        div {
            class: "fixed inset-y-0 right-0 z-50 w-full max-w-5xl bg-background shadow-2xl
                    transform transition-transform duration-500 ease-in-out overflow-hidden",
            onclick: move |e| e.stop_propagation(),

            // Scrollable content
            div {
                class: "h-full overflow-y-auto",

                // Header with close button
                div {
                    class: "sticky top-0 z-10 bg-background/95 backdrop-blur-sm border-b border-border",
                    div {
                        class: "px-4 py-3 flex items-center justify-between",

                        // Title
                        h2 {
                            class: "text-lg font-bold truncate",
                            "{title}"
                        }

                        // Actions
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

                            // Zap button
                            if *HAS_SIGNER.read() {
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
                                    to: Route::PinBoardEdit { naddr: naddr.clone() },
                                    class: "p-2 rounded-lg hover:bg-muted transition",
                                    title: "Edit",
                                    svg {
                                        class: "w-5 h-5",
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
                                }

                                // Delete button
                                button {
                                    class: "p-2 rounded-lg hover:bg-red-100 dark:hover:bg-red-900/20 text-red-500 transition",
                                    onclick: move |_| show_delete_confirm.set(true),
                                    title: "Delete",
                                    svg {
                                        class: "w-5 h-5",
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
                                }
                            }

                            // Close button
                            button {
                                class: "p-2 rounded-lg hover:bg-muted transition ml-2",
                                onclick: move |_| on_close.call(()),
                                title: "Close",
                                svg {
                                    class: "w-6 h-6",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M6 18L18 6M6 6l12 12"
                                    }
                                }
                            }
                        }
                    }
                }

                // Cover image
                if let Some(ref img_url) = cover_image {
                    div {
                        class: "relative w-full h-48 md:h-64 overflow-hidden",
                        img {
                            src: "{img_url}",
                            alt: "{title}",
                            class: "w-full h-full object-cover"
                        }
                        // Gradient overlay
                        div {
                            class: "absolute inset-0 bg-gradient-to-t from-black/40 to-transparent"
                        }
                    }
                }

                // Board info
                div {
                    class: "px-4 py-4 border-b border-border",

                    // Author row
                    div {
                        class: "flex items-center gap-3 mb-3",

                        // Avatar
                        div {
                            class: "w-10 h-10 rounded-full overflow-hidden bg-muted flex items-center justify-center flex-shrink-0",
                            if let Some(ref pic_url) = profile_picture() {
                                img {
                                    src: "{pic_url}",
                                    alt: "{display_name()}",
                                    class: "w-full h-full object-cover",
                                    loading: "lazy",
                                }
                            } else {
                                span {
                                    class: "text-lg font-semibold text-muted-foreground",
                                    "{avatar_letter()}"
                                }
                            }
                        }

                        // Name and meta
                        div {
                            class: "flex-1",
                            span {
                                class: "font-medium",
                                "{display_name()}"
                            }
                            div {
                                class: "flex items-center gap-2 text-sm text-muted-foreground",
                                if *pins_loading.read() {
                                    span { "Loading pins..." }
                                } else {
                                    span {
                                        {if pin_count == 1 { "1 pin".to_string() } else { format!("{} pins", pin_count) }}
                                    }
                                }
                            }
                        }
                    }

                    // Tags
                    if !tags.is_empty() {
                        div {
                            class: "flex flex-wrap items-center gap-2 mb-3",
                            for tag in tags.iter() {
                                HashtagBadge { key: "{tag}", tag: tag.clone() }
                            }
                        }
                    }

                    // Description
                    if let Some(ref desc) = description {
                        p {
                            class: "text-muted-foreground",
                            "{desc}"
                        }
                    }
                }

                // Pins grid
                div {
                    class: "p-4",

                    if *pins_loading.read() {
                        // Loading skeleton
                        div {
                            class: "grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 auto-rows-auto",
                            for i in 0..6 {
                                div {
                                    key: "skeleton-{i}",
                                    class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse",
                                    div { class: "w-full aspect-video bg-muted" }
                                    div {
                                        class: "p-3 space-y-2",
                                        div { class: "h-4 bg-muted rounded w-full" }
                                        div { class: "h-3 bg-muted rounded w-3/4" }
                                    }
                                }
                            }
                        }
                    } else if pins.read().is_empty() {
                        div {
                            class: "text-center py-12 text-muted-foreground",
                            PinIcon {
                                class: "w-12 h-12 mx-auto mb-4 text-muted-foreground/50".to_string(),
                                filled: false
                            }
                            p { "This board is empty." }
                            if *is_owner.read() {
                                p { class: "text-sm mt-2", "Add items from recipes, notes, communities, and more!" }
                            }
                        }
                    } else {
                        div {
                            class: "grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 auto-rows-auto",
                            for pin in pins.read().iter() {
                                PinCard {
                                    key: "{pin.event_id}",
                                    pin: pin.clone(),
                                    show_remove: *is_owner.read(),
                                    on_remove: handle_pin_remove,
                                }
                            }
                        }
                    }
                }
            }
        }

        // Modals
        if *show_share_modal.read() {
            ShareModal {
                event: board.event.clone(),
                on_close: move |_| show_share_modal.set(false),
            }
        }

        if *show_zap_modal.read() {
            ZapModal {
                recipient_pubkey: board.pubkey.clone(),
                recipient_name: display_name(),
                lud16: author_metadata.read().as_ref().and_then(|m| m.lud16.clone()),
                lud06: author_metadata.read().as_ref().and_then(|m| m.lud06.clone()),
                event_id: Some(board.event_id.clone()),
                on_close: move |_| show_zap_modal.set(false),
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

        if pending_pin_removal.read().is_some() {
            ConfirmModal {
                title: "Remove Pin".to_string(),
                message: "Remove this pin from the board?".to_string(),
                confirm_text: Some("Remove".to_string()),
                cancel_text: Some("Cancel".to_string()),
                on_confirm: handle_pin_remove_confirmed,
                on_cancel: move |_| pending_pin_removal.set(None),
            }
        }
    }
}
