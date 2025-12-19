//! Community Detail Page
//! Displays a single community with threaded posts, moderation queue, and about info

use dioxus::prelude::*;
use crate::stores::community_store::{
    Community, CommunityPost, CommunityThread, fetch_community_by_a_tag, fetch_community_posts,
    fetch_pending_posts, get_user_role, can_moderate, UserRole, MembershipStatus,
    build_community_thread_tree, flatten_thread_tree, get_membership_status,
};
use crate::stores::auth_store;
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::profiles::{fetch_profiles_batch, get_cached_profile};
use crate::services::aggregation::{fetch_interaction_counts_batch, InteractionCounts};
use crate::components::{
    CommunityPostCard, CommunityPostCardSkeleton, CommunityPostComposerInline,
    UserRoleBadge, JoinButton, ClientInitializing,
};
use crate::hooks::use_infinite_scroll;
use std::collections::{HashMap, HashSet};

/// Community page tabs
#[derive(Clone, Copy, PartialEq, Eq)]
enum CommunityTab {
    Posts,
    Pending,
    About,
}

#[component]
pub fn CommunityPage(a_tag: String) -> Element {
    let mut community = use_signal(|| None::<Community>);
    let mut posts = use_signal(|| Vec::<CommunityPost>::new());
    let mut thread_tree = use_signal(|| Vec::<CommunityThread>::new());
    let mut pending_posts = use_signal(|| Vec::<CommunityPost>::new());
    let mut loading_community = use_signal(|| true);
    let mut loading_posts = use_signal(|| true);
    let mut loading_pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut active_tab = use_signal(|| CommunityTab::Posts);
    let mut has_more = use_signal(|| true);
    let mut refresh_trigger = use_signal(|| 0u32);

    // Interaction counts for posts
    let mut interaction_counts = use_signal(|| HashMap::<String, InteractionCounts>::new());

    // Pagination state
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut pagination_loading = use_signal(|| false);

    // View mode: flat or threaded
    let mut show_threaded = use_signal(|| true);

    let has_signer = *HAS_SIGNER.read();
    let current_pubkey = auth_store::get_pubkey();
    let current_pubkey_for_role = current_pubkey.clone();
    let current_pubkey_for_mod = current_pubkey.clone();

    // Compute user role
    let user_role = use_memo(move || {
        if let (Some(comm), Some(pk)) = (community.read().as_ref(), current_pubkey_for_role.as_ref()) {
            get_user_role(pk, comm)
        } else {
            UserRole::Visitor
        }
    });

    // Check if user can moderate
    let is_moderator = use_memo(move || {
        if let (Some(comm), Some(pk)) = (community.read().as_ref(), current_pubkey_for_mod.as_ref()) {
            can_moderate(pk, comm)
        } else {
            false
        }
    });

    // Compute membership status for JoinButton
    let membership_status = use_memo(move || {
        if let (Some(comm), Some(pk)) = (community.read().as_ref(), auth_store::get_pubkey().as_ref()) {
            get_membership_status(pk, comm)
        } else {
            MembershipStatus::None
        }
    });

    let a_tag_for_community = a_tag.clone();
    let a_tag_for_posts = a_tag.clone();

    // Fetch community metadata
    use_effect(move || {
        let _ = refresh_trigger.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        let a_tag_str = a_tag_for_community.clone();
        loading_community.set(true);
        error.set(None);

        spawn(async move {
            match fetch_community_by_a_tag(&a_tag_str).await {
                Ok(Some(comm)) => {
                    community.set(Some(comm));
                    loading_community.set(false);
                }
                Ok(None) => {
                    error.set(Some("Community not found".to_string()));
                    loading_community.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading_community.set(false);
                }
            }
        });
    });

    // Fetch community posts when community is loaded
    use_effect(move || {
        let _trigger = *refresh_trigger.read();
        let _a_tag_str = a_tag_for_posts.clone();

        if let Some(comm) = community.read().as_ref() {
            let community_clone = comm.clone();
            loading_posts.set(true);

            spawn(async move {
                match fetch_community_posts(&community_clone, 50, false, None).await {
                    Ok(community_posts) => {
                        // Track oldest timestamp for pagination
                        if let Some(last) = community_posts.last() {
                            oldest_timestamp.set(Some(last.created_at));
                        }
                        has_more.set(community_posts.len() >= 50);

                        // Build thread tree
                        let tree = build_community_thread_tree(community_posts.clone());
                        thread_tree.set(tree);

                        // Prefetch profiles for all authors
                        let pubkeys: Vec<String> = community_posts
                            .iter()
                            .map(|p| p.pubkey.clone())
                            .collect::<HashSet<_>>()
                            .into_iter()
                            .collect();

                        if !pubkeys.is_empty() {
                            spawn(async move {
                                if let Err(e) = fetch_profiles_batch(pubkeys).await {
                                    log::warn!("Failed to prefetch profiles: {}", e);
                                }
                            });
                        }

                        // Fetch interaction counts
                        let event_ids: Vec<nostr_sdk::EventId> = community_posts
                            .iter()
                            .filter_map(|p| nostr_sdk::EventId::from_hex(&p.id).ok())
                            .collect();

                        if !event_ids.is_empty() {
                            spawn(async move {
                                match fetch_interaction_counts_batch(event_ids, std::time::Duration::from_secs(5)).await {
                                    Ok(counts) => {
                                        interaction_counts.set(counts);
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to fetch interaction counts: {}", e);
                                    }
                                }
                            });
                        }

                        posts.set(community_posts);
                        loading_posts.set(false);
                    }
                    Err(e) => {
                        log::error!("Failed to load posts: {}", e);
                        loading_posts.set(false);
                    }
                }
            });
        }
    });

    // Fetch pending posts for moderators
    use_effect(move || {
        if *is_moderator.read() && *active_tab.read() == CommunityTab::Pending {
            if let Some(comm) = community.read().as_ref() {
                let community_clone = comm.clone();
                loading_pending.set(true);

                spawn(async move {
                    match fetch_pending_posts(&community_clone).await {
                        Ok(pending) => {
                            pending_posts.set(pending);
                            loading_pending.set(false);
                        }
                        Err(e) => {
                            log::error!("Failed to load pending posts: {}", e);
                            loading_pending.set(false);
                        }
                    }
                });
            }
        }
    });

    // Load more function for infinite scroll
    let load_more = move || {
        if *pagination_loading.peek() || !*has_more.peek() {
            return;
        }

        if let Some(comm) = community.peek().as_ref() {
            let community_clone = comm.clone();
            pagination_loading.set(true);
            let until = *oldest_timestamp.peek();

            spawn(async move {
                // Fetch more posts with until parameter for proper relay-side pagination
                match fetch_community_posts(&community_clone, 50, false, until).await {
                    Ok(new_posts) => {
                        if new_posts.is_empty() {
                            has_more.set(false);
                        } else {
                            // Update oldest timestamp
                            if let Some(last) = new_posts.last() {
                                oldest_timestamp.set(Some(last.created_at));
                            }

                            // Append to existing posts
                            posts.write().extend(new_posts.clone());

                            // Rebuild thread tree with all posts
                            let all_posts = posts.read().clone();
                            let tree = build_community_thread_tree(all_posts);
                            thread_tree.set(tree);

                            // Fetch profiles for new authors
                            let new_pubkeys: Vec<String> = new_posts
                                .iter()
                                .map(|p| p.pubkey.clone())
                                .collect::<HashSet<_>>()
                                .into_iter()
                                .collect();

                            if !new_pubkeys.is_empty() {
                                spawn(async move {
                                    let _ = fetch_profiles_batch(new_pubkeys).await;
                                });
                            }
                        }
                        pagination_loading.set(false);
                    }
                    Err(e) => {
                        log::error!("Failed to load more posts: {}", e);
                        pagination_loading.set(false);
                    }
                }
            });
        }
    };

    // Set up infinite scroll sentinel
    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);

    // Handle post success - refresh posts
    let on_post_success = move |_event_id: String| {
        refresh_trigger.set(refresh_trigger() + 1);
    };

    // Flatten thread tree for display (with depth info)
    let flattened_posts = use_memo(move || {
        if *show_threaded.read() {
            flatten_thread_tree(thread_tree.read().clone())
        } else {
            // Flat view: just posts sorted by time
            posts.read().iter().map(|p| (p.clone(), 0usize)).collect()
        }
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center gap-4",
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition",
                        onclick: move |_| {
                            let nav = navigator();
                            nav.go_back();
                        },
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
                            line { x1: "19", y1: "12", x2: "5", y2: "12" }
                            polyline { points: "12 19 5 12 12 5" }
                        }
                    }
                    if let Some(comm) = community.read().as_ref() {
                        div {
                            class: "flex-1 min-w-0",
                            div {
                                class: "flex items-center gap-2",
                                h2 {
                                    class: "text-xl font-bold truncate",
                                    "{comm.name.as_ref().unwrap_or(&comm.d_tag)}"
                                }
                                UserRoleBadge {
                                    role: user_role.read().clone()
                                }
                            }
                            if !posts.read().is_empty() {
                                p {
                                    class: "text-sm text-muted-foreground",
                                    "{posts.read().len()} posts"
                                }
                            }
                        }
                    }
                }
            }

            // Community Info/Banner
            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading_community.read() && community.read().is_none()) {
                ClientInitializing {}
            } else if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-4",
                    div {
                        class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "{err}"
                    }
                }
            } else if let Some(comm) = community.read().as_ref() {
                // Community header
                div {
                    class: "border-b border-border p-4",

                    // Banner (if available)
                    if let Some(banner) = &comm.banner {
                        div {
                            class: "mb-4 -mx-4 -mt-4",
                            img {
                                class: "w-full h-32 object-cover",
                                src: "{banner}",
                                alt: "Community banner"
                            }
                        }
                    }

                    div {
                        class: "flex items-start gap-3 mb-3",

                        // Community avatar
                        if let Some(image_url) = &comm.image {
                            img {
                                class: "w-16 h-16 rounded-full object-cover",
                                src: "{image_url}",
                                alt: "Community image"
                            }
                        } else {
                            div {
                                class: "w-16 h-16 rounded-full bg-gradient-to-br from-purple-400 to-blue-500 flex items-center justify-center text-white",
                                svg {
                                    class: "w-8 h-8",
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

                        // Community info
                        div {
                            class: "flex-1",
                            h1 {
                                class: "text-2xl font-bold mb-1",
                                "{comm.name.as_ref().unwrap_or(&comm.d_tag)}"
                            }
                            p {
                                class: "text-muted-foreground text-sm",
                                "{comm.d_tag}"
                            }
                        }
                    }

                    // Description
                    if let Some(desc) = &comm.description {
                        p {
                            class: "text-sm mb-3 whitespace-pre-wrap",
                            "{desc}"
                        }
                    }

                    // Stats and Join Button row
                    div {
                        class: "flex items-center justify-between gap-4",

                        // Stats
                        div {
                            class: "flex gap-4 text-sm text-muted-foreground",
                            if !comm.moderators.is_empty() {
                                span {
                                    class: "flex items-center gap-1",
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
                                        path { d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10" }
                                    }
                                    "{comm.moderators.len()} moderator"
                                    if comm.moderators.len() != 1 { "s" }
                                }
                            }
                        }

                        // Join Button
                        JoinButton {
                            community: comm.clone(),
                            membership_status: membership_status.read().clone()
                        }
                    }
                }

                // Tabs
                {
                    let posts_tab_class = if *active_tab.read() == CommunityTab::Posts {
                        "flex-1 py-3 text-center font-medium transition border-b-2 border-blue-500 text-blue-500"
                    } else {
                        "flex-1 py-3 text-center font-medium transition border-b-2 border-transparent hover:bg-accent"
                    };
                    let pending_tab_class = if *active_tab.read() == CommunityTab::Pending {
                        "flex-1 py-3 text-center font-medium transition border-b-2 border-blue-500 text-blue-500"
                    } else {
                        "flex-1 py-3 text-center font-medium transition border-b-2 border-transparent hover:bg-accent"
                    };
                    let about_tab_class = if *active_tab.read() == CommunityTab::About {
                        "flex-1 py-3 text-center font-medium transition border-b-2 border-blue-500 text-blue-500"
                    } else {
                        "flex-1 py-3 text-center font-medium transition border-b-2 border-transparent hover:bg-accent"
                    };

                    rsx! {
                        div {
                            class: "border-b border-border",
                            div {
                                class: "flex",

                                // Posts tab
                                button {
                                    class: "{posts_tab_class}",
                                    onclick: move |_| active_tab.set(CommunityTab::Posts),
                                    "Posts"
                                }

                                // Pending tab (only for moderators)
                                if *is_moderator.read() {
                                    button {
                                        class: "{pending_tab_class}",
                                        onclick: move |_| active_tab.set(CommunityTab::Pending),
                                        span {
                                            class: "flex items-center justify-center gap-2",
                                            "Pending"
                                            if !pending_posts.read().is_empty() {
                                                span {
                                                    class: "px-1.5 py-0.5 bg-yellow-500 text-white text-xs rounded-full",
                                                    "{pending_posts.read().len()}"
                                                }
                                            }
                                        }
                                    }
                                }

                                // About tab
                                button {
                                    class: "{about_tab_class}",
                                    onclick: move |_| active_tab.set(CommunityTab::About),
                                    "About"
                                }
                            }
                        }
                    }
                }

                // Tab content
                match *active_tab.read() {
                    CommunityTab::Posts => rsx! {
                        // Post composer (only for signed-in users)
                        if has_signer {
                            CommunityPostComposerInline {
                                community: comm.clone(),
                                on_success: on_post_success
                            }
                        }

                        // View toggle
                        if !posts.read().is_empty() {
                            div {
                                class: "px-4 py-2 border-b border-border flex items-center justify-between",
                                span {
                                    class: "text-sm text-muted-foreground",
                                    if *show_threaded.read() {
                                        "Threaded view"
                                    } else {
                                        "Flat view"
                                    }
                                }
                                button {
                                    class: "text-sm text-blue-500 hover:underline",
                                    onclick: move |_| {
                                        let current = *show_threaded.read();
                                        show_threaded.set(!current);
                                    },
                                    if *show_threaded.read() {
                                        "Switch to flat"
                                    } else {
                                        "Switch to threaded"
                                    }
                                }
                            }
                        }

                        // Posts list
                        if *loading_posts.read() && posts.read().is_empty() {
                            div {
                                class: "divide-y divide-border",
                                for _ in 0..5 {
                                    CommunityPostCardSkeleton {}
                                }
                            }
                        } else if !posts.read().is_empty() {
                            div {
                                class: "divide-y divide-border",
                                for (post, depth) in flattened_posts.read().iter() {
                                    {
                                        let counts = interaction_counts.read().get(&post.id).cloned();
                                        rsx! {
                                            CommunityPostCard {
                                                key: "{post.id}",
                                                post: post.clone(),
                                                community: comm.clone(),
                                                depth: *depth,
                                                interaction_counts: counts,
                                                show_actions: true,
                                                show_moderation: *is_moderator.read(),
                                                on_reply_success: on_post_success
                                            }
                                        }
                                    }
                                }
                            }

                            // Infinite scroll sentinel
                            if *has_more.read() {
                                div {
                                    id: "{sentinel_id}",
                                    class: "h-4"
                                }
                            }

                            // Pagination loading indicator
                            if *pagination_loading.read() {
                                div {
                                    class: "flex justify-center py-4",
                                    span {
                                        class: "inline-block w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin"
                                    }
                                }
                            }

                            // End indicator
                            if !*has_more.read() && !*pagination_loading.read() {
                                div {
                                    class: "p-8 text-center text-muted-foreground",
                                    "You've reached the end"
                                }
                            }
                        } else if !*loading_posts.read() {
                            // Empty state
                            div {
                                class: "text-center py-12",
                                svg {
                                    class: "w-16 h-16 mx-auto mb-4 text-muted-foreground",
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
                                h3 {
                                    class: "text-xl font-semibold mb-2",
                                    "No posts yet"
                                }
                                p {
                                    class: "text-muted-foreground",
                                    if has_signer {
                                        "Be the first to post in this community!"
                                    } else {
                                        "Sign in to post to this community"
                                    }
                                }
                            }
                        }
                    },

                    CommunityTab::Pending => rsx! {
                        if *loading_pending.read() {
                            div {
                                class: "divide-y divide-border",
                                for _ in 0..3 {
                                    CommunityPostCardSkeleton {}
                                }
                            }
                        } else if !pending_posts.read().is_empty() {
                            div {
                                class: "divide-y divide-border",
                                for post in pending_posts.read().iter() {
                                    CommunityPostCard {
                                        key: "{post.id}",
                                        post: post.clone(),
                                        community: comm.clone(),
                                        depth: 0,
                                        show_actions: true,
                                        show_moderation: true
                                    }
                                }
                            }
                        } else {
                            div {
                                class: "text-center py-12",
                                svg {
                                    class: "w-16 h-16 mx-auto mb-4 text-muted-foreground",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M22 11.08V12a10 10 0 1 1-5.93-9.14" }
                                    polyline { points: "22 4 12 14.01 9 11.01" }
                                }
                                h3 {
                                    class: "text-xl font-semibold mb-2",
                                    "All caught up!"
                                }
                                p {
                                    class: "text-muted-foreground",
                                    "No pending posts to review"
                                }
                            }
                        }
                    },

                    CommunityTab::About => rsx! {
                        div {
                            class: "p-4 space-y-6",

                            // Description section
                            if let Some(desc) = &comm.description {
                                div {
                                    h3 {
                                        class: "text-lg font-semibold mb-2",
                                        "About"
                                    }
                                    p {
                                        class: "text-muted-foreground whitespace-pre-wrap",
                                        "{desc}"
                                    }
                                }
                            }

                            // Rules section
                            if let Some(rules) = &comm.rules {
                                div {
                                    h3 {
                                        class: "text-lg font-semibold mb-2",
                                        "Rules"
                                    }
                                    p {
                                        class: "text-muted-foreground whitespace-pre-wrap",
                                        "{rules}"
                                    }
                                }
                            }

                            // Moderators section
                            if !comm.moderators.is_empty() {
                                div {
                                    h3 {
                                        class: "text-lg font-semibold mb-2",
                                        "Moderators"
                                    }
                                    div {
                                        class: "space-y-2",
                                        for mod_pubkey in comm.moderators.iter() {
                                            {
                                                let profile = get_cached_profile(mod_pubkey);
                                                let display_name = profile
                                                    .as_ref()
                                                    .and_then(|p| p.display_name.clone().or(p.name.clone()))
                                                    .unwrap_or_else(|| format!("{}...", &mod_pubkey[..16.min(mod_pubkey.len())]));
                                                let avatar = profile.as_ref().and_then(|p| p.picture.clone());

                                                rsx! {
                                                    div {
                                                        key: "{mod_pubkey}",
                                                        class: "flex items-center gap-2 p-2 bg-accent/50 rounded-lg",
                                                        if let Some(pic) = avatar {
                                                            img {
                                                                class: "w-8 h-8 rounded-full object-cover",
                                                                src: "{pic}",
                                                                alt: "Moderator"
                                                            }
                                                        } else {
                                                            div {
                                                                class: "w-8 h-8 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white text-sm",
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
                                                                    path { d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10" }
                                                                }
                                                            }
                                                        }
                                                        span {
                                                            class: "text-sm font-medium truncate",
                                                            "{display_name}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Technical info
                            {
                                let owner_profile = get_cached_profile(&comm.pubkey);
                                let owner_name = owner_profile
                                    .as_ref()
                                    .and_then(|p| p.display_name.clone().or(p.name.clone()))
                                    .unwrap_or_else(|| format!("{}...", &comm.pubkey[..16.min(comm.pubkey.len())]));
                                let id_truncated = if comm.id.len() > 16 {
                                    format!("{}...", &comm.id[..16])
                                } else {
                                    comm.id.clone()
                                };
                                rsx! {
                                    div {
                                        h3 {
                                            class: "text-lg font-semibold mb-2",
                                            "Details"
                                        }
                                        div {
                                            class: "space-y-2 text-sm",
                                            div {
                                                class: "flex justify-between",
                                                span { class: "text-muted-foreground", "Identifier" }
                                                span { class: "font-mono", "{comm.d_tag}" }
                                            }
                                            div {
                                                class: "flex justify-between",
                                                span { class: "text-muted-foreground", "Owner" }
                                                span { class: "truncate max-w-[200px]", "{owner_name}" }
                                            }
                                            div {
                                                class: "flex justify-between",
                                                span { class: "text-muted-foreground", "Event ID" }
                                                span { class: "font-mono truncate max-w-[200px]", "{id_truncated}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
