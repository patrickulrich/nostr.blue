//! Community Card Component
//! Displays a community in a card format for listing pages
//! With membership badges, join buttons, and pin functionality

use dioxus::prelude::*;
use crate::stores::community_store::{
    Community, MembershipStatus, CommunityWithMembership,
    membership_status_to_role, submit_join_request,
};
use crate::stores::pinned_communities::{is_community_pinned, pin_community, unpin_community};
use crate::stores::auth_store;
use crate::routes::Route;
use crate::utils::validation::is_valid_http_url;
use super::post_card::UserRoleBadge;
use crate::components::icons::PinIcon;

// ============================================================================
// JoinButton Component
// ============================================================================

/// Join button with multiple states based on membership status
#[component]
pub fn JoinButton(
    community: Community,
    membership_status: MembershipStatus,
) -> Element {
    let mut is_loading = use_signal(|| false);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let has_signer = *crate::stores::nostr_client::HAS_SIGNER.read();
    let is_logged_in = auth_store::get_pubkey().is_some();

    let community_for_click = community.clone();

    let on_click = move |_| {
        if *is_loading.read() {
            return;
        }

        let community = community_for_click.clone();
        is_loading.set(true);
        error_msg.set(None);

        spawn(async move {
            match submit_join_request(&community, None).await {
                Ok(_) => {
                    log::info!("Join request submitted successfully");
                }
                Err(e) => {
                    log::error!("Failed to submit join request: {}", e);
                    error_msg.set(Some(e));
                }
            }
            is_loading.set(false);
        });
    };

    // Determine button state and appearance
    let (button_class, button_text, is_disabled) = match &membership_status {
        MembershipStatus::Owner => (
            "bg-purple-500 cursor-default opacity-75",
            "Owner",
            true,
        ),
        MembershipStatus::Moderator => (
            "bg-blue-500 cursor-default opacity-75",
            "Moderator",
            true,
        ),
        MembershipStatus::Member => (
            "bg-green-500 cursor-default opacity-75",
            "Member",
            true,
        ),
        MembershipStatus::Pending { .. } => (
            "bg-orange-500 cursor-default opacity-75",
            "Pending",
            true,
        ),
        MembershipStatus::Declined { .. } => (
            "bg-red-500 cursor-default opacity-75",
            "Declined",
            true,
        ),
        MembershipStatus::Banned { .. } => (
            "bg-red-700 cursor-not-allowed opacity-50",
            "Banned",
            true,
        ),
        MembershipStatus::None => {
            if !is_logged_in {
                ("bg-gray-400 cursor-not-allowed opacity-50", "Log in to join", true)
            } else if !has_signer {
                ("bg-gray-400 cursor-not-allowed opacity-50", "No signer", true)
            } else if *is_loading.read() {
                ("bg-green-500 opacity-75", "Joining...", true)
            } else {
                ("bg-green-500 hover:bg-green-600", "Join", false)
            }
        }
    };

    rsx! {
        button {
            class: "px-4 py-2 text-white rounded-lg font-medium transition text-sm {button_class}",
            disabled: is_disabled,
            onclick: on_click,
            {button_text}
        }
        if let Some(err) = error_msg.read().as_ref() {
            p {
                class: "text-xs text-red-500 mt-1",
                "{err}"
            }
        }
    }
}

// ============================================================================
// PinCommunityButton Component
// ============================================================================

/// Pin/unpin toggle button for communities
#[component]
fn PinCommunityButton(a_tag: String) -> Element {
    let mut is_pinned = use_signal(|| is_community_pinned(&a_tag));
    let mut is_loading = use_signal(|| false);
    let mut pin_error: Signal<Option<String>> = use_signal(|| None);
    let is_logged_in = auth_store::get_pubkey().is_some();

    let a_tag_for_click = a_tag.clone();

    let on_click = move |evt: Event<MouseData>| {
        evt.stop_propagation();

        if *is_loading.read() || !is_logged_in {
            return;
        }

        let a_tag = a_tag_for_click.clone();
        let current_pinned = *is_pinned.read();
        is_loading.set(true);
        pin_error.set(None);

        spawn(async move {
            let result = if current_pinned {
                unpin_community(a_tag.clone()).await
            } else {
                pin_community(a_tag.clone()).await
            };

            match result {
                Ok(_) => {
                    // Toggle the state
                    is_pinned.set(!current_pinned);
                    pin_error.set(None);
                }
                Err(e) => {
                    log::error!("Failed to toggle pin: {}", e);
                    pin_error.set(Some("Failed to update pin".to_string()));
                }
            }
            is_loading.set(false);
        });
    };

    if !is_logged_in {
        return rsx! {};
    }

    rsx! {
        div {
            class: "relative",
            button {
                class: "p-1.5 rounded hover:bg-accent transition",
                class: if *is_pinned.read() { "text-primary" } else { "text-muted-foreground hover:text-foreground" },
                title: if *is_pinned.read() { "Unpin community" } else { "Pin community" },
                onclick: on_click,
                PinIcon {
                    class: "w-4 h-4".to_string(),
                    filled: *is_pinned.read()
                }
            }
            if pin_error.read().is_some() {
                span {
                    class: "absolute top-full right-0 mt-1 text-xs text-destructive whitespace-nowrap",
                    "Pin failed"
                }
            }
        }
    }
}

