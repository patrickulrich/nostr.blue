//! Pinboard Card Component
//! Displays a pinboard in a card format for listing pages
//! Image-focused card with cover image gradient, title, and author
use crate::components::icons::PinIcon;
use crate::hooks::use_author_metadata;
use crate::routes::Route;
use crate::stores::auth_store;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::pin_boards_store::{
    fetch_pinboard_reaction_state, toggle_pinboard_reaction, Pinboard,
};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
/// Hashtag badge for displaying board tags
#[component]
pub fn HashtagBadge(tag: String) -> Element {
    rsx! {
        span { class: "px-2 py-0.5 text-xs font-medium text-primary bg-primary/10 rounded-full",
            "#{tag}"
        }
    }
}
/// Pinboard card for grid/list display
#[component]
pub fn PinBoardCard(
    board: Pinboard,
    /// Optional pin count to display (fetched separately)
    #[props(default)]
    pin_count: Option<usize>,
) -> Element {
    let title = board.title.clone();
    let description = board.description.clone();
    let cover_image = board.image.clone();
    let tags = board.tags.clone();
    let naddr = board.naddr.clone();
    let author_pubkey = board.pubkey.clone();
    let author_metadata = use_author_metadata(author_pubkey.clone());
    let display_name = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));
    let profile_picture = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone());
    let avatar_letter = display_name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let pin_text = pin_count.map(|c| {
        if c == 1 {
            "1 pin".to_string()
        } else {
            format!("{} pins", c)
        }
    });
    let is_owner = auth_store::get_pubkey()
        .map(|pk| pk == author_pubkey)
        .unwrap_or(false);
    rsx! {
        div { class: "group bg-card rounded-xl border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-lg flex flex-col",
            Link {
                to: Route::PinBoardDetail {
                    naddr: naddr.clone(),
                },
                class: "block flex-1 flex flex-col",
                div { class: "relative w-full h-40 bg-muted overflow-hidden",
                    if let Some(ref img_url) = cover_image {
                        img {
                            src: "{img_url}",
                            alt: "{title}",
                            class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full bg-gradient-to-br from-primary/30 via-purple-500/20 to-pink-500/20 flex items-center justify-center",
                            PinIcon {
                                class: "w-12 h-12 text-foreground/30".to_string(),
                                filled: false,
                            }
                        }
                    }
                    div { class: "absolute inset-0 bg-gradient-to-t from-black/60 via-transparent to-transparent" }
                    div { class: "absolute bottom-0 left-0 right-0 p-3",
                        h3 { class: "text-white font-bold text-lg line-clamp-2 drop-shadow-lg",
                            "{title}"
                        }
                        if let Some(ref text) = pin_text {
                            p { class: "text-white/80 text-sm mt-0.5", "{text}" }
                        }
                    }
                }
                div { class: "p-3 flex-1 flex flex-col gap-2",
                    div { class: "flex flex-wrap items-center gap-2",
                        for tag in tags.iter().take(2) {
                            HashtagBadge { key: "{tag}", tag: tag.clone() }
                        }
                        if is_owner {
                            span { class: "px-2 py-0.5 text-xs font-medium text-primary bg-primary/10 rounded-full",
                                "Your Board"
                            }
                        }
                    }
                    if let Some(ref desc) = description {
                        p { class: "text-sm text-muted-foreground line-clamp-2", "{desc}" }
                    }
                    div { class: "flex-1" }
                    div { class: "flex items-center gap-2 pt-2 border-t border-border",
                        div { class: "w-6 h-6 rounded-full overflow-hidden bg-muted flex items-center justify-center shrink-0",
                            if let Some(ref pic_url) = profile_picture {
                                img {
                                    src: "{pic_url}",
                                    alt: "{display_name}",
                                    class: "w-full h-full object-cover",
                                    loading: "lazy",
                                }
                            } else {
                                span { class: "text-xs font-semibold text-muted-foreground",
                                    "{avatar_letter}"
                                }
                            }
                        }
                        span { class: "text-sm text-muted-foreground truncate flex-1",
                            "{display_name}"
                        }
                    }
                }
            }
        }
    }
}
/// Masonry-style mosaic card with hover-reveal effects
/// Shows only image by default, reveals overlay with buttons and info on hover
#[component]
pub fn PinBoardCardMosaic(
    board: Pinboard,
    /// DEPRECATED: No longer used - cards now link directly to board detail
    #[props(default)]
    on_click: Option<EventHandler<Pinboard>>,
    /// Event handler for zap button click - parent should open ZapModal
    #[props(default)]
    on_zap_request: Option<EventHandler<Pinboard>>,
    /// Optional size variant for visual variety (small, medium, large)
    #[props(default)]
    size_variant: Option<String>,
) -> Element {
    let title = board.title.clone();
    let cover_image = board.image.clone();
    let naddr = board.naddr.clone();
    let author_pubkey = board.pubkey.clone();
    let _board_for_click = board.clone();
    let board_for_zap = board.clone();
    let board_for_react = board.clone();
    let author_metadata = use_author_metadata(author_pubkey.clone());
    let mut has_reacted = use_signal(|| false);
    let mut reaction_count = use_signal(|| 0usize);
    let mut reaction_loading = use_signal(|| false);
    let mut reaction_request_gen = use_signal(|| 0u32);
    let mut reaction_bootstrapped = use_signal(|| false);
    let mut reaction_error = use_signal(|| None::<String>);
    let a_tag_for_reactions = board.a_tag.clone();
    use_effect(use_reactive(
        (
            &a_tag_for_reactions,
            &crate::stores::auth_store::AUTH_STATE.read().pubkey,
        ),
        move |(a_tag, _pubkey)| {
            let a_tag = a_tag.clone();
            let has_signer = *HAS_SIGNER.read();
            let current_gen = reaction_request_gen.peek().wrapping_add(1);
            reaction_request_gen.set(current_gen);
            reaction_loading.set(true);
            reaction_error.set(None);
            if !has_signer {
                reaction_bootstrapped.set(false);
                reaction_count.set(0);
                has_reacted.set(false);
                reaction_loading.set(false);
                return;
            }
            reaction_bootstrapped.set(false);
            spawn(async move {
                if let Ok((count, reacted)) = fetch_pinboard_reaction_state(&a_tag).await {
                    if *reaction_request_gen.peek() != current_gen {
                        reaction_loading.set(false);
                        return;
                    }
                    reaction_count.set(count);
                    has_reacted.set(if has_signer { reacted } else { false });
                    reaction_bootstrapped.set(true);
                    reaction_error.set(None);
                    reaction_loading.set(false);
                } else if *reaction_request_gen.peek() == current_gen {
                    has_reacted.set(false);
                    reaction_bootstrapped.set(false);
                    reaction_count.set(0);
                    reaction_error.set(Some(
                        "Failed to load reactions. Click to retry.".to_string(),
                    ));
                    reaction_loading.set(false);
                } else {
                    reaction_loading.set(false);
                }
            });
        },
    ));
    let display_name = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));
    let profile_picture = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone());
    let avatar_letter = display_name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let heart_fill_class = if *has_reacted.read() {
        "fill-red-500 text-red-500"
    } else {
        ""
    };
    let heart_button_class = if *has_reacted.read() {
        "text-red-500"
    } else {
        "text-gray-600 hover:text-red-500"
    };
    let placeholder_height = match size_variant.as_deref() {
        Some("small") => "h-32",
        Some("large") => "h-64",
        _ => "h-48",
    };
    rsx! {
        Link {
            to: Route::PinBoardDetail {
                naddr: naddr.clone(),
            },
            class: "group relative overflow-hidden rounded-lg bg-muted cursor-pointer break-inside-avoid mb-2 shadow-xs hover:shadow-lg transition-shadow duration-300 block",
            if let Some(ref img_url) = cover_image {
                img {
                    src: "{img_url}",
                    alt: "{title}",
                    class: "w-full object-cover",
                    loading: "lazy",
                }
            } else {
                div { class: "w-full {placeholder_height} bg-gradient-to-br from-primary/30 via-purple-500/20 to-pink-500/20 flex items-center justify-center",
                    PinIcon {
                        class: "w-12 h-12 text-foreground/30".to_string(),
                        filled: false,
                    }
                }
            }
            div { class: "absolute inset-0 bg-gradient-to-t from-black/80 to-transparent
                        opacity-0 group-hover:opacity-100 transition-opacity duration-300 z-[1] pointer-events-none" }
            div { class: "absolute top-3 right-3 flex gap-2 z-[4]
                        opacity-0 translate-x-2
                        group-hover:opacity-100 group-hover:translate-x-0
                        transition-all duration-200 delay-100",
                button {
                    class: "rounded-full p-2 bg-white/90 hover:bg-white shadow-md backdrop-blur-sm
                            {heart_button_class} transition-colors disabled:opacity-50",
                    disabled: *reaction_loading.read() || !*HAS_SIGNER.read(),
                    onclick: move |e| {
                        e.stop_propagation();
                        if *reaction_loading.read() || !*HAS_SIGNER.read() {
                            return;
                        }
                        // If not bootstrapped, retry the bootstrap fetch first
                        if !*reaction_bootstrapped.read() {
                            let next_gen = reaction_request_gen.peek().wrapping_add(1);
                            reaction_request_gen.set(next_gen);
                            let current_gen = next_gen;
                            let a_tag = board_for_react.a_tag.clone();
                            let has_signer = *HAS_SIGNER.read();
                            reaction_loading.set(true);
                            reaction_error.set(None);
                            spawn(async move {
                                match fetch_pinboard_reaction_state(&a_tag).await {
                                    Ok((count, reacted)) => {
                                        if *reaction_request_gen.peek() != current_gen {
                                            reaction_loading.set(false);
                                            return;
                                        }
                                        reaction_count.set(count);
                                        has_reacted.set(if has_signer { reacted } else { false });
                                        reaction_bootstrapped.set(true);
                                        reaction_error.set(None);
                                        reaction_loading.set(false);
                                    }
                                    Err(_e) => {
                                        if *reaction_request_gen.peek() == current_gen {
                                            reaction_error.set(Some("Failed to load reactions. Click heart to retry.".to_string()));
                                        }
                                        reaction_loading.set(false);
                                    }
                                }
                            });
                            return;
                        }
                        // Invalidate any in-flight bootstrap fetch before optimistic update
                        let next_gen = reaction_request_gen.peek().wrapping_add(1);
                        reaction_request_gen.set(next_gen);
                        let board = board_for_react.clone();
                        let currently_reacted = *has_reacted.peek();
                        let current_count = *reaction_count.peek();
                        reaction_error.set(None);
                        has_reacted.set(!currently_reacted);
                        if currently_reacted {
                            reaction_count.set(current_count.saturating_sub(1));
                        } else {
                            reaction_count.set(current_count + 1);
                        }
                        reaction_loading.set(true);
                        let current_gen = *reaction_request_gen.peek();
                        spawn(async move {
                            match toggle_pinboard_reaction(&board, "+").await {
                                Ok(added) => {
                                    if *reaction_request_gen.peek() != current_gen {
                                        reaction_loading.set(false);
                                        return;
                                    }
                                    if added == currently_reacted {
                                        match fetch_pinboard_reaction_state(&board.a_tag).await {
                                            Ok((count, reacted)) => {
                                                if *reaction_request_gen.peek() != current_gen {
                                                    reaction_loading.set(false);
                                                    return;
                                                }
                                                has_reacted.set(reacted);
                                                reaction_count.set(count);
                                                reaction_error.set(None);
                                            }
                                            Err(e) => {
                                                log::warn!(
                                                    "Failed to reconcile reaction state after toggle: {}",
                                                    e
                                                );
                                                reaction_error.set(Some(
                                                    "Failed to update reaction - tap to retry."
                                                        .to_string(),
                                                ));
                                                reaction_count.set(current_count);
                                                has_reacted.set(currently_reacted);
                                            }
                                        }
                                    } else {
                                        has_reacted.set(added);
                                        reaction_error.set(None);
                                    }
                                }
                                Err(e) => {
                                    if *reaction_request_gen.peek() != current_gen {
                                        reaction_loading.set(false);
                                        return;
                                    }
                                    log::error!("Failed to toggle reaction: {}", e);
                                    reaction_error.set(Some(
                                        "Failed to update reaction - tap to retry.".to_string(),
                                    ));
                                    has_reacted.set(currently_reacted);
                                    reaction_count.set(current_count);
                                }
                            }
                            reaction_loading.set(false);
                        });
                    },
                    title: if let Some(ref err) = *reaction_error.read() {
                        "{err}"
                    } else if *has_reacted.read() {
                        "Unlike"
                    } else {
                        "Like"
                    },
                    svg {
                        class: "w-5 h-5 {heart_fill_class}",
                        fill: if *has_reacted.read() { "currentColor" } else { "none" },
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z",
                        }
                    }
                }
                button {
                    class: "rounded-full p-2 bg-white/90 hover:bg-white shadow-md backdrop-blur-sm
                            text-gray-600 hover:text-amber-500 transition-colors",
                    onclick: move |e| {
                        e.stop_propagation();
                        if let Some(handler) = &on_zap_request {
                            handler.call(board_for_zap.clone());
                        }
                    },
                    title: "Zap",
                    svg {
                        class: "w-5 h-5",
                        fill: "currentColor",
                        view_box: "0 0 24 24",
                        path { d: "M13 10V3L4 14h7v7l9-11h-7z" }
                    }
                }
            }
            div { class: "absolute bottom-3 left-2 right-3 text-white z-[4]
                        opacity-0 -translate-x-2
                        group-hover:opacity-100 group-hover:translate-x-0
                        transition-all duration-200 delay-100 pointer-events-none",
                div { class: "flex items-center gap-2 mb-1",
                    div { class: "w-5 h-5 rounded-full overflow-hidden bg-white/20 flex items-center justify-center shrink-0",
                        if let Some(ref pic_url) = profile_picture {
                            img {
                                src: "{pic_url}",
                                alt: "{display_name}",
                                class: "w-full h-full object-cover",
                                loading: "lazy",
                            }
                        } else {
                            span { class: "text-[10px] font-semibold text-white/80",
                                "{avatar_letter}"
                            }
                        }
                    }
                    span { class: "text-xs font-semibold text-white/90 truncate hover:underline",
                        "{display_name}"
                    }
                }
                h4 { class: "text-sm font-bold text-white line-clamp-2 hover:underline",
                    "{title}"
                }
            }
        }
    }
}
/// Loading skeleton for PinBoardCardMosaic with varied heights
#[component]
pub fn PinBoardCardMosaicSkeleton(
    /// Height variant for visual variety (small, medium, large)
    #[props(default)]
    height_variant: Option<String>,
) -> Element {
    let height_class = match height_variant.as_deref() {
        Some("small") => "h-32",
        Some("large") => "h-64",
        _ => "h-48",
    };
    rsx! {
        div { class: "bg-muted rounded-lg overflow-hidden animate-pulse break-inside-avoid mb-2 shadow-xs",
            div { class: "w-full {height_class} bg-gradient-to-br from-muted-foreground/10 to-muted-foreground/5" }
            if matches!(height_variant.as_deref(), Some("large")) {
                div { class: "p-3 space-y-2",
                    div { class: "h-4 w-3/4 bg-muted-foreground/10 rounded" }
                    div { class: "h-3 w-1/2 bg-muted-foreground/10 rounded" }
                }
            }
        }
    }
}
/// Size variants for visual variety in masonry layout
const SIZE_VARIANTS: [&str; 3] = ["small", "medium", "large"];
/// Masonry grid layout for pinboards
/// Uses CSS columns for true masonry layout with varied card heights
#[component]
pub fn PinBoardMosaicGrid(
    boards: Vec<Pinboard>,
    #[props(default)] on_board_click: Option<EventHandler<Pinboard>>,
    /// Handler for zap button clicks - parent should open ZapModal
    #[props(default)]
    on_zap_request: Option<EventHandler<Pinboard>>,
    #[props(default = false)] loading: bool,
    #[props(default = 8)] skeleton_count: usize,
    /// Callback when more items should be loaded
    #[props(default)]
    on_load_more: Option<EventHandler<()>>,
    /// Whether there are more items to load
    #[props(default = true)]
    has_more: bool,
    /// Whether currently loading more
    #[props(default = false)]
    loading_more: bool,
) -> Element {
    rsx! {
        div { class: "columns-1 sm:columns-2 md:columns-3 lg:columns-4 xl:columns-5 2xl:columns-6 gap-2",
            for (idx , board) in boards.iter().enumerate() {
                PinBoardCardMosaic {
                    key: "{board.a_tag}",
                    board: board.clone(),
                    on_click: on_board_click,
                    on_zap_request,
                    size_variant: if board.image.is_some() { None } else { Some(SIZE_VARIANTS[idx % 3].to_string()) },
                }
            }
            if loading {
                for i in 0..skeleton_count {
                    PinBoardCardMosaicSkeleton {
                        key: "mosaic-skeleton-{i}",
                        height_variant: Some(SIZE_VARIANTS[i % 3].to_string()),
                    }
                }
            }
        }
        if !loading && has_more && on_load_more.is_some() {
            div { class: "flex justify-center py-8",
                if loading_more {
                    div { class: "flex items-center gap-2 text-muted-foreground",
                        div { class: "w-5 h-5 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        span { "Loading more..." }
                    }
                } else {
                    button {
                        class: "px-6 py-3 bg-primary/10 hover:bg-primary/20 text-primary rounded-full font-medium transition-colors",
                        onclick: move |_| {
                            if let Some(ref handler) = on_load_more {
                                handler.call(());
                            }
                        },
                        "Load more boards"
                    }
                }
            }
        }
    }
}
/// Compact pinboard card for smaller displays or sidebars
#[component]
pub fn PinBoardCardCompact(board: Pinboard, #[props(default)] pin_count: Option<usize>) -> Element {
    let title = board.title.clone();
    let cover_image = board.image.clone();
    let naddr = board.naddr.clone();
    let pin_text = pin_count.map(|c| {
        if c == 1 {
            "1 pin".to_string()
        } else {
            format!("{} pins", c)
        }
    });
    rsx! {
        Link {
            to: Route::PinBoardDetail {
                naddr: naddr.clone(),
            },
            class: "flex items-center gap-3 p-3 rounded-lg hover:bg-accent transition",
            div { class: "w-12 h-12 rounded-lg overflow-hidden bg-muted shrink-0",
                if let Some(ref img_url) = cover_image {
                    img {
                        src: "{img_url}",
                        alt: "{title}",
                        class: "w-full h-full object-cover",
                        loading: "lazy",
                    }
                } else {
                    div { class: "w-full h-full bg-gradient-to-br from-primary/30 to-purple-500/20 flex items-center justify-center",
                        PinIcon {
                            class: "w-5 h-5 text-foreground/30".to_string(),
                            filled: false,
                        }
                    }
                }
            }
            div { class: "flex-1 min-w-0",
                h4 { class: "font-medium truncate", "{title}" }
                if let Some(ref text) = pin_text {
                    p { class: "text-xs text-muted-foreground", "{text}" }
                }
            }
            svg {
                class: "w-4 h-4 text-muted-foreground shrink-0",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                polyline { points: "9 18 15 12 9 6" }
            }
        }
    }
}
/// Loading skeleton for PinBoardCard
#[component]
pub fn PinBoardCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card rounded-xl border border-border overflow-hidden animate-pulse",
            div { class: "w-full h-40 bg-muted" }
            div { class: "p-3 space-y-3",
                div { class: "h-5 w-20 bg-muted rounded-full" }
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "flex items-center gap-2 pt-2 border-t border-border",
                    div { class: "w-6 h-6 rounded-full bg-muted" }
                    div { class: "h-4 w-24 bg-muted rounded" }
                }
            }
        }
    }
}
/// Loading skeleton for PinBoardCardCompact
#[component]
pub fn PinBoardCardCompactSkeleton() -> Element {
    rsx! {
        div { class: "flex items-center gap-3 p-3 animate-pulse",
            div { class: "w-12 h-12 rounded-lg bg-muted shrink-0" }
            div { class: "flex-1 space-y-2",
                div { class: "h-4 w-32 bg-muted rounded" }
                div { class: "h-3 w-16 bg-muted rounded" }
            }
        }
    }
}
/// Masonry-style grid for pinboards
#[component]
pub fn PinBoardGrid(
    boards: Vec<Pinboard>,
    #[props(default = false)] loading: bool,
    #[props(default = 4)] skeleton_count: usize,
) -> Element {
    rsx! {
        div { class: "grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4",
            for board in boards {
                PinBoardCard { key: "{board.a_tag}", board }
            }
            if loading {
                for i in 0..skeleton_count {
                    PinBoardCardSkeleton { key: "skeleton-{i}" }
                }
            }
        }
    }
}
