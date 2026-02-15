//! Code User Profile Page
//!
//! Displays a user's code-specific profile at /code/profile/:pubkey
//! showing their repositories, issues, pull requests, code snippets,
//! activity timeline, and sponsors.
use crate::components::{CodeRepoCard, CodeSnippetCard};
use crate::components::code::{CodeIssueCard, CodePullCard, ContributionGraph};
use crate::routes::Route;
use crate::services::git_hosting::{
    fetch_user_issues, fetch_user_prs, fetch_user_repositories, fetch_user_snippets,
};
use crate::services::git_hosting::activity::{fetch_user_activity, Activity, ActivityType};
use crate::stores::{auth_store, nostr_client, profiles};
use crate::utils::nip34::{DisplaySnippet, Issue, PullRequest, Repository};
use crate::utils::time::format_time_ago;
use dioxus::prelude::*;
use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::{Filter, Kind, PublicKey};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum CodeProfileTab {
    Repositories,
    Issues,
    PullRequests,
    Snippets,
    Activity,
    Sponsors,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
#[derive(Default)]
struct TabData {
    loaded: bool,
    count: usize,
}

fn default_tab_data_map() -> HashMap<CodeProfileTab, TabData> {
    let mut map = HashMap::new();
    map.insert(CodeProfileTab::Repositories, TabData::default());
    map.insert(CodeProfileTab::Issues, TabData::default());
    map.insert(CodeProfileTab::PullRequests, TabData::default());
    map.insert(CodeProfileTab::Snippets, TabData::default());
    map.insert(CodeProfileTab::Activity, TabData::default());
    map.insert(CodeProfileTab::Sponsors, TabData::default());
    map
}

/// Fetch zap supporters for a user by querying Kind 9735 (ZapReceipt) events.
/// Returns a vec of (sender_pubkey_hex, total_sats) sorted by total descending.
async fn fetch_zap_supporters(pubkey: &PublicKey) -> Result<Vec<(String, u64)>, String> {
    use nostr_sdk::{Alphabet, SingleLetterTag};
    let filter = Filter::new()
        .kind(Kind::ZapReceipt)
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::P),
            pubkey.to_hex(),
        )
        .limit(500);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
        .await
        .map_err(|e| format!("Failed to fetch zap receipts: {e}"))?;

    let mut totals: HashMap<String, u64> = HashMap::new();

    for event in &events {
        // Sender is in uppercase P tag (the zapper's pubkey)
        let sender = event.tags.iter().find_map(|t| {
            let v = t.as_slice();
            if v.len() >= 2 && v[0] == "P" {
                Some(v[1].to_string())
            } else {
                None
            }
        });

        // Try to get amount from bolt11 description tag or amount tag
        let amount_msats = event.tags.iter().find_map(|t| {
            let v = t.as_slice();
            if v.len() >= 2 && v[0] == "amount" {
                v[1].parse::<u64>().ok()
            } else {
                None
            }
        });

        if let (Some(sender_pk), Some(msats)) = (sender, amount_msats) {
            let sats = msats / 1000;
            if sats > 0 {
                *totals.entry(sender_pk).or_insert(0) += sats;
            }
        }
    }

    let mut supporters: Vec<(String, u64)> = totals.into_iter().collect();
    supporters.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(supporters)
}

