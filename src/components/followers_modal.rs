use crate::services::social_graph::{self, ProfileMetadata};
use crate::stores::{auth_store, nostr_client, profiles};
use crate::utils::format::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FollowersTab {
    Following,
    Followers,
}

#[derive(Props, Clone, PartialEq)]
pub struct FollowersModalProps {
    pub pubkey: String,
    pub initial_tab: FollowersTab,
    pub open: Signal<bool>,
}

#[derive(Debug, Clone)]
struct UserEntry {
    pubkey: String,
    display_name: String,
    picture: Option<String>,
    npub: String,
}

fn get_display_name_from_metadata(meta: &ProfileMetadata) -> String {
    if let Some(name) = &meta.display_name {
        if !name.trim().is_empty() {
            return name.clone();
        }
    }
    if let Some(name) = &meta.name {
        if !name.trim().is_empty() {
            return name.clone();
        }
    }
    truncate_pubkey(&meta.pubkey)
}

fn get_display_name_from_profile(profile: &profiles::Profile) -> String {
    profile.get_display_name()
}

fn get_picture_from_metadata(meta: &ProfileMetadata) -> Option<String> {
    meta.picture.as_ref().filter(|p| !p.trim().is_empty()).cloned()
}

#[component]
pub fn FollowersModal(props: FollowersModalProps) -> Element {
    let mut open = props.open;

    let mut active_tab = use_signal(|| props.initial_tab);
    let mut following_entries = use_signal(Vec::<UserEntry>::new);
    let mut followers_entries = use_signal(Vec::<UserEntry>::new);
    let mut following_total = use_signal(|| 0i64);
    let mut followers_total = use_signal(|| 0i64);
    let mut following_loaded = use_signal(|| 0i64);
    let mut followers_loaded = use_signal(|| 0i64);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let hex_pubkey = {
        crate::utils::nip19_urls::parse_profile_id(&props.pubkey)
            .map(|pk| pk.to_hex())
            .unwrap_or_else(|| props.pubkey.clone())
    };

    let hex_for_following_tab = hex_pubkey.clone();
    let hex_for_followers_tab = hex_pubkey.clone();
    let hex_for_load_more = hex_pubkey.clone();
    let hex_for_retry = hex_pubkey.clone();

    let is_open_for_effect = *open.read();
    let pubkey_for_effect = props.pubkey.clone();

    use_effect(use_reactive!(|(
        is_open_for_effect,
        pubkey_for_effect,
    )| {
        if !is_open_for_effect {
            return;
        }

        following_entries.set(Vec::new());
        followers_entries.set(Vec::new());
        following_total.set(0);
        followers_total.set(0);
        following_loaded.set(0);
        followers_loaded.set(0);
        error.set(None);

        let hex = crate::utils::nip19_urls::parse_profile_id(&pubkey_for_effect)
            .map(|pk| pk.to_hex())
            .unwrap_or_else(|| pubkey_for_effect.clone());
        let tab = *active_tab.read();

        spawn(async move {
            loading.set(true);

            let fetch_follows = matches!(tab, FollowersTab::Following);
            let fetch_followers = matches!(tab, FollowersTab::Followers);

            match social_graph::fetch_social_graph(
                &hex,
                if fetch_follows { 100 } else { 0 },
                0,
                if fetch_followers { 100 } else { 0 },
                0,
            )
            .await
            {
                Ok(response) => {
                    if fetch_follows {
                        following_total.set(response.follows.count);
                        following_loaded.set(response.follows.pubkeys.len() as i64);
                        let entries = resolve_pubkeys(response.follows.pubkeys).await;
                        following_entries.set(entries);
                    }
                    if fetch_followers {
                        followers_total.set(response.followers.count);
                        followers_loaded.set(response.followers.pubkeys.len() as i64);
                        let entries = resolve_pubkeys(response.followers.pubkeys).await;
                        followers_entries.set(entries);
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch social graph: {}", e);
                    error.set(Some("Failed to load data. Please try again.".to_string()));
                }
            }

            loading.set(false);
        });
    }));

    if !*open.read() {
        return rsx! {};
    }

    let current_entries = match *active_tab.read() {
        FollowersTab::Following => following_entries.read().clone(),
        FollowersTab::Followers => followers_entries.read().clone(),
    };
    let (current_total, current_loaded) = match *active_tab.read() {
        FollowersTab::Following => (*following_total.read(), *following_loaded.read()),
        FollowersTab::Followers => (*followers_total.read(), *followers_loaded.read()),
    };
    let has_more = current_loaded > 0 && current_total > current_loaded;

    let following_count_str = if *following_total.read() > 0 {
        format!(" ({})", *following_total.read())
    } else {
        String::new()
    };
    let followers_count_str = if *followers_total.read() > 0 {
        format!(" ({})", *followers_total.read())
    } else {
        String::new()
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm",
            onclick: move |_| open.set(false),
            div {
                class: "bg-card border border-border rounded-lg shadow-xl w-full max-w-lg max-h-[85vh] flex flex-col",
                onclick: move |e: MouseEvent| e.stop_propagation(),

                // Header with tabs
                div { class: "flex items-center justify-between border-b border-border px-4 pt-4",
                    div { class: "flex gap-1",
                        button {
                            class: if matches!(*active_tab.read(), FollowersTab::Following) {
                                "px-4 py-2 text-sm font-semibold border-b-2 border-foreground text-foreground"
                            } else {
                                "px-4 py-2 text-sm font-semibold border-b-2 border-transparent text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| {
                                active_tab.set(FollowersTab::Following);
                                if *following_total.read() == 0 && following_entries.read().is_empty() {
                                    error.set(None);
                                    let hex = hex_for_following_tab.clone();
                                    loading.set(true);
                                    spawn(async move {
                                        match social_graph::fetch_social_graph(&hex, 100, 0, 0, 0).await {
                                            Ok(response) => {
                                                following_total.set(response.follows.count);
                                                following_loaded.set(response.follows.pubkeys.len() as i64);
                                                let entries = resolve_pubkeys(response.follows.pubkeys).await;
                                                following_entries.set(entries);
                                            }
                                            Err(e) => {
                                                log::error!("Failed to fetch following: {}", e);
                                                error.set(Some("Failed to load following list.".to_string()));
                                            }
                                        }
                                        loading.set(false);
                                    });
                                }
                            },
                            "Following{following_count_str}"
                        }
                        button {
                            class: if matches!(*active_tab.read(), FollowersTab::Followers) {
                                "px-4 py-2 text-sm font-semibold border-b-2 border-foreground text-foreground"
                            } else {
                                "px-4 py-2 text-sm font-semibold border-b-2 border-transparent text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| {
                                active_tab.set(FollowersTab::Followers);
                                if *followers_total.read() == 0 && followers_entries.read().is_empty() {
                                    error.set(None);
                                    let hex = hex_for_followers_tab.clone();
                                    loading.set(true);
                                    spawn(async move {
                                        match social_graph::fetch_social_graph(&hex, 0, 0, 100, 0).await {
                                            Ok(response) => {
                                                followers_total.set(response.followers.count);
                                                followers_loaded.set(response.followers.pubkeys.len() as i64);
                                                let entries = resolve_pubkeys(response.followers.pubkeys).await;
                                                followers_entries.set(entries);
                                            }
                                            Err(e) => {
                                                log::error!("Failed to fetch followers: {}", e);
                                                error.set(Some("Failed to load followers list.".to_string()));
                                            }
                                        }
                                        loading.set(false);
                                    });
                                }
                            },
                            "Followers{followers_count_str}"
                        }
                    }
                    button {
                        class: "p-2 rounded-full hover:bg-accent text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| open.set(false),
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M6 18L18 6M6 6l12 12",
                            }
                        }
                    }
                }

                // User list
                div { class: "flex-1 overflow-y-auto scrollbar-hide",
                    if let Some(err) = error.read().as_ref() {
                        div { class: "flex flex-col items-center justify-center py-12 gap-3",
                            p { class: "text-red-500 text-sm text-center px-4", "{err}" }
                            button {
                                class: "px-4 py-2 text-sm border border-border rounded-lg hover:bg-accent transition",
                                onclick: move |_| {
                                    error.set(None);
                                    let hex = hex_for_retry.clone();
                                    let tab = *active_tab.read();
                                    loading.set(true);
                                    spawn(async move {
                                        let fetch_follows = matches!(tab, FollowersTab::Following);
                                        let fetch_followers = matches!(tab, FollowersTab::Followers);
                                        match social_graph::fetch_social_graph(
                                            &hex,
                                            if fetch_follows { 100 } else { 0 },
                                            0,
                                            if fetch_followers { 100 } else { 0 },
                                            0,
                                        )
                                        .await
                                        {
                                            Ok(response) => {
                                                if fetch_follows {
                                                    following_total.set(response.follows.count);
                                                    following_loaded.set(response.follows.pubkeys.len() as i64);
                                                    let entries = resolve_pubkeys(response.follows.pubkeys).await;
                                                    following_entries.set(entries);
                                                }
                                                if fetch_followers {
                                                    followers_total.set(response.followers.count);
                                                    followers_loaded.set(response.followers.pubkeys.len() as i64);
                                                    let entries = resolve_pubkeys(response.followers.pubkeys).await;
                                                    followers_entries.set(entries);
                                                }
                                            }
                                            Err(e) => {
                                                log::error!("Retry failed: {}", e);
                                                error.set(Some("Failed to load data. Please try again.".to_string()));
                                            }
                                        }
                                        loading.set(false);
                                    });
                                },
                                "Retry"
                            }
                        }
                    } else if *loading.read() && current_entries.is_empty() {
                        div { class: "flex items-center justify-center py-12",
                            div { class: "animate-spin w-8 h-8 border-2 border-foreground border-t-transparent rounded-full" }
                        }
                    } else if current_entries.is_empty() && !*loading.read() {
                        div { class: "text-center py-12",
                            p { class: "text-muted-foreground",
                                match *active_tab.read() {
                                    FollowersTab::Following => "Not following anyone yet",
                                    FollowersTab::Followers => "No followers yet",
                                }
                            }
                        }
                    } else {
                        {current_entries.into_iter().map(|entry| {
                            let pk_for_nav = entry.npub.clone();
                            let pk_for_follow = entry.pubkey.clone();
                            let display_name = entry.display_name.clone();
                            let picture = entry.picture.clone();
                            let initial = display_name.chars().next().unwrap_or('?').to_uppercase().to_string();

                            rsx! {
                                UserRow {
                                    key: "{pk_for_follow}",
                                    pubkey: pk_for_follow,
                                    display_name: display_name,
                                    picture: picture,
                                    initial: initial,
                                    npub: pk_for_nav,
                                    hex_for_follow: entry.pubkey.clone(),
                                }
                            }
                        })}
                        if has_more {
                            div { class: "p-4 flex justify-center",
                                button {
                                    class: "px-4 py-2 text-sm border border-border rounded-lg hover:bg-accent transition disabled:opacity-50",
                                    disabled: *loading.read(),
                                    onclick: move |_| {
                                        error.set(None);
                                        let tab = *active_tab.read();
                                        let hex = hex_for_load_more.clone();
                                        loading.set(true);
                                        spawn(async move {
                                            match tab {
                                                FollowersTab::Following => {
                                                    let offset = *following_loaded.read();
                                                    match social_graph::fetch_social_graph(&hex, 100, offset, 0, 0).await {
                                                        Ok(response) => {
                                                            let new_entries = resolve_pubkeys(response.follows.pubkeys).await;
                                                            following_loaded.set(offset + new_entries.len() as i64);
                                                            let mut current = following_entries.read().clone();
                                                            current.extend(new_entries);
                                                            following_entries.set(current);
                                                        }
                                                        Err(e) => {
                                                            log::error!("Failed to load more: {}", e);
                                                            error.set(Some("Failed to load more.".to_string()));
                                                        }
                                                    }
                                                }
                                                FollowersTab::Followers => {
                                                    let offset = *followers_loaded.read();
                                                    match social_graph::fetch_social_graph(&hex, 0, 0, 100, offset).await {
                                                        Ok(response) => {
                                                            let new_entries = resolve_pubkeys(response.followers.pubkeys).await;
                                                            followers_loaded.set(offset + new_entries.len() as i64);
                                                            let mut current = followers_entries.read().clone();
                                                            current.extend(new_entries);
                                                            followers_entries.set(current);
                                                        }
                                                        Err(e) => {
                                                            log::error!("Failed to load more: {}", e);
                                                            error.set(Some("Failed to load more.".to_string()));
                                                        }
                                                    }
                                                }
                                            }
                                            loading.set(false);
                                        });
                                    },
                                    if *loading.read() { "Loading..." } else { "Load more" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct UserRowProps {
    pubkey: String,
    display_name: String,
    picture: Option<String>,
    initial: String,
    npub: String,
    hex_for_follow: String,
}

#[component]
fn UserRow(props: UserRowProps) -> Element {
    let mut is_following = use_signal(|| false);
    let mut follow_loading = use_signal(|| false);
    let mut follow_checked = use_signal(|| false);

    let hex_for_check = props.hex_for_follow.clone();
    let hex_for_toggle = props.hex_for_follow.clone();
    let npub_for_nav = props.npub.clone();

    let is_authenticated = auth_store::is_authenticated();

    use_effect(move || {
        if *follow_checked.read() {
            return;
        }
        if !is_authenticated {
            follow_checked.set(true);
            return;
        }
        let hex = hex_for_check.clone();
        spawn(async move {
            if let Ok(following) = nostr_client::is_following(hex).await {
                is_following.set(following);
            }
            follow_checked.set(true);
        });
    });

    let npub_short = truncate_pubkey(&props.pubkey);

    rsx! {
        div { class: "flex items-center gap-3 px-4 py-3 hover:bg-accent/50 transition cursor-pointer",
            onclick: move |_| {
                let nav = navigator();
                nav.push(crate::routes::Route::AddressViewer {
                    address: npub_for_nav.clone(),
                });
            },
            div { class: "w-10 h-10 rounded-full bg-muted shrink-0 overflow-hidden",
                if let Some(url) = &props.picture {
                    img { src: "{url}", class: "w-full h-full object-cover", alt: "Profile picture" }
                } else {
                    div { class: "w-full h-full flex items-center justify-center text-muted-foreground font-bold text-sm",
                        "{props.initial}"
                    }
                }
            }
            div { class: "flex-1 min-w-0",
                p { class: "font-medium text-sm truncate", "{props.display_name}" }
                p { class: "text-xs text-muted-foreground truncate", "{npub_short}" }
            }
            if is_authenticated {
                button {
                    class: if *is_following.read() {
                        "px-3 py-1.5 text-xs border border-border rounded-full font-semibold hover:bg-accent transition shrink-0"
                    } else {
                        "px-3 py-1.5 text-xs bg-foreground text-background rounded-full font-semibold hover:opacity-90 transition shrink-0"
                    },
                    disabled: *follow_loading.read(),
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        let hex = hex_for_toggle.clone();
                        let current = *is_following.read();
                        follow_loading.set(true);
                        spawn(async move {
                            let result = if current {
                                nostr_client::unfollow_user(hex).await
                            } else {
                                nostr_client::follow_user(hex).await
                            };
                            match result {
                                Ok(_) => {
                                    is_following.set(!current);
                                }
                                Err(e) => {
                                    log::error!("Failed to follow/unfollow: {}", e);
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

async fn resolve_pubkeys(pubkeys: Vec<String>) -> Vec<UserEntry> {
    if pubkeys.is_empty() {
        return Vec::new();
    }

    let mut entries = Vec::new();
    let mut uncached = Vec::new();

    for pk in &pubkeys {
        let cache = profiles::PROFILE_CACHE.read();
        if let Some(profile) = cache.peek(pk) {
            let npub = PublicKey::parse(pk)
                .ok()
                .and_then(|p| p.to_bech32().ok())
                .unwrap_or_else(|| pk.clone());
            entries.push(UserEntry {
                pubkey: pk.clone(),
                display_name: get_display_name_from_profile(profile),
                picture: profile.picture.clone(),
                npub,
            });
        } else {
            uncached.push(pk.clone());
        }
    }

    if !uncached.is_empty() {
        let batch_size = 500;
        for chunk in uncached.chunks(batch_size) {
            let chunk_vec = chunk.to_vec();
            match social_graph::fetch_profiles_metadata(chunk_vec).await {
                Ok(metadata_list) => {
                    for meta in metadata_list {
                        let npub = PublicKey::parse(&meta.pubkey)
                            .ok()
                            .and_then(|p| p.to_bech32().ok())
                            .unwrap_or_else(|| meta.pubkey.clone());
                        let picture = get_picture_from_metadata(&meta);
                        let display_name = get_display_name_from_metadata(&meta);

                        {
                            let profile = profiles::Profile {
                                pubkey: meta.pubkey.clone(),
                                name: meta.name.clone(),
                                display_name: meta.display_name.clone(),
                                about: meta.about.clone(),
                                picture: picture.clone(),
                                banner: None,
                                nip05: meta.nip05.clone(),
                                lud16: None,
                                lud06: None,
                                website: None,
                                bot: None,
                                birthday: None,
                                event_created_at: None,
                                fetched_at: chrono::Utc::now(),
                                raw_metadata_json: None,
                            };
                            profiles::PROFILE_CACHE
                                .write()
                                .put(meta.pubkey.clone(), profile);
                        }

                        entries.push(UserEntry {
                            pubkey: meta.pubkey.clone(),
                            display_name,
                            picture,
                            npub,
                        });
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch profile metadata batch: {}", e);
                    for pk in chunk {
                        let npub = PublicKey::parse(pk)
                            .ok()
                            .and_then(|p| p.to_bech32().ok())
                            .unwrap_or_else(|| pk.clone());
                        entries.push(UserEntry {
                            pubkey: pk.clone(),
                            display_name: truncate_pubkey(pk),
                            picture: None,
                            npub,
                        });
                    }
                }
            }
        }
    }

    let mut pubkey_order: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, pk) in pubkeys.iter().enumerate() {
        pubkey_order.insert(pk.clone(), i);
    }
    entries.sort_by_key(|e| pubkey_order.get(&e.pubkey).copied().unwrap_or(usize::MAX));

    entries
}