// ============================================================================
// MembershipBadge Component
// ============================================================================

/// Admin badge showing pending request count
#[component]
fn AdminBadge(count: u32) -> Element {
    if count == 0 {
        return rsx! {};
    }

    let title = if count == 1 {
        format!("{} pending join request", count)
    } else {
        format!("{} pending join requests", count)
    };

    rsx! {
        span {
            class: "px-1.5 py-0.5 bg-blue-500 text-white rounded-full text-xs font-medium",
            title: "{title}",
            "{count}"
        }
    }
}

// ============================================================================
// Enhanced CommunityCard Component
// ============================================================================

/// Community card with membership display for grid/list display
#[component]
pub fn CommunityCardWithMembership(
    data: CommunityWithMembership,
) -> Element {
    let community = &data.community;
    let membership_status = &data.membership_status;
    let _is_pinned = data.is_pinned; // Used for future enhancements
    let pending_count = data.pending_request_count;

    // Determine if user has a role worth showing
    let show_role_badge = !matches!(membership_status, MembershipStatus::None);
    let role = membership_status_to_role(membership_status);

    // Background tint for members
    let bg_class = match membership_status {
        MembershipStatus::Member | MembershipStatus::Moderator | MembershipStatus::Owner => {
            "border border-border rounded-lg p-4 hover:shadow-md transition-shadow bg-green-50/30 dark:bg-green-950/20"
        }
        MembershipStatus::Pending { .. } => {
            "border border-border rounded-lg p-4 hover:shadow-md transition-shadow bg-orange-50/30 dark:bg-orange-950/20"
        }
        _ => {
            "border border-border rounded-lg p-4 hover:shadow-md transition-shadow bg-card"
        }
    };

    rsx! {
        div {
            class: "{bg_class}",

            // Header with pin button
            div {
                class: "flex items-start gap-3 mb-3",

                // Avatar/Image (Security Fix #2 - URL validation)
                if let Some(image_url) = community.image.as_ref().filter(|u| is_valid_http_url(u)) {
                    img {
                        class: "w-12 h-12 rounded-full object-cover shrink-0",
                        src: "{image_url}",
                        alt: "Community image"
                    }
                } else {
                    div {
                        class: "w-12 h-12 rounded-full bg-gradient-to-br from-purple-400 to-blue-500 flex items-center justify-center text-white shrink-0",
                        svg {
                            class: "w-6 h-6",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }
                            circle { cx: "9", cy: "7", r: "4" }
                            path { d: "M22 21v-2a4 4 0 0 0-3-3.87" }
                            path { d: "M16 3.13a4 4 0 0 1 0 7.75" }
                        }
                    }
                }

                // Info and badges
                div {
                    class: "flex-1 min-w-0",
                    div {
                        class: "flex items-center gap-2 flex-wrap",
                        h3 {
                            class: "text-lg font-bold truncate",
                            "{community.name.as_ref().unwrap_or(&community.d_tag)}"
                        }
                        if show_role_badge {
                            UserRoleBadge { role: role.clone() }
                        }
                        if let Some(count) = pending_count {
                            AdminBadge { count }
                        }
                    }
                    p {
                        class: "text-xs text-muted-foreground truncate",
                        "{community.d_tag}"
                    }
                }

                // Pin button
                PinCommunityButton { a_tag: community.a_tag.clone() }
            }

            // Description
            if let Some(desc) = &community.description {
                p {
                    class: "text-sm text-muted-foreground mb-3 line-clamp-2",
                    "{desc}"
                }
            }

            // Moderators count
            if !community.moderators.is_empty() {
                div {
                    class: "mb-3",
                    p {
                        class: "text-xs text-muted-foreground flex items-center gap-1",
                        svg {
                            class: "w-3 h-3",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10" }
                        }
                        "{community.moderators.len()} moderator"
                        if community.moderators.len() != 1 { "s" }
                    }
                }
            }

            // Action buttons
            div {
                class: "pt-3 border-t border-border flex items-center gap-2",

                // Join button (shows status)
                JoinButton {
                    community: community.clone(),
                    membership_status: membership_status.clone()
                }

                // View button
                Link {
                    to: Route::CommunityPage { a_tag: community.a_tag.clone() },
                    class: "flex-1 flex items-center justify-center gap-2 px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition text-sm",
                    "View"
                    svg {
                        class: "w-4 h-4",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        line { x1: "5", y1: "12", x2: "19", y2: "12" }
                        polyline { points: "12 5 19 12 12 19" }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Original CommunityCard (backward compatible)
// ============================================================================

/// Community card for grid/list display (original simple version)
#[component]
pub fn CommunityCard(community: Community) -> Element {
    rsx! {
        div {
            class: "border border-border rounded-lg p-4 hover:shadow-md transition-shadow bg-card",

            div {
                class: "flex items-start gap-3 mb-3",

                // Avatar/Image (Security Fix #3 - URL validation)
                if let Some(image_url) = community.image.as_ref().filter(|u| is_valid_http_url(u)) {
                    img {
                        class: "w-12 h-12 rounded-full object-cover",
                        src: "{image_url}",
                        alt: "Community image"
                    }
                } else {
                    div {
                        class: "w-12 h-12 rounded-full bg-gradient-to-br from-purple-400 to-blue-500 flex items-center justify-center text-white",
                        svg {
                            class: "w-6 h-6",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }
                            circle { cx: "9", cy: "7", r: "4" }
                            path { d: "M22 21v-2a4 4 0 0 0-3-3.87" }
                            path { d: "M16 3.13a4 4 0 0 1 0 7.75" }
                        }
                    }
                }

                // Info
                div {
                    class: "flex-1 min-w-0",
                    h3 {
                        class: "text-lg font-bold truncate",
                        "{community.name.as_ref().unwrap_or(&community.d_tag)}"
                    }
                    p {
                        class: "text-xs text-muted-foreground truncate",
                        "{community.d_tag}"
                    }
                }
            }

            // Description
            if let Some(desc) = &community.description {
                p {
                    class: "text-sm text-muted-foreground mb-3 line-clamp-2",
                    "{desc}"
                }
            }

            // Moderators count
            if !community.moderators.is_empty() {
                div {
                    class: "mb-3",
                    p {
                        class: "text-xs text-muted-foreground flex items-center gap-1",
                        svg {
                            class: "w-3 h-3",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10" }
                        }
                        "{community.moderators.len()} moderator"
                        if community.moderators.len() != 1 { "s" }
                    }
                }
            }

            // View button
            div {
                class: "pt-3 border-t border-border",
                Link {
                    to: Route::CommunityPage { a_tag: community.a_tag.clone() },
                    class: "w-full flex items-center justify-between px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                    span {
                        class: "flex items-center gap-2",
                        "View Community"
                    }
                    svg {
                        class: "w-4 h-4",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        line { x1: "5", y1: "12", x2: "19", y2: "12" }
                        polyline { points: "12 5 19 12 12 19" }
                    }
                }
            }
        }
    }
}

/// Compact community card for smaller displays
#[component]
pub fn CommunityCardCompact(community: Community) -> Element {
    rsx! {
        Link {
            to: Route::CommunityPage { a_tag: community.a_tag.clone() },
            class: "flex items-center gap-3 p-3 rounded-lg hover:bg-accent transition",

            // Avatar (Security Fix #4 - URL validation)
            if let Some(image_url) = community.image.as_ref().filter(|u| is_valid_http_url(u)) {
                img {
                    class: "w-10 h-10 rounded-full object-cover",
                    src: "{image_url}",
                    alt: "Community image"
                }
            } else {
                div {
                    class: "w-10 h-10 rounded-full bg-gradient-to-br from-purple-400 to-blue-500 flex items-center justify-center text-white",
                    svg {
                        class: "w-5 h-5",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }
                        circle { cx: "9", cy: "7", r: "4" }
                        path { d: "M22 21v-2a4 4 0 0 0-3-3.87" }
                        path { d: "M16 3.13a4 4 0 0 1 0 7.75" }
                    }
                }
            }

            // Info
            div {
                class: "flex-1 min-w-0",
                h4 {
                    class: "font-medium truncate",
                    "{community.name.as_ref().unwrap_or(&community.d_tag)}"
                }
                if !community.moderators.is_empty() {
                    p {
                        class: "text-xs text-muted-foreground",
                        if community.moderators.len() == 1 {
                            "1 moderator"
                        } else {
                            "{community.moderators.len()} moderators"
                        }
                    }
                }
            }

            // Arrow
            svg {
                class: "w-4 h-4 text-muted-foreground",
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

/// Loading skeleton for CommunityCard
#[component]
pub fn CommunityCardSkeleton() -> Element {
    rsx! {
        div {
            class: "border border-border rounded-lg p-4 bg-card animate-pulse",

            div {
                class: "flex items-start gap-3 mb-3",
                // Avatar skeleton
                div {
                    class: "w-12 h-12 rounded-full bg-muted"
                }
                // Info skeleton
                div {
                    class: "flex-1",
                    div {
                        class: "h-5 w-32 bg-muted rounded mb-2"
                    }
                    div {
                        class: "h-3 w-24 bg-muted rounded"
                    }
                }
            }

            // Description skeleton
            div {
                class: "space-y-2 mb-3",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-3/4" }
            }

            // Button skeleton
            div {
                class: "pt-3 border-t border-border",
                div {
                    class: "h-10 bg-muted rounded-lg"
                }
            }
        }
    }
}