#[component]
pub fn CodeUserProfile(pubkey: String) -> Element {
    let mut profile_data = use_signal(|| None::<profiles::Profile>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
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
    let mut activities = use_signal(Vec::<Activity>::new);
    let mut sponsors = use_signal(Vec::<(String, u64)>::new);

    // Pinned repos
    let mut pinned_repos = use_signal(Vec::<Repository>::new);
    let mut pinned_naddrs = use_signal(Vec::<String>::new);

    // Sponsor profile cache
    let mut sponsor_profiles = use_signal(HashMap::<String, profiles::Profile>::new);

    // Clipboard copied state
    #[allow(unused_mut)]
    let mut npub_copied = use_signal(|| false);

    let parsed_pubkey = PublicKey::parse(&pubkey)
        .ok();

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

    // Phase 1: Load profile metadata + all counts on mount/pubkey change
    let pubkey_for_load = pubkey.clone();
    use_effect(use_reactive(&pubkey, move |pk| {
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
        activities.set(Vec::new());
        sponsors.set(Vec::new());
        pinned_repos.set(Vec::new());
        pinned_naddrs.set(Vec::new());
        sponsor_profiles.set(HashMap::new());
        is_following.set(false);

        spawn(async move {
            // Wait for client
            loop {
                if *nostr_client::CLIENT_INITIALIZED.read() {
                    break;
                }
                gloo_timers::future::TimeoutFuture::new(100).await;
            }

            let parsed = match PublicKey::parse(&pk)
            {
                Ok(pk) => pk,
                Err(e) => {
                    error.set(Some(format!("Invalid public key: {}", e)));
                    loading.set(false);
                    return;
                }
            };

            // Load profile metadata
            match profiles::fetch_profile(parsed.to_hex()).await {
                Ok(profile) => profile_data.set(Some(profile)),
                Err(e) => log::warn!("Failed to fetch profile: {}", e),
            }

            // Check following status
            if auth_store::is_authenticated() {
                if let Ok(following) = nostr_client::is_following(parsed.to_hex()).await {
                    is_following.set(following);
                }
            }

            // Load repos (default tab) and counts for all tabs
            let repos_result = fetch_user_repositories(&parsed, 50).await;
            let issues_result = fetch_user_issues(&parsed, 50).await;
            let prs_result = fetch_user_prs(&parsed, 50).await;
            let snippets_result = fetch_user_snippets(&parsed, 50).await;
            let activities_result = fetch_user_activity(&parsed, 50).await;
            let sponsors_result = fetch_zap_supporters(&parsed).await;

            let repo_count = repos_result.as_ref().map(|r| r.len()).unwrap_or(0);
            let issue_count = issues_result.as_ref().map(|i| i.len()).unwrap_or(0);
            let pr_count = prs_result.as_ref().map(|p| p.len()).unwrap_or(0);
            let snippet_count = snippets_result.as_ref().map(|s| s.len()).unwrap_or(0);
            let activity_count = activities_result.as_ref().map(|a| a.len()).unwrap_or(0);
            let sponsor_count = sponsors_result.as_ref().map(|s| s.len()).unwrap_or(0);

            // Clone repos before moving into signal for pinned lookup
            let all_repos = repos_result.as_ref().cloned().unwrap_or_default();

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
            if let Ok(a) = activities_result {
                activities.set(a);
            }
            if let Ok(ref s) = sponsors_result {
                sponsors.set(s.clone());
                // Fetch sponsor profiles
                let sponsor_pks: Vec<String> = s.iter().map(|(pk, _)| pk.clone()).collect();
                if !sponsor_pks.is_empty() {
                    if let Ok(profiles_map) = profiles::fetch_profiles_batch(sponsor_pks).await {
                        sponsor_profiles.set(profiles_map);
                    }
                }
            }

            // Fetch pinned repos (Kind 30003 bookmark set with d-tag "code-pinned-repos")
            let pinned_filter = Filter::new()
                .kind(Kind::Custom(30003))
                .author(parsed)
                .identifier("code-pinned-repos");
            if let Ok(pinned_events) = nostr_client::fetch_events_aggregated(
                pinned_filter,
                Duration::from_secs(10),
            ).await {
                if let Some(event) = pinned_events.into_iter().next() {
                    // Extract "a" tags (coordinate references to repos)
                    let coords: Vec<String> = event.tags.iter().filter_map(|t| {
                        let v = t.as_slice();
                        if v.len() >= 2 && v[0] == "a" && v[1].starts_with("30617:") {
                            Some(v[1].to_string())
                        } else {
                            None
                        }
                    }).collect();

                    pinned_naddrs.set(coords.clone());

                    // Match pinned coordinates against fetched repos
                    let pinned: Vec<Repository> = coords.iter().filter_map(|coord| {
                        // coord format: "30617:pubkey:identifier"
                        all_repos.iter().find(|r| {
                            let repo_coord = format!("30617:{}:{}", r.pubkey, r.id);
                            repo_coord == *coord
                        }).cloned()
                    }).take(6).collect();

                    pinned_repos.set(pinned);
                }
            }

            let mut td = default_tab_data_map();
            td.insert(CodeProfileTab::Repositories, TabData { loaded: true, count: repo_count });
            td.insert(CodeProfileTab::Issues, TabData { loaded: true, count: issue_count });
            td.insert(CodeProfileTab::PullRequests, TabData { loaded: true, count: pr_count });
            td.insert(CodeProfileTab::Snippets, TabData { loaded: true, count: snippet_count });
            td.insert(CodeProfileTab::Activity, TabData { loaded: true, count: activity_count });
            td.insert(CodeProfileTab::Sponsors, TabData { loaded: true, count: sponsor_count });
            tab_data.set(td);

            loading.set(false);
        });
    }));

    // Follow/unfollow handler
    let pubkey_for_follow = pubkey.clone();
    let on_follow_click = move |_| {
        let pk = pubkey_for_follow.clone();
        spawn(async move {
            follow_loading.set(true);
            let hex = PublicKey::parse(&pk)
                .map(|k| k.to_hex())
                .unwrap_or(pk);

            if is_following() {
                match nostr_client::unfollow_user(hex).await {
                    Ok(()) => is_following.set(false),
                    Err(e) => log::error!("Failed to unfollow: {}", e),
                }
            } else {
                match nostr_client::follow_user(hex).await {
                    Ok(()) => is_following.set(true),
                    Err(e) => log::error!("Failed to follow: {}", e),
                }
            }
            follow_loading.set(false);
        });
    };

    // Copy npub to clipboard
    let npub_for_copy = npub_display.clone();
    let on_copy_npub = move |_| {
        #[allow(unused_variables)]
        let npub = npub_for_copy.clone();
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            {
                let window = web_sys::window().unwrap();
                let navigator = window.navigator();
                let clipboard = navigator.clipboard();
                let _ = wasm_bindgen_futures::JsFuture::from(
                    clipboard.write_text(&npub),
                )
                .await;
                npub_copied.set(true);
                gloo_timers::future::TimeoutFuture::new(2000).await;
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
                format!("{}...{}", &npub_display[..8], &npub_display[npub_display.len()-4..])
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
    let repo_count = tab_counts.get(&CodeProfileTab::Repositories).map(|t| t.count).unwrap_or(0);
    let issue_count = tab_counts.get(&CodeProfileTab::Issues).map(|t| t.count).unwrap_or(0);
    let pr_count = tab_counts.get(&CodeProfileTab::PullRequests).map(|t| t.count).unwrap_or(0);
    let snippet_count = tab_counts.get(&CodeProfileTab::Snippets).map(|t| t.count).unwrap_or(0);
    let activity_count = tab_counts.get(&CodeProfileTab::Activity).map(|t| t.count).unwrap_or(0);
    let sponsor_count = tab_counts.get(&CodeProfileTab::Sponsors).map(|t| t.count).unwrap_or(0);

    let current_tab = *active_tab.read();

    // Truncated npub for display
    let npub_short = if npub_display.len() > 20 {
        format!("{}...{}", &npub_display[..12], &npub_display[npub_display.len()-4..])
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
                                                    path { d: "M22 11.08V12a10 10 0 1 1-5.93-9.14" }
                                                    polyline { points: "22 4 12 14.01 9 11.01" }
                                                }
                                                span { "{nip05_val}" }
                                            }
                                        }
                                    }

                                    // Follow button (hidden on own profile)
                                    if !is_own_profile && auth_store::is_authenticated() {
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
                                    }
                                }

                                // About
                                if let Some(ref about_text) = about {
                                    p { class: "mt-2 text-sm text-muted-foreground line-clamp-3", "{about_text}" }
                                }

                                // Metadata links row
                                div { class: "mt-3 flex flex-wrap items-center gap-3 text-sm",
                                    if let Some(ref website_url) = website {
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
                                        to: Route::Profile { pubkey: pubkey_for_load.clone() },
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

                // Stats row
                div { class: "px-4 mt-4",
                    div { class: "flex flex-wrap gap-3",
                        StatChip { label: "Repositories", count: repo_count, icon: "repo" }
                        StatChip { label: "Issues", count: issue_count, icon: "issue" }
                        StatChip { label: "Pull Requests", count: pr_count, icon: "pr" }
                        StatChip { label: "Snippets", count: snippet_count, icon: "snippet" }
                        StatChip { label: "Activity", count: activity_count, icon: "activity" }
                        StatChip { label: "Sponsors", count: sponsor_count, icon: "sponsor" }
                    }
                }

                // Pinned repositories section (above contribution graph)
                if !pinned_repos.read().is_empty() {
                    div { class: "px-4 mt-4",
                        div { class: "flex items-center gap-2 mb-3",
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
                                // Pin icon
                                path { d: "M12 17v5" }
                                path { d: "M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 1 1 0 0 0 1-1V4a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v1a1 1 0 0 0 1 1 1 1 0 0 1 1 1z" }
                            }
                            h3 { class: "text-sm font-medium text-foreground", "Pinned" }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                            for repo in pinned_repos.read().iter() {
                                CodeRepoCard {
                                    key: "{repo.naddr}",
                                    repo: repo.clone(),
                                }
                            }
                        }
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
                        TabButton { label: "Activity", tab: CodeProfileTab::Activity, active: current_tab, on_click: move |_| active_tab.set(CodeProfileTab::Activity) }
                        TabButton { label: "Sponsors", tab: CodeProfileTab::Sponsors, active: current_tab, on_click: move |_| active_tab.set(CodeProfileTab::Sponsors) }
                    }
                }

                // Tab content
                div { class: "p-4",
                    match current_tab {
                        CodeProfileTab::Repositories => rsx! {
                            if repos.read().is_empty() {
                                EmptyTabState { label: "repositories" }
                            } else {
                                // Pin/unpin toggle on own profile
                                div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                                    for repo in repos.read().iter() {
                                        div { key: "{repo.naddr}",
                                            class: "relative",
                                            CodeRepoCard {
                                                repo: repo.clone(),
                                            }
                                            if is_own_profile {
                                                {
                                                    let repo_coord = format!("30617:{}:{}", repo.pubkey, repo.id);
                                                    let is_pinned = pinned_naddrs.read().contains(&repo_coord);
                                                    let coord_for_click = repo_coord.clone();
                                                    rsx! {
                                                        button {
                                                            class: if is_pinned {
                                                                "absolute top-2 right-2 p-1.5 rounded-md bg-primary/10 text-primary hover:bg-primary/20 transition z-10"
                                                            } else {
                                                                "absolute top-2 right-2 p-1.5 rounded-md bg-muted text-muted-foreground hover:bg-accent transition z-10"
                                                            },
                                                            title: if is_pinned { "Unpin repository" } else { "Pin repository" },
                                                            onclick: move |_| {
                                                                let coord = coord_for_click.clone();
                                                                spawn(async move {
                                                                    let mut current = pinned_naddrs.read().clone();
                                                                    if let Some(pos) = current.iter().position(|c| *c == coord) {
                                                                        current.remove(pos);
                                                                    } else if current.len() < 6 {
                                                                        current.push(coord);
                                                                    }
                                                                    // Publish updated Kind 30003 event
                                                                    if let Err(e) = publish_pinned_repos(&current).await {
                                                                        log::error!("Failed to publish pinned repos: {}", e);
                                                                        return;
                                                                    }
                                                                    pinned_naddrs.set(current.clone());
                                                                    // Update pinned repos display
                                                                    let all = repos.read().clone();
                                                                    let pinned: Vec<Repository> = current.iter().filter_map(|c| {
                                                                        all.iter().find(|r| format!("30617:{}:{}", r.pubkey, r.id) == *c).cloned()
                                                                    }).take(6).collect();
                                                                    pinned_repos.set(pinned);
                                                                });
                                                            },
                                                            svg {
                                                                class: "w-4 h-4",
                                                                xmlns: "http://www.w3.org/2000/svg",
                                                                width: "24",
                                                                height: "24",
                                                                view_box: "0 0 24 24",
                                                                fill: if is_pinned { "currentColor" } else { "none" },
                                                                stroke: "currentColor",
                                                                stroke_width: "2",
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                path { d: "M12 17v5" }
                                                                path { d: "M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 1 1 0 0 0 1-1V4a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v1a1 1 0 0 0 1 1 1 1 0 0 1 1 1z" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
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
                        CodeProfileTab::Activity => rsx! {
                            if activities.read().is_empty() {
                                EmptyTabState { label: "activity" }
                            } else {
                                div { class: "space-y-1",
                                    for activity in activities.read().iter() {
                                        ActivityTimelineItem { activity: activity.clone() }
                                    }
                                }
                            }
                        },
                        CodeProfileTab::Sponsors => rsx! {
                            if sponsors.read().is_empty() {
                                EmptyTabState { label: "sponsors" }
                            } else {
                                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3",
                                    for (pk, total_sats) in sponsors.read().iter() {
                                        SponsorCard {
                                            key: "{pk}",
                                            pubkey: pk.clone(),
                                            total_sats: *total_sats,
                                            profile: sponsor_profiles.read().get(pk).cloned(),
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

/// Publish pinned repos as a Kind 30003 bookmark set with d-tag "code-pinned-repos"
async fn publish_pinned_repos(coords: &[String]) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    use nostr_sdk::{Alphabet, EventBuilder, SingleLetterTag, Tag, TagKind};

    let mut builder = EventBuilder::new(Kind::Custom(30003), "")
        .tag(Tag::identifier("code-pinned-repos"));

    for coord in coords {
        builder = builder.tag(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::A)),
            vec![coord.clone()],
        ));
    }

    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish pinned repos: {}", e))?;

    Ok(())
}

#[component]
fn ActivityTimelineItem(activity: Activity) -> Element {
    let time_ago = format_time_ago(activity.created_at);
    let type_label = activity.activity_type.to_string();

    rsx! {
        div { class: "flex items-start gap-3 p-3 rounded-lg hover:bg-muted/50 transition",
            // Activity type icon
            div { class: "shrink-0 mt-0.5",
                match activity.activity_type {
                    ActivityType::RepoCreated => rsx! {
                        div { class: "w-8 h-8 rounded-full bg-green-500/10 flex items-center justify-center",
                            svg {
                                class: "w-4 h-4 text-green-500",
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
                        }
                    },
                    ActivityType::IssueOpened => rsx! {
                        div { class: "w-8 h-8 rounded-full bg-orange-500/10 flex items-center justify-center",
                            svg {
                                class: "w-4 h-4 text-orange-500",
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
                                circle { cx: "12", cy: "12", r: "1" }
                            }
                        }
                    },
                    ActivityType::PullRequestOpened => rsx! {
                        div { class: "w-8 h-8 rounded-full bg-purple-500/10 flex items-center justify-center",
                            svg {
                                class: "w-4 h-4 text-purple-500",
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
                        }
                    },
                    ActivityType::CommentPosted => rsx! {
                        div { class: "w-8 h-8 rounded-full bg-blue-500/10 flex items-center justify-center",
                            svg {
                                class: "w-4 h-4 text-blue-500",
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
                        }
                    },
                    ActivityType::StatusChanged => rsx! {
                        div { class: "w-8 h-8 rounded-full bg-yellow-500/10 flex items-center justify-center",
                            svg {
                                class: "w-4 h-4 text-yellow-500",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z" }
                                line { x1: "7", y1: "7", x2: "7.01", y2: "7" }
                            }
                        }
                    },
                    ActivityType::SnippetCreated => rsx! {
                        div { class: "w-8 h-8 rounded-full bg-cyan-500/10 flex items-center justify-center",
                            svg {
                                class: "w-4 h-4 text-cyan-500",
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
                        }
                    },
                    ActivityType::ReviewSubmitted => rsx! {
                        div { class: "w-8 h-8 rounded-full bg-pink-500/10 flex items-center justify-center",
                            svg {
                                class: "w-4 h-4 text-pink-500",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                                polyline { points: "14 2 14 8 20 8" }
                                line { x1: "16", y1: "13", x2: "8", y2: "13" }
                                line { x1: "16", y1: "17", x2: "8", y2: "17" }
                                polyline { points: "10 9 9 9 8 9" }
                            }
                        }
                    },
                }
            }

            // Content
            div { class: "flex-1 min-w-0",
                p { class: "text-sm font-medium text-foreground truncate", "{activity.title}" }
                p { class: "text-xs text-muted-foreground mt-0.5", "{type_label}" }
            }

            // Timestamp
            span { class: "text-xs text-muted-foreground shrink-0", "{time_ago}" }
        }
    }
}

#[component]
fn SponsorCard(
    pubkey: String,
    total_sats: u64,
    profile: Option<profiles::Profile>,
) -> Element {
    let display_name = profile
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| {
            if pubkey.len() > 16 {
                format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-4..])
            } else {
                pubkey.clone()
            }
        });
    let avatar_url = profile
        .as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| {
            format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", pubkey)
        });

    let sats_display = if total_sats >= 1_000_000 {
        format!("{:.1}M sats", total_sats as f64 / 1_000_000.0)
    } else if total_sats >= 1_000 {
        format!("{:.1}k sats", total_sats as f64 / 1_000.0)
    } else {
        format!("{} sats", total_sats)
    };

    rsx! {
        Link {
            to: Route::CodeUserProfile { pubkey: pubkey.clone() },
            class: "block bg-card border border-border rounded-lg p-4 hover:bg-accent/50 transition",
            div { class: "flex items-center gap-3",
                img {
                    class: "w-10 h-10 rounded-full object-cover bg-muted",
                    src: "{avatar_url}",
                    alt: "{display_name}",
                }
                div { class: "flex-1 min-w-0",
                    p { class: "text-sm font-medium text-foreground truncate", "{display_name}" }
                    p { class: "text-xs text-muted-foreground flex items-center gap-1",
                        "⚡"
                        "{sats_display}"
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
                "activity" => rsx! {
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
                        polyline { points: "22 12 18 12 15 21 9 3 6 12 2 12" }
                    }
                },
                "sponsor" => rsx! {
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
                        path { d: "M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" }
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
