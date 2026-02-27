//! Pack Detail Page
//!
//! Full view of a starter pack with follow-all, edit, delete, and share actions.

use std::time::Duration;

use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::prelude::*;

use crate::components::{ClientInitializing, NoteCard};
use crate::hooks::use_mute_block_cache;
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client, packs_store, profiles};
use crate::stores::packs_store::StarterPack;
use crate::utils::time::format_relative_time;
use crate::utils::validation::is_valid_http_url;
use crate::utils::{process_events_to_feed_items, truncate_pubkey, FeedItem};

#[derive(Clone, Copy, PartialEq)]
enum DetailTab {
    People,
    Posts,
}

/// Pack detail page
#[component]
pub fn PackDetail(naddr: String) -> Element {
    let mut pack = use_signal(|| None::<StarterPack>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut follow_all_status = use_signal(|| None::<String>);
    let mut following_all = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);
    let mut delete_error = use_signal(|| None::<String>);
    let mut copied = use_signal(|| false);
    let mut active_tab = use_signal(|| DetailTab::People);
    let mut posts: Signal<Vec<FeedItem>> = use_signal(Vec::new);
    let mut posts_loading = use_signal(|| false);
    let mut posts_request_id = use_signal(|| 0u32);
    let mut pack_request_id = use_signal(|| 0u32);
    let (cached_muted_posts, cached_blocked_users) = use_mute_block_cache();

    let navigator = use_navigator();
    let is_authenticated = auth_store::is_authenticated();
    let user_pubkey = auth_store::get_pubkey();

    // Load pack data
    let client_init = *nostr_client::CLIENT_INITIALIZED.read();
    use_effect(
        use_reactive(
            (&naddr, &client_init),
            move |(addr, client_initialized)| {
                if !client_initialized {
                    return;
                }
                let current_id = pack_request_id.peek().wrapping_add(1);
                pack_request_id.set(current_id);
                spawn(async move {
                    loading.set(true);
                    error.set(None);

                    match packs_store::fetch_pack_by_naddr(&addr).await {
                        Ok(Some(p)) => {
                            if *pack_request_id.peek() != current_id {
                                log::debug!("Discarding stale pack request");
                                return;
                            }
                            // Prefetch all member profiles
                            let member_pubkeys: Vec<String> =
                                p.members.iter().map(|m| m.pubkey.clone()).collect();
                            let mut to_prefetch = vec![p.author_pubkey.clone()];
                            to_prefetch.extend(member_pubkeys);
                            profiles::prefetch_profiles(to_prefetch).await;
                            if *pack_request_id.peek() != current_id {
                                log::debug!("Discarding stale pack request after prefetch");
                                return;
                            }
                            pack.set(Some(p));
                        }
                        Ok(None) => {
                            if *pack_request_id.peek() != current_id {
                                return;
                            }
                            error.set(Some("Pack not found".to_string()));
                        }
                        Err(e) => {
                            if *pack_request_id.peek() != current_id {
                                return;
                            }
                            log::error!("Failed to fetch pack: {}", e);
                            error.set(Some(e));
                        }
                    }
                    if *pack_request_id.peek() == current_id {
                        loading.set(false);
                    }
                });
            },
        ),
    );

    // Load posts when Posts tab is selected
    use_effect(move || {
        let tab = *active_tab.read();
        let pack_data_for_posts = pack.read().clone();
        if tab != DetailTab::Posts {
            return;
        }
        if let Some(p) = pack_data_for_posts {
            let current_id = posts_request_id.peek().wrapping_add(1);
            posts_request_id.set(current_id);
            posts_loading.set(true);
            spawn(async move {
                match packs_store::fetch_pack_member_posts(&p, 50, None).await {
                    Ok(events) => {
                        if *posts_request_id.peek() != current_id {
                            log::debug!("Discarding stale posts request");
                            posts.set(Vec::new());
                            posts_loading.set(false);
                            return;
                        }
                        let items = process_events_to_feed_items(events);

                        // Prefetch profiles for post authors
                        let author_pks: Vec<String> = items
                            .iter()
                            .map(|item| item.event().pubkey.to_hex())
                            .collect();
                        profiles::prefetch_profiles(author_pks).await;

                        if *posts_request_id.peek() != current_id {
                            log::debug!("Discarding stale posts after prefetch");
                            return;
                        }
                        posts.set(items);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch pack member posts: {}", e);
                        posts.set(Vec::new());
                    }
                }
                if *posts_request_id.peek() == current_id {
                    posts_loading.set(false);
                }
            });
        }
    });

    let is_author = pack
        .read()
        .as_ref()
        .map(|p| user_pubkey.as_deref() == Some(p.author_pubkey.as_str()))
        .unwrap_or(false);

    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! { ClientInitializing {} };
    }

    let pack_data = pack.read().clone();

    rsx! {
        div { class: "min-h-screen",
            // Sticky header
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center gap-4 px-4 py-3",
                    button {
                        class: "p-2 hover:bg-accent rounded-lg transition",
                        onclick: move |_| navigator.go_back(),
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7",
                            }
                        }
                    }
                    h1 { class: "text-xl font-bold", "Pack" }
                }
            }

            if *loading.read() {
                // Loading skeleton
                div { class: "p-4 space-y-4",
                    div { class: "h-60 bg-muted rounded-xl animate-pulse" }
                    div { class: "h-8 bg-muted rounded w-1/2 animate-pulse" }
                    div { class: "h-4 bg-muted rounded w-3/4 animate-pulse" }
                    div { class: "space-y-3 mt-6",
                        for _ in 0..5 {
                            div { class: "flex items-center gap-3 p-3 animate-pulse",
                                div { class: "w-12 h-12 rounded-full bg-muted" }
                                div { class: "flex-1 space-y-2",
                                    div { class: "h-4 bg-muted rounded w-1/3" }
                                    div { class: "h-3 bg-muted rounded w-2/3" }
                                }
                            }
                        }
                    }
                }
            } else if let Some(e) = error.read().as_ref() {
                div { class: "flex flex-col items-center justify-center py-16 text-center p-4",
                    div { class: "text-6xl mb-4", "⚠️" }
                    p { class: "text-muted-foreground", "{e}" }
                    Link {
                        to: Route::PacksHome {},
                        class: "text-primary hover:underline mt-4 inline-block",
                        "← Back to Packs"
                    }
                }
            } else if let Some(p) = pack_data {
                div { class: "space-y-0",
                    // Cover image hero
                    div { class: "h-60 bg-muted overflow-hidden",
                        if let Some(ref img) = p.image_url.as_ref().filter(|u| is_valid_http_url(u)) {
                            img {
                                src: "{img}",
                                alt: "{p.name}",
                                class: "w-full h-full object-cover",
                            }
                        } else {
                            div { class: "w-full h-full flex items-center justify-center",
                                span { class: "text-6xl", "📦" }
                            }
                        }
                    }

                    // Pack info
                    div { class: "p-4 space-y-4",
                        h2 { class: "text-2xl font-bold", "{p.name}" }

                        if let Some(ref desc) = p.description {
                            p { class: "text-muted-foreground", "{desc}" }
                        }

                        // Author + metadata
                        {
                            let author_profile = profiles::get_profile(&p.author_pubkey);
                            let author_name = author_profile
                                .as_ref()
                                .and_then(|pr| pr.display_name.clone().or(pr.name.clone()))
                                .unwrap_or_else(|| truncate_pubkey(&p.author_pubkey));
                            let author_picture = author_profile.as_ref().and_then(|pr| pr.picture.clone());
                            let created_ts = Timestamp::from(p.created_at);

                            rsx! {
                                div { class: "flex items-center gap-3",
                                    Link {
                                        to: Route::Profile { pubkey: p.author_pubkey.clone() },
                                        class: "flex items-center gap-2 hover:underline",
                                        if let Some(ref pic) = author_picture.as_ref().filter(|u| is_valid_http_url(u)) {
                                            img {
                                                src: "{pic}",
                                                alt: "{author_name}",
                                                class: "w-8 h-8 rounded-full",
                                            }
                                        } else {
                                            div { class: "w-8 h-8 rounded-full bg-primary/20 flex items-center justify-center",
                                                span { class: "text-sm font-bold text-primary",
                                                    "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                                                }
                                            }
                                        }
                                        span { class: "font-medium", "{author_name}" }
                                    }
                                    span { class: "text-muted-foreground", "·" }
                                    span { class: "text-sm text-muted-foreground",
                                        "{p.members.len()} people"
                                    }
                                    span { class: "text-muted-foreground", "·" }
                                    span { class: "text-sm text-muted-foreground",
                                        "{format_relative_time(created_ts)}"
                                    }
                                }
                            }
                        }

                        // Action bar
                        div { class: "flex flex-wrap gap-2 pt-2",
                            // Follow All button
                            if is_authenticated {
                                button {
                                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm font-medium disabled:opacity-50",
                                    disabled: *following_all.read(),
                                    onclick: {
                                        let pack_clone = p.clone();
                                        move |_| {
                                            following_all.set(true);
                                            follow_all_status.set(None);
                                            let pc = pack_clone.clone();
                                            spawn(async move {
                                                match packs_store::follow_all_pack_members(&pc).await {
                                                    Ok(count) => {
                                                        if count > 0 {
                                                            follow_all_status.set(Some(
                                                                format!("Followed {} new users!", count),
                                                            ));
                                                        } else {
                                                            follow_all_status.set(Some(
                                                                "Already following all members".to_string(),
                                                            ));
                                                        }
                                                    }
                                                    Err(e) => {
                                                        follow_all_status.set(Some(
                                                            format!("Error: {}", e),
                                                        ));
                                                    }
                                                }
                                                following_all.set(false);
                                            });
                                        }
                                    },
                                    if *following_all.read() {
                                        "Following..."
                                    } else {
                                        "Follow All"
                                    }
                                }
                            }

                            // Edit (author only)
                            if is_author {
                                {
                                    #[allow(unused_variables)]
                                    let edit_naddr = p.naddr.clone();
                                    rsx! {
                                        Link {
                                            to: Route::PackNew {},
                                            class: "px-4 py-2 border border-border rounded-lg hover:bg-accent transition text-sm",
                                            onclick: move |_| {
                                                #[cfg(feature = "web")]
                                                {
                                                    if let Some(window) = web_sys::window() {
                                                        if let Ok(Some(storage)) = window.local_storage() {
                                                            let _ = storage.set_item("pack_edit_naddr", &edit_naddr);
                                                        }
                                                    }
                                                }
                                            },
                                            "Edit"
                                        }
                                    }
                                }

                                // Delete
                                button {
                                    class: "px-4 py-2 border border-destructive text-destructive rounded-lg hover:bg-destructive/10 transition text-sm",
                                    onclick: move |_| {
                                        delete_error.set(None);
                                        show_delete_confirm.set(true);
                                    },
                                    "Delete"
                                }
                            }

                            // Share
                            button {
                                class: "px-4 py-2 border border-border rounded-lg hover:bg-accent transition text-sm flex items-center gap-2",
                                onclick: {
                                    let naddr_str = p.naddr.clone();
                                    move |_| {
                                        let uri = format!("nostr:{}", naddr_str);
                                        spawn(async move {
                                            match crate::utils::clipboard::copy_to_clipboard(&uri).await {
                                                Ok(()) => {
                                                    copied.set(true);
                                                    crate::platform::timer::sleep_ms(2000).await;
                                                    copied.set(false);
                                                }
                                                Err(e) => {
                                                    log::error!("Failed to copy: {:?}", e);
                                                }
                                            }
                                        });
                                    }
                                },
                                if *copied.read() { "Copied!" } else { "Share" }
                            }
                        }

                        // Follow all status message
                        if let Some(ref status) = *follow_all_status.read() {
                            div { class: "text-sm px-3 py-2 rounded-lg bg-muted", "{status}" }
                        }

                        // Delete confirmation dialog
                        if *show_delete_confirm.read() {
                            div { class: "bg-destructive/10 border border-destructive/20 rounded-xl p-4 space-y-3",
                                p { class: "font-medium text-destructive",
                                    "Are you sure you want to delete this pack?"
                                }
                                if let Some(ref err) = *delete_error.read() {
                                    p { class: "text-sm text-destructive", "{err}" }
                                }
                                div { class: "flex gap-2",
                                    button {
                                        class: "px-4 py-2 rounded-lg border border-border hover:bg-accent transition text-sm",
                                        onclick: move |_| show_delete_confirm.set(false),
                                        "Cancel"
                                    }
                                    button {
                                        class: "px-4 py-2 rounded-lg bg-destructive text-destructive-foreground hover:bg-destructive/90 transition text-sm disabled:opacity-50",
                                        disabled: *deleting.read(),
                                        onclick: {
                                            let pack_clone = p.clone();
                                            move |_| {
                                                deleting.set(true);
                                                let pc = pack_clone.clone();
                                                spawn(async move {
                                                    match packs_store::delete_starter_pack(&pc).await {
                                                        Ok(()) => {
                                                            navigator.push(Route::PacksHome {});
                                                        }
                                                        Err(e) => {
                                                            log::error!("Failed to delete pack: {}", e);
                                                            delete_error.set(Some(format!("Failed to delete: {}", e)));
                                                        }
                                                    }
                                                    deleting.set(false);
                                                });
                                            }
                                        },
                                        if *deleting.read() { "Deleting..." } else { "Delete" }
                                    }
                                }
                            }
                        }
                    }

                    // Tabs
                    {
                        let people_tab_class = if *active_tab.read() == DetailTab::People {
                            "flex-1 py-3 text-center font-medium transition border-b-2 border-primary text-primary"
                        } else {
                            "flex-1 py-3 text-center font-medium transition border-b-2 border-transparent text-muted-foreground hover:text-foreground hover:border-border"
                        };
                        let posts_tab_class = if *active_tab.read() == DetailTab::Posts {
                            "flex-1 py-3 text-center font-medium transition border-b-2 border-primary text-primary"
                        } else {
                            "flex-1 py-3 text-center font-medium transition border-b-2 border-transparent text-muted-foreground hover:text-foreground hover:border-border"
                        };
                        rsx! {
                            div { class: "border-t border-border",
                                div { class: "flex",
                                    button {
                                        class: "{people_tab_class}",
                                        onclick: move |_| active_tab.set(DetailTab::People),
                                        "People ({p.members.len()})"
                                    }
                                    button {
                                        class: "{posts_tab_class}",
                                        onclick: move |_| active_tab.set(DetailTab::Posts),
                                        "Posts"
                                    }
                                }
                            }
                        }
                    }

                    // Tab content
                    match *active_tab.read() {
                        DetailTab::People => rsx! {
                            div { class: "divide-y divide-border",
                                for member in &p.members {
                                    MemberRow {
                                        key: "{member.pubkey}",
                                        pubkey: member.pubkey.clone(),
                                    }
                                }
                            }
                        },
                        DetailTab::Posts => rsx! {
                            if *posts_loading.read() {
                                div { class: "p-4 space-y-4",
                                    for _ in 0..3 {
                                        div { class: "animate-pulse space-y-3 py-4",
                                            div { class: "flex items-center gap-3",
                                                div { class: "w-10 h-10 rounded-full bg-muted" }
                                                div { class: "flex-1 space-y-2",
                                                    div { class: "h-4 bg-muted rounded w-1/4" }
                                                    div { class: "h-3 bg-muted rounded w-1/6" }
                                                }
                                            }
                                            div { class: "h-4 bg-muted rounded w-3/4" }
                                            div { class: "h-4 bg-muted rounded w-1/2" }
                                        }
                                    }
                                }
                            } else if posts.read().is_empty() {
                                div { class: "flex flex-col items-center justify-center py-16 text-center",
                                    div { class: "text-4xl mb-3", "📝" }
                                    p { class: "text-muted-foreground", "No recent posts from pack members" }
                                }
                            } else {
                                div { class: "divide-y divide-border",
                                    for feed_item in posts.read().iter() {
                                        NoteCard {
                                            key: "{feed_item.event().id}",
                                            event: feed_item.event().clone(),
                                            repost_info: feed_item.repost_info(),
                                            collapsible: true,
                                            cached_muted_posts: cached_muted_posts.read().clone(),
                                            cached_blocked_users: cached_blocked_users.read().clone(),
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Member row component
#[component]
fn MemberRow(pubkey: String) -> Element {
    let profile = profiles::get_profile(&pubkey);
    let display_name = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&pubkey));
    let picture = profile.as_ref().and_then(|p| p.picture.clone());
    let nip05 = profile.as_ref().and_then(|p| p.nip05.clone());
    let about = profile
        .as_ref()
        .and_then(|p| p.about.clone())
        .map(|a| {
            if a.chars().count() > 100 {
                format!("{}...", a.chars().take(100).collect::<String>())
            } else {
                a
            }
        });

    let is_authenticated = auth_store::is_authenticated();
    let mut is_following = use_signal(|| false);
    let mut follow_loading = use_signal(|| false);
    let toast = consume_toast();

    // Check follow status
    {
        let pk = pubkey.clone();
        use_effect(move || {
            if !auth_store::is_authenticated() {
                return;
            }
            let pk = pk.clone();
            spawn(async move {
                match nostr_client::is_following(pk).await {
                    Ok(following) => is_following.set(following),
                    Err(e) => log::error!("Failed to check follow status: {}", e),
                }
            });
        });
    }

    rsx! {
        div { class: "flex items-center gap-3 px-4 py-3 hover:bg-accent/50 transition",
            Link {
                to: Route::Profile { pubkey: pubkey.clone() },
                class: "flex items-center gap-3 flex-1 min-w-0",
                // Avatar
                if let Some(ref pic) = picture.as_ref().filter(|u| is_valid_http_url(u)) {
                    img {
                        src: "{pic}",
                        alt: "{display_name}",
                        class: "w-12 h-12 rounded-full shrink-0",
                    }
                } else {
                    div { class: "w-12 h-12 rounded-full bg-primary/20 flex items-center justify-center shrink-0",
                        span { class: "text-lg font-bold text-primary",
                            "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                        }
                    }
                }

                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2",
                        span { class: "font-medium truncate", "{display_name}" }
                    }
                    if let Some(ref nip) = nip05 {
                        p { class: "text-xs text-muted-foreground truncate", "{nip}" }
                    }
                    if let Some(ref bio) = about {
                        p { class: "text-sm text-muted-foreground truncate mt-0.5", "{bio}" }
                    }
                }
            }

            // Follow/Following toggle
            if is_authenticated {
                {
                    let pk = pubkey.clone();
                    rsx! {
                        button {
                            class: if *is_following.read() {
                                "px-3 py-1.5 text-sm rounded-lg border border-border hover:bg-accent transition shrink-0"
                            } else {
                                "px-3 py-1.5 text-sm rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition shrink-0"
                            },
                            disabled: *follow_loading.read(),
                            onclick: move |evt: MouseEvent| {
                                evt.prevent_default();
                                evt.stop_propagation();
                                follow_loading.set(true);
                                let pk = pk.clone();
                                let currently_following = *is_following.read();
                                spawn(async move {
                                    let result = if currently_following {
                                        nostr_client::unfollow_user(pk).await
                                    } else {
                                        nostr_client::follow_user(pk).await
                                    };
                                    match result {
                                        Ok(()) => is_following.set(!currently_following),
                                        Err(e) => {
                                            log::error!("Follow/unfollow error: {}", e);
                                            toast.error(
                                                if currently_following { "Failed to unfollow" } else { "Failed to follow" }.to_string(),
                                                ToastOptions::new().duration(Duration::from_secs(3)),
                                            );
                                        }
                                    }
                                    follow_loading.set(false);
                                });
                            },
                            if *follow_loading.read() {
                                "..."
                            } else if *is_following.read() {
                                "Following"
                            } else {
                                "Follow"
                            }
                        }
                    }
                }
            }
        }
    }
}
