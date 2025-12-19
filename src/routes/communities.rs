//! Communities List Page
//! Displays NIP-72 moderated communities with:
//! - Pinned communities at top
//! - User's communities sorted by role (Owner > Moderator > Member > Pending)
//! - Discover section for other communities
//! - Real relay search
//! - Infinite scroll pagination

use dioxus::prelude::*;
use crate::stores::community_store::{self, Community, MembershipStatus, CommunityWithMembership};
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::auth_store;
use crate::stores::pinned_communities::{self, get_pinned_communities_set};
use crate::components::{CommunityCard, CommunityCardWithMembership, CommunityCardSkeleton, ClientInitializing};
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;
use std::collections::HashSet;

#[component]
pub fn Communities() -> Element {
    // Main communities list
    let mut communities = use_signal(|| Vec::<Community>::new());
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // User's communities with membership info (sorted by role)
    let mut user_communities_with_membership = use_signal(|| Vec::<CommunityWithMembership>::new());
    let mut user_communities_loading = use_signal(|| false);

    // Pinned communities
    let mut pinned_communities = use_signal(|| Vec::<CommunityWithMembership>::new());
    let mut pinned_loading = use_signal(|| false);

    // Search state
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(|| None::<Vec<Community>>);
    let mut search_loading = use_signal(|| false);
    let mut search_version = use_signal(|| 0u64);

    // Pagination state for infinite scroll
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut pagination_loading = use_signal(|| false);

    // Refresh trigger for re-fetching
    let refresh_trigger = use_signal(|| 0);

    // Initialize pinned communities on mount (if signed in)
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = *HAS_SIGNER.read();
        if !client_initialized || !has_signer {
            return;
        }

        spawn(async move {
            if let Err(e) = pinned_communities::init_pinned_communities().await {
                log::warn!("Failed to initialize pinned communities: {}", e);
            }
        });
    });

    // Fetch user's communities with membership info on mount (if signed in)
    use_effect(move || {
        let _ = refresh_trigger.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = *HAS_SIGNER.read();
        if !client_initialized || !has_signer {
            return;
        }

        if let Some(pubkey) = auth_store::get_pubkey() {
            user_communities_loading.set(true);
            pinned_loading.set(true);

            spawn(async move {
                // Fetch user's pending join requests first
                if let Err(e) = community_store::fetch_user_join_requests(&pubkey).await {
                    log::warn!("Failed to fetch user join requests: {}", e);
                }

                match community_store::fetch_user_communities(&pubkey).await {
                    Ok(comms) => {
                        // Get pinned set for sorting
                        let pinned_set = get_pinned_communities_set();

                        // Deduplicate communities (relays may return duplicates)
                        let mut seen = HashSet::new();
                        let deduped: Vec<_> = comms
                            .into_iter()
                            .filter(|c| seen.insert(c.a_tag.clone()))
                            .collect();

                        // Sort communities by membership status
                        let sorted = community_store::sort_communities_by_membership(
                            deduped,
                            Some(pubkey.as_str()),
                            &pinned_set,
                        );

                        // Separate pinned from user communities
                        let (pinned, user): (Vec<_>, Vec<_>) = sorted
                            .into_iter()
                            .partition(|c| c.is_pinned);

                        // Filter user communities to only those where user has a role
                        let user_with_roles: Vec<_> = user
                            .into_iter()
                            .filter(|c| !matches!(c.membership_status, MembershipStatus::None))
                            .collect();

                        pinned_communities.set(pinned);
                        user_communities_with_membership.set(user_with_roles);
                        user_communities_loading.set(false);
                        pinned_loading.set(false);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch user communities: {}", e);
                        user_communities_loading.set(false);
                        pinned_loading.set(false);
                    }
                }
            });
        }
    });

    // Fetch initial communities on mount
    use_effect(move || {
        let _ = refresh_trigger.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);

        spawn(async move {
            match community_store::fetch_communities_page(50, None).await {
                Ok(comms) => {
                    // Track oldest timestamp for pagination
                    if let Some(last) = comms.last() {
                        oldest_timestamp.set(Some(last.created_at));
                    }
                    communities.set(comms);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Debounced search effect
    use_effect(move || {
        let query = search_query.read().clone();

        // Reset search if query is too short
        if query.len() < 2 {
            search_results.set(None);
            search_loading.set(false);
            return;
        }

        // Increment version to invalidate previous searches
        let version = search_version.with_mut(|v| { *v += 1; *v });
        search_loading.set(true);

        spawn(async move {
            // Debounce 300ms
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(300).await;

            // Check if this search is still current
            if *search_version.peek() != version {
                return;
            }

            match community_store::search_communities(&query, 50).await {
                Ok(results) => {
                    // Check again after async operation
                    if *search_version.peek() == version {
                        search_results.set(Some(results));
                        search_loading.set(false);
                    }
                }
                Err(e) => {
                    log::error!("Search failed: {}", e);
                    if *search_version.peek() == version {
                        search_loading.set(false);
                    }
                }
            }
        });
    });

    // Load more function for infinite scroll
    let load_more = move || {
        if *pagination_loading.peek() || !*has_more.peek() {
            return;
        }

        // Don't paginate while searching
        if search_results.peek().is_some() {
            return;
        }

        pagination_loading.set(true);
        let until = *oldest_timestamp.peek();

        spawn(async move {
            match community_store::fetch_communities_page(50, until).await {
                Ok(new_communities) => {
                    if new_communities.is_empty() {
                        has_more.set(false);
                    } else {
                        // Update oldest timestamp for next page
                        if let Some(last) = new_communities.last() {
                            oldest_timestamp.set(Some(last.created_at));
                        }
                        // Append to existing communities
                        communities.write().extend(new_communities);
                    }
                    pagination_loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more communities: {}", e);
                    pagination_loading.set(false);
                }
            }
        });
    };

    // Set up infinite scroll sentinel
    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);

    // Determine which communities to display in Discover section
    let display_communities = use_memo(move || {
        // If searching, show search results (deduplicated)
        if let Some(results) = search_results.read().as_ref() {
            let mut seen = HashSet::new();
            return results.iter()
                .filter(|c| seen.insert(c.a_tag.clone()))
                .cloned()
                .collect();
        }

        // Build set of a_tags to exclude (pinned + user's communities)
        let mut excluded_a_tags: HashSet<String> = HashSet::new();

        // Exclude pinned communities
        for c in pinned_communities.read().iter() {
            excluded_a_tags.insert(c.community.a_tag.clone());
        }

        // Exclude user's communities (with membership)
        for c in user_communities_with_membership.read().iter() {
            excluded_a_tags.insert(c.community.a_tag.clone());
        }

        let all_communities = communities.read();

        // Filter and deduplicate (relays may return duplicates)
        let mut seen = HashSet::new();
        let filtered: Vec<_> = all_communities
            .iter()
            .filter(|c| !excluded_a_tags.contains(&c.a_tag))
            .filter(|c| seen.insert(c.a_tag.clone()))
            .cloned()
            .collect();

        log::info!("display_communities: total={}, excluded={}, filtered={}",
            all_communities.len(), excluded_a_tags.len(), filtered.len());

        filtered
    });

    // Check if we're in search mode
    let is_searching = search_query.read().len() >= 2;

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
                            "Communities"
                        }
                        // Create Community button (only if signed in)
                        if *HAS_SIGNER.read() {
                            Link {
                                to: Route::CommunityNew {},
                                class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                                "+ Create"
                            }
                        }
                    }
                    p {
                        class: "text-sm text-muted-foreground mb-3",
                        "Discover NIP-72 moderated communities and join the conversation"
                    }

                    // Search
                    div {
                        class: "relative",
                        svg {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "11", cy: "11", r: "8" }
                            line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                        }
                        input {
                            class: "w-full pl-10 pr-4 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-blue-500",
                            r#type: "text",
                            placeholder: "Search communities...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value())
                        }
                        // Search loading indicator
                        if *search_loading.read() {
                            div {
                                class: "absolute right-3 top-1/2 -translate-y-1/2",
                                span {
                                    class: "inline-block w-4 h-4 border-2 border-blue-500 border-t-transparent rounded-full animate-spin"
                                }
                            }
                        }
                    }
                }
            }

            // Content
            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && communities.read().is_empty()) {
                ClientInitializing {}
            } else if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-4",
                    div {
                        class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "{err}"
                    }
                }
            } else {
                div {
                    class: "p-4 space-y-6",

                    // Pinned Communities section (only if signed in and has pinned communities, not searching)
                    if *HAS_SIGNER.read() && !is_searching && !pinned_communities.read().is_empty() {
                        div {
                            h2 {
                                class: "text-lg font-semibold mb-3 flex items-center gap-2",
                                svg {
                                    class: "w-5 h-5 text-yellow-500",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "currentColor",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z" }
                                }
                                "Pinned Communities"
                            }
                            div {
                                class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                                for cwm in pinned_communities.read().iter() {
                                    CommunityCardWithMembership {
                                        key: "{cwm.community.a_tag}",
                                        data: cwm.clone()
                                    }
                                }
                            }
                        }
                    }

                    // Pinned communities loading skeleton
                    if *HAS_SIGNER.read() && !is_searching && *pinned_loading.read() && pinned_communities.read().is_empty() {
                        div {
                            h2 {
                                class: "text-lg font-semibold mb-3 flex items-center gap-2",
                                svg {
                                    class: "w-5 h-5 text-yellow-500",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "currentColor",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z" }
                                }
                                "Pinned Communities"
                            }
                            div {
                                class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                                for _ in 0..2 {
                                    CommunityCardSkeleton {}
                                }
                            }
                        }
                    }

                    // Your Communities section (only if signed in and has communities, not searching)
                    if *HAS_SIGNER.read() && !is_searching && !user_communities_with_membership.read().is_empty() {
                        div {
                            h2 {
                                class: "text-lg font-semibold mb-3 flex items-center gap-2",
                                svg {
                                    class: "w-5 h-5 text-blue-500",
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
                                "Your Communities"
                            }
                            div {
                                class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                                for cwm in user_communities_with_membership.read().iter() {
                                    CommunityCardWithMembership {
                                        key: "{cwm.community.a_tag}",
                                        data: cwm.clone()
                                    }
                                }
                            }
                        }
                    }

                    // User communities loading skeleton
                    if *HAS_SIGNER.read() && !is_searching && *user_communities_loading.read() && user_communities_with_membership.read().is_empty() {
                        div {
                            h2 {
                                class: "text-lg font-semibold mb-3",
                                "Your Communities"
                            }
                            div {
                                class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                                for _ in 0..3 {
                                    CommunityCardSkeleton {}
                                }
                            }
                        }
                    }

                    // Discover / Search Results section
                    div {
                        h2 {
                            class: "text-lg font-semibold mb-3",
                            if is_searching {
                                "Search Results"
                            } else {
                                "Discover Communities"
                            }
                        }

                        if display_communities.read().is_empty() {
                            div {
                                class: "flex flex-col items-center justify-center py-12 px-4 text-center",
                                svg {
                                    class: "w-12 h-12 mb-4 text-muted-foreground",
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
                                h3 {
                                    class: "text-lg font-medium mb-1",
                                    if is_searching {
                                        "No communities found"
                                    } else {
                                        "No communities available"
                                    }
                                }
                                p {
                                    class: "text-muted-foreground text-sm",
                                    if is_searching {
                                        "Try a different search term"
                                    } else {
                                        "Connect to more relays to discover communities"
                                    }
                                }
                            }
                        } else {
                            div {
                                class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                                for community in display_communities.read().iter() {
                                    CommunityCard {
                                        key: "{community.a_tag}",
                                        community: community.clone()
                                    }
                                }
                            }

                            // Infinite scroll sentinel (only when not searching)
                            if !is_searching && *has_more.read() {
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
                        }
                    }
                }
            }
        }
    }
}
