//! Code User Profile Page
//!
//! Displays a user's code-specific profile at /code/profile/:pubkey
//! showing their repositories, issues, pull requests, and code snippets.
use crate::components::code::{CodeIssueCard, CodePullCard, ContributionGraph};
use crate::components::icons::GlobeIcon;
use crate::components::{CodeRepoCard, CodeSnippetCard};
use crate::routes::Route;
use crate::services::git_hosting::{
    fetch_user_issues, fetch_user_prs, fetch_user_repositories, fetch_user_snippets,
};
use crate::stores::{auth_store, nostr_client, profiles};
use crate::utils::clipboard::copy_to_clipboard;
use crate::utils::nip34::{DisplaySnippet, Issue, PullRequest, Repository};
use dioxus::prelude::*;
use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::PublicKey;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum CodeProfileTab {
    Repositories,
    Issues,
    PullRequests,
    Snippets,
}

#[derive(Clone, Debug, Default)]
struct TabData {
    count: usize,
}

fn default_tab_data_map() -> HashMap<CodeProfileTab, TabData> {
    let mut map = HashMap::new();
    map.insert(CodeProfileTab::Repositories, TabData::default());
    map.insert(CodeProfileTab::Issues, TabData::default());
    map.insert(CodeProfileTab::PullRequests, TabData::default());
    map.insert(CodeProfileTab::Snippets, TabData::default());
    map
}

