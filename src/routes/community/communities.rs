//! Communities List Page
//! Displays NIP-72 moderated communities with:
//! - Pinned communities at top
//! - User's communities sorted by role (Owner > Moderator > Member > Pending)
//! - Discover section for other communities
//! - Real relay search
//! - Infinite scroll pagination
use crate::components::{
    ClientInitializing, CommunityCard, CommunityCardSkeleton, CommunityCardWithMembership,
};
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;
use crate::stores::auth_store;
use crate::stores::community_store::{
    self, Community, CommunityWithMembership, MembershipStatus,
};
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::pinned_communities::{self, get_pinned_communities_set};
use dioxus::prelude::*;
use std::collections::HashSet;
#[component]
pub fn Communities() -> Element {
    let mut communities = use_signal(Vec::<Community>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut user_communities_with_membership = use_signal(
        Vec::<CommunityWithMembership>::new,
    );
    let mut user_communities_loading = use_signal(|| false);
    let mut pinned_communities = use_signal(Vec::<CommunityWithMembership>::new);
    let mut pinned_loading = use_signal(|| false);
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(|| None::<Vec<Community>>);
    let mut search_loading = use_signal(|| false);
    let mut search_version = use_signal(|| 0u64);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut pagination_loading = use_signal(|| false);
    let refresh_trigger = use_signal(|| 0);
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
                if let Err(e) = community_store::fetch_user_join_requests(&pubkey).await
                {
                    log::warn!("Failed to fetch user join requests: {}", e);
                }
                match community_store::fetch_user_communities(&pubkey).await {
                    Ok(comms) => {
                        let pinned_set = get_pinned_communities_set();
                        let mut seen = HashSet::new();
                        let deduped: Vec<_> = comms
                            .into_iter()
                            .filter(|c| seen.insert(c.a_tag.clone()))
                            .collect();
                        let sorted = community_store::sort_communities_by_membership(
                            deduped,
                            Some(pubkey.as_str()),
                            &pinned_set,
                        );
                        let (pinned, user): (Vec<_>, Vec<_>) = sorted
                            .into_iter()
                            .partition(|c| c.is_pinned);
                        let user_with_roles: Vec<_> = user
                            .into_iter()
                            .filter(|c| {
                                !matches!(c.membership_status, MembershipStatus::None)
                            })
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
    use_effect(move || {
        let query = search_query.read().clone();
        if query.len() < 2 {
            search_results.set(None);
            search_loading.set(false);
            return;
        }
        let version = search_version
            .with_mut(|v| {
                *v += 1;
                *v
            });
        search_loading.set(true);
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(300).await;
            if *search_version.peek() != version {
                return;
            }
            match community_store::search_communities(&query, 50).await {
                Ok(results) => {
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
    let load_more = move || {
        if *pagination_loading.peek() || !*has_more.peek() {
            return;
        }
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
                        if let Some(last) = new_communities.last() {
                            oldest_timestamp.set(Some(last.created_at));
                        }
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
    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);
    let display_communities = use_memo(move || {
        if let Some(results) = search_results.read().as_ref() {
            let mut seen = HashSet::new();
            return results
                .iter()
                .filter(|c| seen.insert(c.a_tag.clone()))
                .cloned()
                .collect();
        }
        let mut excluded_a_tags: HashSet<String> = HashSet::new();
        for c in pinned_communities.read().iter() {
            excluded_a_tags.insert(c.community.a_tag.clone());
        }
        for c in user_communities_with_membership.read().iter() {
            excluded_a_tags.insert(c.community.a_tag.clone());
        }
        let all_communities = communities.read();
        let mut seen = HashSet::new();
        let filtered: Vec<_> = all_communities
            .iter()
            .filter(|c| !excluded_a_tags.contains(&c.a_tag))
            .filter(|c| seen.insert(c.a_tag.clone()))
            .cloned()
            .collect();
        log::info!(
            "display_communities: total={}, excluded={}, filtered={}", all_communities
            .len(), excluded_a_tags.len(), filtered.len()
        );
        filtered
    });
    let is_searching = search_query.read().len() >= 2;
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3",
                    div { class: "flex items-center justify-between mb-3",
                        h1 { class: "text-xl font-bold flex items-center gap-2",
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
                        if *HAS_SIGNER.read() {
                            Link {
                                to: Route::CommunityNew {},
                                class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                                "+ Create"
                            }
                        }
                    }
                    p { class: "text-sm text-muted-foreground mb-3",
                        "Discover NIP-72 moderated communities and join the conversation"
                    }
                    div { class: "relative",
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
                            line {
                                x1: "21",
                                y1: "21",
                                x2: "16.65",
                                y2: "16.65",
                            }
                        }
                        input {
                            class: "w-full pl-10 pr-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                            r#type: "text",
                            placeholder: "Search communities...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                        if *search_loading.read() {
                            div { class: "absolute right-3 top-1/2 -translate-y-1/2",
                                span { class: "inline-block w-4 h-4 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" }
                            }
                        }
                    }
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read()
                || (*loading.read() && communities.read().is_empty())
            {
                ClientInitializing {}
            } else if let Some(err) = error.read().as_ref() {
                div { class: "p-4",
                    div { class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "{err}"
                    }
                }
            } else {
                div { class: "p-4 space-y-6",
                    if *HAS_SIGNER.read() && !is_searching && !pinned_communities.read().is_empty() {
                        div {
                            h2 { class: "text-lg font-semibold mb-3 flex items-center gap-2",
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
                            div { class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                                for cwm in pinned_communities.read().iter() {
                                    CommunityCardWithMembership {
                                        key: "{cwm.community.a_tag}",
                                        data: cwm.clone(),
                                    }
                                }
                            }
                        }
                    }
                    if *HAS_SIGNER.read() && !is_searching && *pinned_loading.read()
                        && pinned_communities.read().is_empty()
                    {
                        div {
                            h2 { class: "text-lg font-semibold mb-3 flex items-center gap-2",
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
                            div { class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                                for _ in 0..2 {
                                    CommunityCardSkeleton {}
                                }
                            }
                        }
                    }
                    if *HAS_SIGNER.read() && !is_searching
                        && !user_communities_with_membership.read().is_empty()
                    {
                        div {
                            h2 { class: "text-lg font-semibold mb-3 flex items-center gap-2",
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
                            div { class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                                for cwm in user_communities_with_membership.read().iter() {
                                    CommunityCardWithMembership {
                                        key: "{cwm.community.a_tag}",
                                        data: cwm.clone(),
                                    }
                                }
                            }
                        }
                    }
                    if *HAS_SIGNER.read() && !is_searching && *user_communities_loading.read()
                        && user_communities_with_membership.read().is_empty()
                    {
                        div {
                            h2 { class: "text-lg font-semibold mb-3", "Your Communities" }
                            div { class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                                for _ in 0..3 {
                                    CommunityCardSkeleton {}
                                }
                            }
                        }
                    }
                    div {
                        h2 { class: "text-lg font-semibold mb-3",
                            if is_searching {
                                "Search Results"
                            } else {
                                "Discover Communities"
                            }
                        }
                        if display_communities.read().is_empty() {
                            div { class: "flex flex-col items-center justify-center py-12 px-4 text-center",
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
                                h3 { class: "text-lg font-medium mb-1",
                                    if is_searching {
                                        "No communities found"
                                    } else {
                                        "No communities available"
                                    }
                                }
                                p { class: "text-muted-foreground text-sm",
                                    if is_searching {
                                        "Try a different search term"
                                    } else {
                                        "Connect to more relays to discover communities"
                                    }
                                }
                            }
                        } else {
                            div { class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                                for community in display_communities.read().iter() {
                                    CommunityCard {
                                        key: "{community.a_tag}",
                                        community: community.clone(),
                                    }
                                }
                            }
                            if !is_searching && *has_more.read() {
                                div { id: "{sentinel_id}", class: "h-4" }
                            }
                            if *pagination_loading.read() {
                                div { class: "flex justify-center py-4",
                                    span { class: "inline-block w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
