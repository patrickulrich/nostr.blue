//! Community Post Card Component
//! Displays a community post with approval status badges, moderation actions,
//! reactions, zaps, and threading support
use crate::components::{CommunityPostComposer, RichContent, SensitiveContent, ZapModal};
use crate::hooks::{use_reaction, ReactionState};
use crate::routes::Route;
use crate::services::aggregation::InteractionCounts;
use crate::stores::auth_store;
use crate::stores::community_store::{
    approve_post, can_moderate, remove_post, ApprovalStatus, AutoApprovalReason, Community,
    CommunityPost, UserRole,
};
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::profiles::get_cached_profile;
use crate::utils::format::{format_relative_time_or, format_sats_human};
use crate::utils::validation::is_valid_http_url;
use crate::utils::nip36;
use dioxus::prelude::*;
/// Maximum depth for visual indentation
const MAX_VISUAL_DEPTH: usize = 4;
/// Post card for community feeds with approval status, moderation, reactions, and zaps
#[component]
pub fn CommunityPostCard(
    post: CommunityPost,
    community: Community,
    #[props(default = 0)] depth: usize,
    #[props(default)] interaction_counts: Option<InteractionCounts>,
    #[props(default = true)] show_actions: bool,
    #[props(default = false)] show_moderation: bool,
    #[props(default)] on_reply_success: Option<EventHandler<String>>,
    /// Called when a moderation action (approve/remove) completes successfully
    #[props(default)]
    on_moderation_complete: Option<EventHandler<()>>,
) -> Element {
    let mut approving = use_signal(|| false);
    let mut removing = use_signal(|| false);
    let mut show_remove_dialog = use_signal(|| false);
    let mut show_reply_composer = use_signal(|| false);
    let mut remove_reason = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut show_zap_modal = use_signal(|| false);
    let has_signer = *HAS_SIGNER.read();
    let current_pubkey = auth_store::get_pubkey();
    let is_moderator = current_pubkey
        .as_ref()
        .map(|pk| can_moderate(pk, &community))
        .unwrap_or(false);
    let profile = get_cached_profile(&post.pubkey);
    let author_name = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| {
            let truncated: String = post.pubkey.chars().take(8).collect();
            format!("{}...", truncated)
        });
    let author_picture = profile.as_ref().and_then(|p| p.picture.clone());
    let author_pubkey = post.pubkey.clone();
    let time_ago = format_relative_time_or(post.created_at, "just now");
    let indent_class = match depth.min(MAX_VISUAL_DEPTH) {
        0 => "",
        1 => "ml-4 border-l-2 border-blue-200 dark:border-blue-800 pl-3",
        2 => "ml-8 border-l-2 border-purple-200 dark:border-purple-800 pl-3",
        3 => "ml-12 border-l-2 border-green-200 dark:border-green-800 pl-3",
        _ => "ml-16 border-l-2 border-orange-200 dark:border-orange-800 pl-3",
    };
    let reaction = use_reaction(
        post.id.clone(),
        post.pubkey.clone(),
        interaction_counts.as_ref(),
    );
    let reply_count = interaction_counts.as_ref().map(|c| c.replies).unwrap_or(0);
    let zap_amount = interaction_counts
        .as_ref()
        .map(|c| c.zap_amount_sats)
        .unwrap_or(0);
    let post_for_approve = post.clone();
    let community_for_approve = community.clone();
    let post_for_remove = post.clone();
    let community_for_remove = community.clone();
    let handle_approve = move |_| {
        let post = post_for_approve.clone();
        let community = community_for_approve.clone();
        approving.set(true);
        error.set(None);
        spawn(async move {
            match approve_post(&community, &post).await {
                Ok(_) => {
                    log::info!("Post approved: {}", post.id);
                    if let Some(ref callback) = on_moderation_complete {
                        callback.call(());
                    }
                }
                Err(e) => {
                    log::error!("Failed to approve post: {}", e);
                    error.set(Some(e));
                }
            }
            approving.set(false);
        });
    };
    let handle_remove = move |_| {
        let post = post_for_remove.clone();
        let community = community_for_remove.clone();
        let reason = remove_reason.read().clone();
        removing.set(true);
        error.set(None);
        spawn(async move {
            let reason_opt = if reason.is_empty() {
                None
            } else {
                Some(reason.as_str())
            };
            match remove_post(&community, &post, reason_opt).await {
                Ok(_) => {
                    log::info!("Post removed: {}", post.id);
                    show_remove_dialog.set(false);
                    if let Some(ref callback) = on_moderation_complete {
                        callback.call(());
                    }
                }
                Err(e) => {
                    log::error!("Failed to remove post: {}", e);
                    error.set(Some(e));
                }
            }
            removing.set(false);
        });
    };
    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition {indent_class}",
            if let Some(err) = error.read().as_ref() {
                div { class: "mb-3 p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                    "{err}"
                }
            }
            if depth > MAX_VISUAL_DEPTH {
                div { class: "text-xs text-muted-foreground mb-2", "Nested {depth} levels deep" }
            }
            div { class: "flex items-center justify-between mb-2",
                div { class: "flex items-center gap-2",
                    Link {
                        to: Route::Profile {
                            pubkey: crate::utils::nip19_urls::profile_route_id(&author_pubkey),
                        },
                        if let Some(ref pic) = author_picture.as_ref().filter(|u| is_valid_http_url(u)) {
                            img {
                                class: "w-10 h-10 rounded-full object-cover",
                                src: "{pic}",
                                alt: "Profile",
                            }
                        } else {
                            div { class: "w-10 h-10 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white font-bold",
                                "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                            }
                        }
                    }
                    div {
                        Link {
                             to: Route::Profile {
                                 pubkey: crate::utils::nip19_urls::profile_route_id(&author_pubkey),
                             },
                             class: "font-medium hover:underline",
                            "{author_name}"
                        }
                        p { class: "text-xs text-muted-foreground", "{time_ago}" }
                    }
                }
                ApprovalBadge { status: post.approval_status.clone() }
            }
            div { class: "mb-3",
                {
                    let content_warning = nip36::get_content_warning(&post.event.tags);
                    if let Some(reason) = content_warning {
                        rsx! {
                            SensitiveContent { reason,
                                RichContent {
                                    content: post.content.clone(),
                                    tags: post.event.tags.iter().cloned().collect(),
                                    interactive_media: true,
                                }
                            }
                        }
                    } else {
                        rsx! {
                            RichContent {
                                content: post.content.clone(),
                                tags: post.event.tags.iter().cloned().collect(),
                                interactive_media: true,
                            }
                        }
                    }
                }
            }
            if show_actions {
                div { class: "flex items-center gap-4 text-sm text-muted-foreground mb-2",
                    if has_signer {
                        button {
                            class: "flex items-center gap-1 hover:text-blue-500 transition",
                            onclick: move |_| show_reply_composer.set(true),
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
                                path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                            }
                            if reply_count > 0 {
                                "{reply_count}"
                            } else {
                                "Reply"
                            }
                        }
                    } else if reply_count > 0 {
                        span { class: "flex items-center gap-1",
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
                                path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                            }
                            "{reply_count}"
                        }
                    }
                    if has_signer {
                        button {
                            class: if *reaction.is_liked.read() { "flex items-center gap-1 hover:text-red-500 transition disabled:opacity-50 text-red-500" } else { "flex items-center gap-1 hover:text-red-500 transition disabled:opacity-50" },
                            disabled: matches!(*reaction.state.read(), ReactionState::Pending),
                            onclick: move |_| reaction.toggle_like.call(()),
                            svg {
                                class: if *reaction.is_liked.read() { "w-4 h-4 fill-current" } else { "w-4 h-4" },
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: if *reaction.is_liked.read() { "currentColor" } else { "none" },
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z" }
                            }
                            if *reaction.like_count.read() > 0 {
                                "{reaction.like_count}"
                            }
                        }
                    } else {
                        span { class: "flex items-center gap-1",
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
                                path { d: "M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z" }
                            }
                            if *reaction.like_count.read() > 0 {
                                "{reaction.like_count}"
                            }
                        }
                    }
                    if has_signer {
                        button {
                            class: if zap_amount > 0 { "flex items-center gap-1 hover:text-yellow-500 transition text-yellow-500" } else { "flex items-center gap-1 hover:text-yellow-500 transition" },
                            onclick: move |_| show_zap_modal.set(true),
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: if zap_amount > 0 { "currentColor" } else { "none" },
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                polygon { points: "13 2 3 14 12 14 11 22 21 10 12 10 13 2" }
                            }
                            if zap_amount > 0 {
                                "{format_sats_human(zap_amount)}"
                            }
                        }
                    } else if zap_amount > 0 {
                        span { class: "flex items-center gap-1 text-yellow-500",
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "currentColor",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                polygon { points: "13 2 3 14 12 14 11 22 21 10 12 10 13 2" }
                            }
                            "{format_sats_human(zap_amount)}"
                        }
                    }
                }
                if show_moderation && is_moderator {
                    div { class: "flex items-center gap-2 pt-2 border-t border-border",
                        if matches!(post.approval_status, ApprovalStatus::Pending) {
                            button {
                                class: "px-3 py-1 bg-green-500 hover:bg-green-600 text-white rounded text-sm font-medium transition disabled:opacity-50",
                                disabled: *approving.read(),
                                onclick: handle_approve,
                                if *approving.read() {
                                    span { class: "flex items-center gap-1",
                                        span { class: "inline-block w-3 h-3 border-2 border-white border-t-transparent rounded-full animate-spin" }
                                        "Approving..."
                                    }
                                } else {
                                    span { class: "flex items-center gap-1",
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
                                            polyline { points: "20 6 9 17 4 12" }
                                        }
                                        "Approve"
                                    }
                                }
                            }
                        }
                        if !matches!(post.approval_status, ApprovalStatus::Removed(_)) {
                            button {
                                class: "px-3 py-1 bg-red-500 hover:bg-red-600 text-white rounded text-sm font-medium transition",
                                onclick: move |_| show_remove_dialog.set(true),
                                span { class: "flex items-center gap-1",
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
                                        path { d: "M3 6h18" }
                                        path { d: "M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" }
                                        path { d: "M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" }
                                    }
                                    "Remove"
                                }
                            }
                        }
                    }
                }
            }
            if *show_remove_dialog.read() {
                div {
                    class: "fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4",
                    onclick: move |_| show_remove_dialog.set(false),
                    div {
                        class: "bg-background rounded-lg p-6 max-w-md w-full shadow-xl",
                        role: "dialog",
                        "aria-modal": "true",
                        tabindex: "-1",
                        onclick: move |e| e.stop_propagation(),
                        onmounted: move |_evt| {
                            #[cfg(feature = "web")]
                            {
                                if let Some(el) = _evt.data().downcast::<web_sys::HtmlElement>() {
                                    let _ = el.focus();
                                }
                            }
                        },
                        onkeydown: move |evt: KeyboardEvent| {
                            if evt.key() == Key::Escape {
                                show_remove_dialog.set(false);
                            }
                        },
                        h3 { class: "text-lg font-bold mb-4", "Remove Post" }
                        p { class: "text-sm text-muted-foreground mb-4",
                            "Are you sure you want to remove this post? Optionally provide a reason."
                        }
                        textarea {
                            class: "w-full p-3 border border-border rounded-lg mb-4 bg-background",
                            placeholder: "Reason (optional)...",
                            rows: 3,
                            value: "{remove_reason}",
                            oninput: move |evt| remove_reason.set(evt.value()),
                        }
                        div { class: "flex gap-2 justify-end",
                            button {
                                class: "px-4 py-2 border border-border rounded-lg hover:bg-accent transition",
                                onclick: move |_| show_remove_dialog.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 bg-red-500 hover:bg-red-600 text-white rounded-lg transition disabled:opacity-50",
                                disabled: *removing.read(),
                                onclick: handle_remove,
                                if *removing.read() {
                                    span { class: "flex items-center gap-2",
                                        span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" }
                                        "Removing..."
                                    }
                                } else {
                                    "Remove Post"
                                }
                            }
                        }
                    }
                }
            }
            if *show_zap_modal.read() {
                {
                    let recipient_name = author_name.clone();
                    let lud16 = profile.as_ref().and_then(|p| p.lud16.clone());
                    let lud06: Option<String> = None;
                    rsx! {
                        ZapModal {
                            recipient_pubkey: post.pubkey.clone(),
                            recipient_name,
                            lud16,
                            lud06,
                            event_id: Some(post.id.clone()),
                            on_close: move |_| show_zap_modal.set(false),
                        }
                    }
                }
            }
            if *show_reply_composer.read() {
                {
                    let on_reply_success_clone = on_reply_success;
                    rsx! {
                        CommunityPostComposer {
                            community: community.clone(),
                            reply_to: Some(post.clone()),
                            on_close: move |_| show_reply_composer.set(false),
                            on_success: move |event_id: String| {
                                show_reply_composer.set(false);
                                if let Some(handler) = on_reply_success_clone.as_ref() {
                                    handler.call(event_id);
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
/// Approval status badge component
#[component]
pub fn ApprovalBadge(status: ApprovalStatus) -> Element {
    match status {
        ApprovalStatus::AutoApproved(reason) => {
            let (label, icon) = match reason {
                AutoApprovalReason::Owner => ("Owner", "crown"),
                AutoApprovalReason::Moderator => ("Mod", "shield"),
                AutoApprovalReason::ApprovedMember => ("Member", "star"),
            };
            rsx! {
                span { class: "px-2 py-0.5 bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 rounded text-xs font-medium flex items-center gap-1",
                    if icon == "crown" {
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
                            path { d: "m2 4 3 12h14l3-12-6 7-4-7-4 7-6-7z" }
                            path { d: "M3 22h18" }
                        }
                    } else if icon == "shield" {
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
                    } else {
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
                            polygon { points: "12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" }
                        }
                    }
                    "{label}"
                }
            }
        }
        ApprovalStatus::Approved(_) => {
            rsx! {
                span { class: "px-2 py-0.5 bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 rounded text-xs font-medium flex items-center gap-1",
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
                        polyline { points: "20 6 9 17 4 12" }
                    }
                    "Approved"
                }
            }
        }
        ApprovalStatus::Pending => {
            rsx! {
                span { class: "px-2 py-0.5 bg-yellow-100 dark:bg-yellow-900 text-yellow-800 dark:text-yellow-200 rounded text-xs font-medium flex items-center gap-1",
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
                        circle { cx: "12", cy: "12", r: "10" }
                        polyline { points: "12 6 12 12 16 14" }
                    }
                    "Pending"
                }
            }
        }
        ApprovalStatus::Removed(removal) => {
            rsx! {
                span {
                    class: "px-2 py-0.5 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-xs font-medium flex items-center gap-1",
                    title: "{removal.reason.as_deref().unwrap_or(\"No reason provided\")}",
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
                        path { d: "M3 6h18" }
                        path { d: "M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" }
                        path { d: "M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" }
                    }
                    "Removed"
                }
            }
        }
    }
}
/// User role badge component
#[component]
pub fn UserRoleBadge(role: UserRole) -> Element {
    match role {
        UserRole::Owner => {
            rsx! {
                span { class: "px-2 py-0.5 bg-purple-100 dark:bg-purple-900 text-purple-800 dark:text-purple-200 rounded text-xs font-medium flex items-center gap-1",
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
                        path { d: "m2 4 3 12h14l3-12-6 7-4-7-4 7-6-7z" }
                        path { d: "M3 22h18" }
                    }
                    "Owner"
                }
            }
        }
        UserRole::Moderator => {
            rsx! {
                span { class: "px-2 py-0.5 bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 rounded text-xs font-medium flex items-center gap-1",
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
                    "Moderator"
                }
            }
        }
        UserRole::ApprovedMember => {
            rsx! {
                span { class: "px-2 py-0.5 bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 rounded text-xs font-medium flex items-center gap-1",
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
                        polygon { points: "12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" }
                    }
                    "Member"
                }
            }
        }
        UserRole::Pending => {
            rsx! {
                span { class: "px-2 py-0.5 bg-orange-100 dark:bg-orange-900 text-orange-800 dark:text-orange-200 rounded text-xs font-medium flex items-center gap-1",
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
                        circle { cx: "12", cy: "12", r: "10" }
                        polyline { points: "12 6 12 12 16 14" }
                    }
                    "Pending"
                }
            }
        }
        UserRole::Declined => {
            rsx! {
                span { class: "px-2 py-0.5 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-xs font-medium flex items-center gap-1",
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
                        circle { cx: "12", cy: "12", r: "10" }
                        line {
                            x1: "15",
                            y1: "9",
                            x2: "9",
                            y2: "15",
                        }
                        line {
                            x1: "9",
                            y1: "9",
                            x2: "15",
                            y2: "15",
                        }
                    }
                    "Declined"
                }
            }
        }
        UserRole::Visitor => {
            rsx! {
                span { class: "px-2 py-0.5 bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400 rounded text-xs font-medium",
                    "Visitor"
                }
            }
        }
    }
}
/// Loading skeleton for CommunityPostCard
#[component]
pub fn CommunityPostCardSkeleton() -> Element {
    rsx! {
        div { class: "p-4 border-b border-border animate-pulse",
            div { class: "flex items-center gap-2 mb-3",
                div { class: "w-10 h-10 rounded-full bg-muted" }
                div {
                    div { class: "h-4 w-24 bg-muted rounded mb-1" }
                    div { class: "h-3 w-16 bg-muted rounded" }
                }
            }
            div { class: "space-y-2 mb-3",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
                div { class: "h-4 bg-muted rounded w-4/6" }
            }
            div { class: "flex gap-4",
                div { class: "h-4 w-12 bg-muted rounded" }
                div { class: "h-4 w-12 bg-muted rounded" }
                div { class: "h-4 w-12 bg-muted rounded" }
            }
        }
    }
}