#[component]
pub fn CodeProfileViewer(pubkey: String) -> Element {
    let mut profile_data = use_signal(|| None::<profiles::Profile>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut profile_meta_error = use_signal(|| None::<String>);
    let mut active_tab = use_signal(|| CodeProfileTab::Repositories);
    let mut tab_data = use_signal(default_tab_data_map);

    // Follow state
    let mut is_following = use_signal(|| false);
    let mut follow_loading = use_signal(|| false);

    // Tab content
    let mut repos = use_signal(Vec::<Repository>::new);
    let mut issues = use_signal(Vec::<Issue>::new);
    let mut prs = use_signal(Vec::<PullRequest>::new);
    let mut snippets = use_signal(Vec::<DisplaySnippet>::new);

    // Request staleness guard
    let mut request_id = use_signal(|| 0u32);

    // Clipboard copied state
    #[allow(unused_mut)]
    let mut npub_copied = use_signal(|| false);

    let parsed_pubkey = PublicKey::parse(&pubkey).ok();

    let auth = auth_store::AUTH_STATE.read();
    let is_own_profile = auth
        .pubkey
        .as_ref()
        .and_then(|pk| PublicKey::parse(pk).ok())
        .and_then(|user_pk| parsed_pubkey.map(|profile_pk| user_pk == profile_pk))
        .unwrap_or(false);

    let npub_display = parsed_pubkey
        .and_then(|pk| pk.to_bech32().ok())
        .unwrap_or_default();

    // Follow/unfollow error + request tracking (declared before effect so closure can capture)
    let mut follow_error = use_signal(|| None::<String>);
    let mut follow_request_id = use_signal(|| 0u64);

    // Phase 1: Load profile metadata + all counts on mount/pubkey change
    let pubkey_for_load = pubkey.clone();
    let mut prev_pubkey = use_signal(String::new);
    use_effect(use_reactive(&pubkey, move |pk| {
        // Clear PROFILE_CACHE entry for old pubkey on navigation
        let old_pk = prev_pubkey.peek().clone();
        if !old_pk.is_empty() && old_pk != pk {
            let normalized = PublicKey::parse(&old_pk)
                .ok()
                .map(|p| p.to_hex())
                .unwrap_or(old_pk);
            profiles::PROFILE_CACHE.write().pop(&normalized);
        }
        prev_pubkey.set(pk.clone());
        // Reset state
        profile_data.set(None);
        loading.set(true);
        error.set(None);
        active_tab.set(CodeProfileTab::Repositories);
        tab_data.set(default_tab_data_map());
        repos.set(Vec::new());
        issues.set(Vec::new());
        prs.set(Vec::new());
        snippets.set(Vec::new());
        is_following.set(false);
        follow_loading.set(false);
        follow_error.set(None);
        let next_follow_id = follow_request_id.peek().wrapping_add(1);
        follow_request_id.set(next_follow_id);
        profile_meta_error.set(None);

        let current_id = request_id.peek().wrapping_add(1);
        request_id.set(current_id);

        spawn(async move {
            // Wait for client
            loop {
                if *nostr_client::CLIENT_INITIALIZED.read() {
                    break;
                }
                crate::platform::timer::sleep_ms(100).await;
                if *request_id.peek() != current_id {
                    return;
                }
            }
            if *request_id.peek() != current_id {
                return;
            }

            let parsed = match PublicKey::parse(&pk) {
                Ok(pk) => pk,
                Err(e) => {
                    if *request_id.peek() != current_id {
                        return;
                    }
                    error.set(Some(format!("Invalid public key: {}", e)));
                    loading.set(false);
                    return;
                }
            };

            // Load profile metadata
            match profiles::fetch_profile(parsed.to_hex()).await {
                Ok(profile) => {
                    if *request_id.peek() != current_id {
                        return;
                    }
                    profile_data.set(Some(profile));
                }
                Err(e) => {
                    if *request_id.peek() == current_id {
                        profile_meta_error.set(Some(format!("Failed to load profile: {}", e)));
                    }
                    log::warn!("Failed to load profile: {}", e);
                }
            }

            // Check following status
            if auth_store::is_authenticated() {
                if let Ok(following) = nostr_client::is_following(parsed.to_hex()).await {
                    if *request_id.peek() != current_id {
                        return;
                    }
                    is_following.set(following);
                }
            }

            // Load repos (default tab) and counts for all tabs concurrently
            let (repos_result, issues_result, prs_result, snippets_result) = futures::join!(
                fetch_user_repositories(&parsed, 50),
                fetch_user_issues(&parsed, 50),
                fetch_user_prs(&parsed, 50),
                fetch_user_snippets(&parsed, 50)
            );

            // Discard stale results if pubkey changed during async fetch
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale profile result");
                return;
            }

            let repo_count = repos_result.as_ref().map(|r| r.len()).unwrap_or(0);
            let issue_count = issues_result.as_ref().map(|i| i.len()).unwrap_or(0);
            let pr_count = prs_result.as_ref().map(|p| p.len()).unwrap_or(0);
            let snippet_count = snippets_result.as_ref().map(|s| s.len()).unwrap_or(0);

            if let Ok(r) = repos_result {
                repos.set(r);
            }
            if let Ok(i) = issues_result {
                issues.set(i);
            }
            if let Ok(p) = prs_result {
                prs.set(p);
            }
            if let Ok(s) = snippets_result {
                snippets.set(s);
            }

            let mut td = default_tab_data_map();
            td.insert(CodeProfileTab::Repositories, TabData { count: repo_count });
            td.insert(CodeProfileTab::Issues, TabData { count: issue_count });
            td.insert(CodeProfileTab::PullRequests, TabData { count: pr_count });
            td.insert(
                CodeProfileTab::Snippets,
                TabData {
                    count: snippet_count,
                },
            );
            tab_data.set(td);

            loading.set(false);
        });
    }));

    // Follow/unfollow handler
    let pubkey_for_follow = pubkey.clone();
    let on_follow_click = move |_| {
        if *follow_loading.peek() {
            return;
        }
        let pk = pubkey_for_follow.clone();
        let captured = follow_request_id.peek().wrapping_add(1);
        follow_request_id.set(captured);
        follow_loading.set(true);
        follow_error.set(None);
        spawn(async move {
            let hex = PublicKey::parse(&pk).map(|k| k.to_hex()).unwrap_or(pk);

            if is_following() {
                match nostr_client::unfollow_user(hex).await {
                    Ok(()) => {
                        if *follow_request_id.peek() != captured {
                            follow_loading.set(false);
                            return;
                        }
                        is_following.set(false);
                    }
                    Err(e) => {
                        if *follow_request_id.peek() != captured {
                            follow_loading.set(false);
                            return;
                        }
                        log::error!("Failed to unfollow: {}", e);
                        follow_error.set(Some(format!("Failed to unfollow: {}", e)));
                    }
                }
            } else {
                match nostr_client::follow_user(hex).await {
                    Ok(()) => {
                        if *follow_request_id.peek() != captured {
                            follow_loading.set(false);
                            return;
                        }
                        is_following.set(true);
                    }
                    Err(e) => {
                        if *follow_request_id.peek() != captured {
                            follow_loading.set(false);
                            return;
                        }
                        log::error!("Failed to follow: {}", e);
                        follow_error.set(Some(format!("Failed to follow: {}", e)));
                    }
                }
            }
            if *follow_request_id.peek() == captured {
                follow_loading.set(false);
            }
        });
    };

    // Copy npub to clipboard
    let npub_for_copy = npub_display.clone();
    let on_copy_npub = move |_| {
        let npub = npub_for_copy.clone();
        spawn(async move {
            if copy_to_clipboard(&npub).await.is_ok() {
                npub_copied.set(true);
                crate::platform::timer::sleep_ms(2000).await;
                npub_copied.set(false);
            }
        });
    };

    // Extract profile fields
    let profile = profile_data.read();
    let display_name = profile
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| {
            if npub_display.len() > 16 {
                format!(
                    "{}...{}",
                    &npub_display[..8],
                    &npub_display[npub_display.len() - 4..]
                )
            } else {
                npub_display.clone()
            }
        });
    let avatar_url = profile
        .as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| {
            let seed = parsed_pubkey.map(|pk| pk.to_hex()).unwrap_or_default();
            format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", seed)
        });
    let about = profile.as_ref().and_then(|p| p.about.clone());
    let nip05 = profile.as_ref().and_then(|p| p.nip05.clone());
    let website = profile.as_ref().and_then(|p| p.website.clone());
    let lud16 = profile.as_ref().and_then(|p| p.lud16.clone());
    let banner = profile.as_ref().and_then(|p| p.banner.clone());

    let tab_counts = tab_data.read();
    let repo_count = tab_counts
        .get(&CodeProfileTab::Repositories)
        .map(|t| t.count)
        .unwrap_or(0);
    let issue_count = tab_counts
        .get(&CodeProfileTab::Issues)
        .map(|t| t.count)
        .unwrap_or(0);
    let pr_count = tab_counts
        .get(&CodeProfileTab::PullRequests)
        .map(|t| t.count)
        .unwrap_or(0);
    let snippet_count = tab_counts
        .get(&CodeProfileTab::Snippets)
        .map(|t| t.count)
        .unwrap_or(0);

    let current_tab = *active_tab.read();

    // Truncated npub for display
    let npub_short = if npub_display.len() > 20 {
        format!(
            "{}...{}",
            &npub_display[..12],
            &npub_display[npub_display.len() - 4..]
        )
    } else {
        npub_display.clone()
    };

    rsx! {
        div { class: "min-h-screen",
            // Back navigation
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-2",
                    Link {
                        to: Route::CodeHome {},
                        class: "flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground transition",
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
                            polyline { points: "15 18 9 12 15 6" }
                        }
                        "Back to Code"
                    }
                }
            }

            if loading() {
                // Loading skeleton
                div { class: "p-4 space-y-4",
                    div { class: "h-32 bg-muted rounded-lg animate-pulse" }
                    div { class: "flex items-center gap-4",
                        div { class: "w-24 h-24 rounded-full bg-muted animate-pulse" }
                        div { class: "space-y-2 flex-1",
                            div { class: "h-6 w-48 bg-muted rounded animate-pulse" }
                            div { class: "h-4 w-32 bg-muted rounded animate-pulse" }
                        }
                    }
                }
            } else if let Some(err) = error() {
                div { class: "p-8 text-center",
                    div { class: "text-destructive text-lg mb-2", "Error" }
                    p { class: "text-muted-foreground", "{err}" }
                    Link {
                        to: Route::CodeHome {},
                        class: "mt-4 inline-block px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition",
                        "Back to Code Home"
                    }
                }
            } else {
                // Banner
                if let Some(ref banner_url) = banner {
                    div { class: "h-32 md:h-48 bg-muted overflow-hidden",
                        img {
                            class: "w-full h-full object-cover",
                            src: "{banner_url}",
                            alt: "Profile banner",
                        }
                    }
                } else {
                    div { class: "h-32 md:h-48 bg-gradient-to-r from-primary/30 to-accent" }
                }

                // Profile header card
                div { class: "px-4 -mt-12 md:-mt-16 relative z-10",
                    div { class: "bg-card border border-border rounded-lg p-4 md:p-6",
                        div { class: "flex flex-col md:flex-row gap-4",
                            // Avatar
                            div { class: "shrink-0",
                                img {
                                    class: "w-20 h-20 md:w-24 md:h-24 rounded-full border-4 border-card object-cover bg-muted",
                                    src: "{avatar_url}",
                                    alt: "{display_name}",
                                }
                            }

                            // Info
                            div { class: "flex-1 min-w-0",
                                div { class: "flex items-start justify-between gap-4",
                                    div {
                                        h1 { class: "text-xl md:text-2xl font-bold text-foreground", "{display_name}" }
                                        if let Some(ref nip05_val) = nip05 {
                                            div { class: "flex items-center gap-1 text-sm text-muted-foreground mt-0.5",
                                                GlobeIcon { class: "w-3.5 h-3.5 text-muted-foreground".to_string() }
                                                span { "{nip05_val}" }
                                            }
                                        }
                                    }

                                    // Follow button (hidden on own profile)
                                    if !is_own_profile && auth_store::is_authenticated() {
                                        div { class: "flex flex-col items-end gap-1",
                                            button {
                                                class: if is_following() {
                                                    "px-4 py-1.5 text-sm rounded-lg border border-border hover:bg-destructive/10 hover:text-destructive hover:border-destructive transition"
                                                } else {
                                                    "px-4 py-1.5 text-sm rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition"
                                                },
                                                disabled: follow_loading(),
                                                onclick: on_follow_click,
                                                if follow_loading() {
                                                    "..."
                                                } else if is_following() {
                                                    "Following"
                                                } else {
                                                    "Follow"
                                                }
                                            }
                                            if let Some(ref err) = *follow_error.read() {
                                                p { class: "text-xs text-destructive max-w-48 text-right", "{err}" }
                                            }
                                        }
                                    }
                                }

                                // About
                                if let Some(ref about_text) = about {
                                    p { class: "mt-2 text-sm text-muted-foreground line-clamp-3", "{about_text}" }
                                }

                                // Metadata links row
                                div { class: "mt-3 flex flex-wrap items-center gap-3 text-sm",
                                    if let Some(ref website_url) = website.as_ref().filter(|u| u.starts_with("http://") || u.starts_with("https://")) {
                                        a {
                                            href: "{website_url}",
                                            target: "_blank",
                                            rel: "noopener noreferrer",
                                            class: "flex items-center gap-1 text-primary hover:underline",
                                            svg {
                                                class: "w-3.5 h-3.5",
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
                                                line { x1: "2", y1: "12", x2: "22", y2: "12" }
                                                path { d: "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" }
                                            }
                                            {
                                                let display_url = website_url
                                                    .trim_start_matches("https://")
                                                    .trim_start_matches("http://")
                                                    .trim_end_matches('/');
                                                rsx! { "{display_url}" }
                                            }
                                        }
                                    }

                                    if let Some(ref ln_addr) = lud16 {
                                        span { class: "flex items-center gap-1 text-muted-foreground",
                                            "⚡"
                                            span { "{ln_addr}" }
                                        }
                                    }

                                    // npub display + copy
                                    button {
                                        class: "flex items-center gap-1 text-muted-foreground hover:text-foreground transition font-mono text-xs",
                                        onclick: on_copy_npub,
                                        "{npub_short}"
                                        if npub_copied() {
                                            svg {
                                                class: "w-3.5 h-3.5 text-green-500",
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
                                        } else {
                                            svg {
                                                class: "w-3.5 h-3.5",
                                                xmlns: "http://www.w3.org/2000/svg",
                                                width: "24",
                                                height: "24",
                                                view_box: "0 0 24 24",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                rect { x: "9", y: "9", width: "13", height: "13", rx: "2", ry: "2" }
                                                path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                                            }
                                        }
                                    }
                                }

                                // View social profile link
                                div { class: "mt-3",
                                    Link {
                                        to: Route::Profile { pubkey: crate::utils::nip19_urls::profile_route_id(&pubkey_for_load) },
                                        class: "inline-flex items-center gap-1 text-sm text-primary hover:underline",
                                        "View Social Profile"
                                        svg {
                                            class: "w-3.5 h-3.5",
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
                        }
                    }
                }

                // Non-blocking profile metadata warning
                if let Some(meta_err) = profile_meta_error() {
                    div { class: "px-4 mt-2",
                        div { class: "bg-accent text-foreground text-sm rounded-lg px-4 py-2",
                            "{meta_err}"
                        }
                    }
                }

                // Stats row
                div { class: "px-4 mt-4",
                    div { class: "flex flex-wrap gap-3",
                        StatChip { label: "Repositories", count: repo_count, icon: "repo" }
                        StatChip { label: "Issues", count: issue_count, icon: "issue" }
                        StatChip { label: "Pull Requests", count: pr_count, icon: "pr" }
                        StatChip { label: "Snippets", count: snippet_count, icon: "snippet" }
                    }
                }

                // Contribution graph
                div { class: "px-4 mt-4",
                    ContributionGraph { pubkey: pubkey.clone() }
                }

                // Tab bar
                div { class: "px-4 mt-4 border-b border-border",
                    div { class: "flex gap-1 overflow-x-auto",
                        TabButton { label: "Repositories", tab: CodeProfileTab::Repositories, active: current_tab, on_click: move |_| active_tab.set(CodeProfileTab::Repositories) }
                        TabButton { label: "Issues", tab: CodeProfileTab::Issues, active: current_tab, on_click: move |_| active_tab.set(CodeProfileTab::Issues) }
                        TabButton { label: "Pull Requests", tab: CodeProfileTab::PullRequests, active: current_tab, on_click: move |_| active_tab.set(CodeProfileTab::PullRequests) }
                        TabButton { label: "Snippets", tab: CodeProfileTab::Snippets, active: current_tab, on_click: move |_| active_tab.set(CodeProfileTab::Snippets) }
                    }
                }

                // Tab content
                div { class: "p-4",
                    match current_tab {
                        CodeProfileTab::Repositories => rsx! {
                            if repos.read().is_empty() {
                                EmptyTabState { label: "repositories" }
                            } else {
                                div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                                    for repo in repos.read().iter() {
                                        CodeRepoCard {
                                            key: "{repo.naddr}",
                                            repo: repo.clone(),
                                        }
                                    }
                                }
                            }
                        },
                        CodeProfileTab::Issues => rsx! {
                            if issues.read().is_empty() {
                                EmptyTabState { label: "issues" }
                            } else {
                                div { class: "space-y-2",
                                    for issue in issues.read().iter() {
                                        CodeIssueCard {
                                            key: "{issue.event_id}",
                                            issue: issue.clone(),
                                        }
                                    }
                                }
                            }
                        },
                        CodeProfileTab::PullRequests => rsx! {
                            if prs.read().is_empty() {
                                EmptyTabState { label: "pull requests" }
                            } else {
                                div { class: "space-y-2",
                                    for pr in prs.read().iter() {
                                        CodePullCard {
                                            key: "{pr.event_id}",
                                            pr: pr.clone(),
                                        }
                                    }
                                }
                            }
                        },
                        CodeProfileTab::Snippets => rsx! {
                            if snippets.read().is_empty() {
                                EmptyTabState { label: "snippets" }
                            } else {
                                div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                                    for snippet in snippets.read().iter() {
                                        CodeSnippetCard {
                                            key: "{snippet.event_id}",
                                            snippet: snippet.clone(),
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

#[component]
fn StatChip(label: &'static str, count: usize, icon: &'static str) -> Element {
    rsx! {
        div { class: "flex items-center gap-1.5 px-3 py-1.5 bg-muted rounded-full text-sm",
            match icon {
                "repo" => rsx! {
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
                        path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" }
                        path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" }
                    }
                },
                "issue" => rsx! {
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
                        circle { cx: "12", cy: "12", r: "10" }
                        line { x1: "12", y1: "8", x2: "12", y2: "12" }
                        line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                    }
                },
                "pr" => rsx! {
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
                        circle { cx: "18", cy: "18", r: "3" }
                        circle { cx: "6", cy: "6", r: "3" }
                        path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                        line { x1: "6", y1: "9", x2: "6", y2: "21" }
                    }
                },
                _ => rsx! {
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
                        polyline { points: "16 18 22 12 16 6" }
                        polyline { points: "8 6 2 12 8 18" }
                    }
                },
            }
            span { class: "font-medium", "{count}" }
            span { class: "text-muted-foreground", "{label}" }
        }
    }
}

#[component]
fn TabButton(
    label: &'static str,
    tab: CodeProfileTab,
    active: CodeProfileTab,
    on_click: EventHandler<MouseEvent>,
) -> Element {
    let is_active = tab == active;
    rsx! {
        button {
            class: if is_active {
                "px-4 py-2 text-sm font-medium text-foreground border-b-2 border-primary transition whitespace-nowrap"
            } else {
                "px-4 py-2 text-sm text-muted-foreground hover:text-foreground transition whitespace-nowrap"
            },
            onclick: move |e| on_click.call(e),
            "{label}"
        }
    }
}

#[component]
fn EmptyTabState(label: &'static str) -> Element {
    rsx! {
        div { class: "py-12 text-center",
            svg {
                class: "w-12 h-12 mx-auto text-muted-foreground/50 mb-3",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" }
                polyline { points: "13 2 13 9 20 9" }
            }
            p { class: "text-muted-foreground", "No {label} found" }
        }
    }
}
