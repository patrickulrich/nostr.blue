//! Repository Releases Page
//!
//! View and create releases for a repository.
//! Follows patterns from code_repo_issues.rs.
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::fetch_repository;
use crate::services::git_hosting::releases::{delete_release, fetch_repo_releases, publish_release};
use crate::stores::{auth_store, nostr_client};
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::format_relative_time_or;
use crate::utils::nip34::{decode_naddr, Release, Repository};
use crate::utils::permissions;
use crate::utils::validation::is_valid_http_url;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::prelude::EventId;
use std::collections::HashMap;
/// Repository releases page component
#[component]
pub fn CodeRepoReleases(naddr: String) -> Element {
    let mut releases = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut show_new_form = use_signal(|| false);
    let mut repo_data = use_signal(|| None::<Repository>);
    let mut repo_error = use_signal(|| None::<String>);
    let mut fetch_trigger = use_signal(|| 0u32);
    let mut request_gen = use_signal(|| 0u32);
    use_effect(use_reactive(&naddr, move |n| {
        let _trigger = *fetch_trigger.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let captured_gen = request_gen.peek().wrapping_add(1);
        request_gen.set(captured_gen);
        repo_data.set(None);
        repo_error.set(None);
        releases.set(vec![]);
        loading.set(true);
        spawn(async move {
            let n_clone = n.clone();
            let (repo_res, releases_res) = futures::join!(
                fetch_repository(&n),
                fetch_repo_releases(&n_clone)
            );
            if *request_gen.peek() != captured_gen { return; }
            match repo_res {
                Ok(repo) => {
                    repo_data.set(Some(repo));
                    repo_error.set(None);
                }
                Err(e) => {
                    repo_error.set(Some(format!("Failed to load repository: {}", e)));
                }
            }
            match releases_res {
                Ok(fetched) => {
                    // Deduplicate by tag_name, keeping the newest release per tag
                    let mut by_tag: HashMap<String, Release> = HashMap::new();
                    for release in fetched {
                        by_tag
                            .entry(release.tag_name.clone())
                            .and_modify(|existing| {
                                if release.created_at > existing.created_at {
                                    *existing = release.clone();
                                }
                            })
                            .or_insert(release);
                    }
                    let mut deduped: Vec<Release> = by_tag.into_values().collect();
                    deduped.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                    releases.set(deduped);
                    error.set(None);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    }));
    let auth = auth_store::AUTH_STATE.read();
    let user_pubkey = auth.pubkey.clone().unwrap_or_default();
    let is_authenticated = auth.is_authenticated;
    let can_create_release = is_authenticated
        && repo_data
            .read()
            .as_ref()
            .map(|r| permissions::is_owner(&user_pubkey, r) || permissions::is_maintainer(&user_pubkey, r))
            .unwrap_or(false);
    let all_releases = releases.read();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeRepo {
                                naddr: naddr.clone(),
                            },
                            class: "text-muted-foreground hover:text-foreground",
                            aria_label: "Back to repository",
                            dangerous_inner_html: icons::ARROW_LEFT,
                        }
                        div {
                            h1 { class: "text-xl font-bold flex items-center gap-2",
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
                                    path { d: "M12 2L2 7l10 5 10-5-10-5z" }
                                    path { d: "M2 17l10 5 10-5" }
                                    path { d: "M2 12l10 5 10-5" }
                                }
                                "Releases"
                            }
                            p { class: "text-sm text-muted-foreground",
                                if !*loading.read() {
                                    "{all_releases.len()} releases"
                                }
                            }
                        }
                    }
                    if can_create_release {
                        button {
                            r#type: "button",
                            class: "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition flex items-center gap-1",
                            onclick: move |_| {
                                let current = *show_new_form.read();
                                show_new_form.set(!current);
                            },
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
                                line { x1: "12", y1: "5", x2: "12", y2: "19" }
                                line { x1: "5", y1: "12", x2: "19", y2: "12" }
                            }
                            "New Release"
                        }
                    }
                }
            }
            div { class: "p-4 space-y-6",
                if let Some(err) = repo_error.read().as_ref() {
                    div { class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{err}"
                    }
                }
                if *show_new_form.read() {
                    NewReleaseForm {
                        naddr: naddr.clone(),
                        on_published: move |_| {
                            show_new_form.set(false);
                            fetch_trigger.with_mut(|v| *v = v.wrapping_add(1));
                        },
                    }
                }
                if let Some(err) = error.read().as_ref() {
                    div { class: "text-center text-red-400 py-4", "{err}" }
                } else if *loading.read() {
                    LoadingSkeleton {}
                } else if all_releases.is_empty() {
                    EmptyReleases {}
                } else {
                    div { class: "space-y-4",
                        {
                            let repo_clone = repo_data.read().clone();
                            rsx! {
                                for release in all_releases.iter() {
                                    ReleaseCard {
                                        key: "{release.event_id}",
                                        release: release.clone(),
                                        naddr: naddr.clone(),
                                        repo: repo_clone.clone(),
                                        user_pubkey: user_pubkey.clone(),
                                        is_authenticated: is_authenticated,
                                        on_mutated: move |_| {
                                            fetch_trigger.with_mut(|v| *v = v.wrapping_add(1));
                                        },
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
#[component]
fn ReleaseCard(
    release: Release,
    naddr: String,
    #[props(default = None)] repo: Option<Repository>,
    #[props(default = String::new())] user_pubkey: String,
    #[props(default = false)] is_authenticated: bool,
    on_mutated: EventHandler<()>,
) -> Element {
    let mut show_edit = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);
    let mut delete_error = use_signal(|| None::<String>);
    let mut is_deleting = use_signal(|| false);
    let author_profile = PROFILE_CACHE.read().peek(&release.pubkey).cloned();
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&release.pubkey));
    let title = release
        .title
        .clone()
        .unwrap_or_else(|| release.tag_name.clone());
    let can_edit = is_authenticated
        && (user_pubkey == release.pubkey
            || repo
                .as_ref()
                .is_some_and(|r| permissions::is_owner(&user_pubkey, r) || permissions::is_maintainer(&user_pubkey, r)));
    if *show_edit.read() {
        return rsx! {
            EditReleaseForm {
                release: release.clone(),
                naddr: naddr.clone(),
                on_cancel: move |_| show_edit.set(false),
                on_saved: move |_| {
                    show_edit.set(false);
                    on_mutated.call(());
                },
            }
        };
    }
    let release_event_id = release.event_id.clone();
    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4",
            div { class: "flex items-start justify-between gap-3",
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2 mb-2",
                        span { class: "px-2 py-0.5 text-xs font-mono rounded bg-green-500/10 text-green-500 border border-green-500/20",
                            "{release.tag_name}"
                        }
                        h3 { class: "text-lg font-semibold truncate", "{title}" }
                        if release.prerelease {
                            span { class: "px-2 py-0.5 text-xs rounded bg-yellow-500/10 text-yellow-500 border border-yellow-500/20",
                                "Pre-release"
                            }
                        }
                    }
                    if !release.description.is_empty() {
                        p { class: "text-sm text-muted-foreground mb-3 line-clamp-3 whitespace-pre-wrap",
                            "{release.description}"
                        }
                    }
                    div { class: "flex items-center gap-2 text-xs text-muted-foreground",
                        span { "{author_name}" }
                        span { "released " {format_relative_time_or(release.created_at, "Unknown")} }
                    }
                }
                if can_edit {
                    div { class: "flex items-center gap-1 shrink-0",
                        button {
                            r#type: "button",
                            class: "px-2 py-1 text-xs text-muted-foreground hover:text-foreground transition rounded hover:bg-accent",
                            onclick: move |_| show_edit.set(true),
                            "Edit"
                        }
                        button {
                            r#type: "button",
                            class: "px-2 py-1 text-xs text-muted-foreground hover:text-destructive transition rounded hover:bg-destructive/10",
                            onclick: move |_| {
                                delete_error.set(None);
                                show_delete_confirm.set(true);
                            },
                            "Delete"
                        }
                    }
                }
            }
            if *show_delete_confirm.read() {
                div { class: "mt-3 p-3 bg-destructive/10 border border-destructive/20 rounded-lg",
                    p { class: "text-sm text-destructive mb-2", "Are you sure you want to delete this release?" }
                    if let Some(err) = delete_error.read().as_ref() {
                        p { class: "text-sm text-destructive mb-2", "{err}" }
                    }
                    div { class: "flex gap-2",
                        button {
                            r#type: "button",
                            class: "px-3 py-1.5 text-xs bg-destructive text-destructive-foreground rounded-lg disabled:opacity-50",
                            disabled: *is_deleting.read(),
                            onclick: {
                                let event_id_hex = release_event_id.clone();
                                move |_| {
                                    if *is_deleting.peek() { return; }
                                    is_deleting.set(true);
                                    delete_error.set(None);
                                    let eid = event_id_hex.clone();
                                    spawn(async move {
                                        match EventId::from_hex(&eid) {
                                            Ok(event_id) => {
                                                match delete_release(event_id).await {
                                                    Ok(_) => {
                                                        show_delete_confirm.set(false);
                                                        on_mutated.call(());
                                                    }
                                                    Err(e) => {
                                                        delete_error.set(Some(e));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                delete_error.set(Some(format!("Invalid event ID: {}", e)));
                                            }
                                        }
                                        is_deleting.set(false);
                                    });
                                }
                            },
                            if *is_deleting.read() { "Deleting..." } else { "Delete" }
                        }
                        button {
                            r#type: "button",
                            class: "px-3 py-1.5 text-xs border border-border rounded-lg hover:bg-accent transition",
                            onclick: move |_| show_delete_confirm.set(false),
                            "Cancel"
                        }
                    }
                }
            }
            if !release.assets.is_empty() {
                div { class: "mt-3 pt-3 border-t border-border",
                    p { class: "text-xs font-medium text-muted-foreground mb-2", "Assets" }
                    div { class: "space-y-1",
                        for (i, asset) in release.assets.iter().enumerate() {
                            if is_valid_http_url(asset) {
                            a {
                                key: "{i}",
                                href: "{asset}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "flex items-center gap-2 text-sm text-primary hover:underline",
                                svg {
                                    class: "w-3.5 h-3.5 shrink-0",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                                    polyline { points: "7 10 12 15 17 10" }
                                    line { x1: "12", y1: "15", x2: "12", y2: "3" }
                                }
                                "{asset}"
                            }
                            }
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn EditReleaseForm(
    release: Release,
    naddr: String,
    on_cancel: EventHandler<()>,
    on_saved: EventHandler<()>,
) -> Element {
    let tag_name = release.tag_name.clone();
    let mut title = use_signal(|| release.title.clone().unwrap_or_default());
    let mut description = use_signal(|| release.description.clone());
    let mut asset_urls = use_signal(|| {
        let mut urls = release.assets.clone();
        if urls.is_empty() {
            urls.push(String::new());
        }
        urls
    });
    let mut prerelease = use_signal(|| release.prerelease);
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let handle_submit = {
        let naddr = naddr.clone();
        let tag_name_for_submit = tag_name.clone();
        move |_| {
            if *is_publishing.peek() { return; }
            let tag = tag_name_for_submit.clone();
            let title_val = title.read().clone();
            let desc = description.read().clone();
            let assets = asset_urls.read().clone();
            let is_pre = *prerelease.read();
            let naddr = naddr.clone();
            if tag.trim().is_empty() {
                error_message.set(Some("Tag name is required".to_string()));
                return;
            }
            if desc.trim().is_empty() {
                error_message.set(Some("Description is required".to_string()));
                return;
            }
            let non_empty_assets: Vec<String> = assets
                .iter()
                .filter(|a| !a.trim().is_empty())
                .cloned()
                .collect();
            for url in &non_empty_assets {
                if !is_valid_http_url(url) {
                    error_message.set(Some(format!("Invalid asset URL: {}", url)));
                    return;
                }
            }
            is_publishing.set(true);
            error_message.set(None);
            spawn(async move {
                let coord = match decode_naddr(&naddr) {
                    Ok(c) => c,
                    Err(e) => {
                        error_message.set(Some(format!("Invalid naddr: {}", e)));
                        is_publishing.set(false);
                        return;
                    }
                };
                let title_opt = if title_val.is_empty() {
                    None
                } else {
                    Some(title_val.as_str())
                };
                let asset_refs: Vec<&str> = non_empty_assets.iter().map(|a| a.as_str()).collect();
                match publish_release(&coord, &tag, title_opt, &desc, &asset_refs, is_pre).await {
                    Ok(_) => {
                        on_saved.call(());
                    }
                    Err(e) => {
                        error_message.set(Some(e));
                    }
                }
                is_publishing.set(false);
            });
        }
    };
    rsx! {
        div { class: "bg-card border border-primary/30 rounded-lg p-4 space-y-4",
            h3 { class: "text-lg font-semibold", "Edit Release" }
            if let Some(error) = error_message.read().as_ref() {
                div { class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                    "{error}"
                }
            }
            div {
                label { class: "block text-sm font-medium mb-2",
                    "Tag Name "
                    span { class: "text-destructive", "*" }
                }
                input {
                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground text-sm opacity-60 cursor-not-allowed",
                    r#type: "text",
                    placeholder: "v1.0.0",
                    value: "{tag_name}",
                    readonly: true,
                }
            }
            div {
                label { class: "block text-sm font-medium mb-2",
                    "Title "
                    span { class: "text-muted-foreground font-normal", "(optional)" }
                }
                input {
                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground text-sm",
                    r#type: "text",
                    placeholder: "Release title",
                    value: "{title}",
                    oninput: move |e| title.set(e.value()),
                }
            }
            div {
                label { class: "block text-sm font-medium mb-2",
                    "Description "
                    span { class: "text-destructive", "*" }
                }
                textarea {
                    class: "w-full h-32 px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground text-sm resize-y",
                    placeholder: "Describe what's in this release...",
                    value: "{description}",
                    oninput: move |e| description.set(e.value()),
                }
            }
            div {
                label { class: "block text-sm font-medium mb-2",
                    "Asset URLs "
                    span { class: "text-muted-foreground font-normal", "(optional)" }
                }
                div { class: "space-y-2",
                    for (i , _) in asset_urls.read().iter().enumerate() {
                        div { key: "{i}", class: "flex gap-2",
                            input {
                                class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground text-sm",
                                r#type: "url",
                                placeholder: "https://example.com/asset.tar.gz",
                                value: "{asset_urls.read()[i]}",
                                oninput: move |e| {
                                    let mut urls = asset_urls.read().clone();
                                    urls[i] = e.value();
                                    asset_urls.set(urls);
                                },
                            }
                            button {
                                class: "px-2 py-2 text-muted-foreground hover:text-destructive transition rounded hover:bg-destructive/10",
                                r#type: "button",
                                onclick: move |_| {
                                    let mut urls = asset_urls.read().clone();
                                    urls.remove(i);
                                    if urls.is_empty() {
                                        urls.push(String::new());
                                    }
                                    asset_urls.set(urls);
                                },
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
                                    line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                    line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                }
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "text-sm text-primary hover:underline",
                        onclick: move |_| {
                            let mut urls = asset_urls.read().clone();
                            urls.push(String::new());
                            asset_urls.set(urls);
                        },
                        "+ Add asset URL"
                    }
                }
            }
            div { class: "flex items-center gap-2",
                input {
                    class: "rounded border-border",
                    r#type: "checkbox",
                    id: "edit-prerelease-{tag_name}",
                    checked: *prerelease.read(),
                    onchange: move |e| prerelease.set(e.checked()),
                }
                label { class: "text-sm", r#for: "edit-prerelease-{tag_name}", "This is a pre-release" }
            }
            div { class: "flex gap-2",
                button {
                    r#type: "button",
                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                    disabled: *is_publishing.read(),
                    onclick: handle_submit,
                    if *is_publishing.read() {
                        "Saving..."
                    } else {
                        "Save Changes"
                    }
                }
                button {
                    r#type: "button",
                    class: "px-4 py-2 border border-border rounded-lg hover:bg-accent transition",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
        }
    }
}
#[component]
fn NewReleaseForm(naddr: String, on_published: EventHandler<()>) -> Element {
    let mut tag_name = use_signal(String::new);
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut asset_urls = use_signal(|| vec![String::new()]);
    let mut prerelease = use_signal(|| false);
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let handle_submit = {
        let naddr = naddr.clone();
        move |_| {
            if *is_publishing.peek() { return; }
            let tag = tag_name.read().trim().to_string();
            let title_val = title.read().clone();
            let desc = description.read().clone();
            let assets = asset_urls.read().clone();
            let is_pre = *prerelease.read();
            let naddr = naddr.clone();
            if tag.is_empty() {
                error_message.set(Some("Tag name is required".to_string()));
                return;
            }
            if desc.trim().is_empty() {
                error_message.set(Some("Description is required".to_string()));
                return;
            }
            is_publishing.set(true);
            error_message.set(None);
            spawn(async move {
                let coord = match decode_naddr(&naddr) {
                    Ok(c) => c,
                    Err(e) => {
                        error_message.set(Some(format!("Invalid naddr: {}", e)));
                        is_publishing.set(false);
                        return;
                    }
                };
                let title_opt = if title_val.is_empty() {
                    None
                } else {
                    Some(title_val.as_str())
                };
                let non_empty_assets: Vec<&str> = assets
                    .iter()
                    .filter(|a| !a.trim().is_empty())
                    .map(|a| a.as_str())
                    .collect();
                for url in &non_empty_assets {
                    if !is_valid_http_url(url) {
                        error_message.set(Some(format!("Invalid asset URL: {}", url)));
                        is_publishing.set(false);
                        return;
                    }
                }
                match publish_release(&coord, &tag, title_opt, &desc, &non_empty_assets, is_pre).await {
                    Ok(_) => {
                        tag_name.set(String::new());
                        title.set(String::new());
                        description.set(String::new());
                        asset_urls.set(vec![String::new()]);
                        prerelease.set(false);
                        on_published.call(());
                    }
                    Err(e) => {
                        error_message.set(Some(e));
                    }
                }
                is_publishing.set(false);
            });
        }
    };
    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
            h3 { class: "text-lg font-semibold", "Create New Release" }
            if let Some(error) = error_message.read().as_ref() {
                div { class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                    "{error}"
                }
            }
            div {
                label { class: "block text-sm font-medium mb-2",
                    "Tag Name "
                    span { class: "text-destructive", "*" }
                }
                input {
                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground text-sm",
                    r#type: "text",
                    placeholder: "v1.0.0",
                    value: "{tag_name}",
                    oninput: move |e| tag_name.set(e.value()),
                }
            }
            div {
                label { class: "block text-sm font-medium mb-2",
                    "Title "
                    span { class: "text-muted-foreground font-normal", "(optional)" }
                }
                input {
                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground text-sm",
                    r#type: "text",
                    placeholder: "Release title",
                    value: "{title}",
                    oninput: move |e| title.set(e.value()),
                }
            }
            div {
                label { class: "block text-sm font-medium mb-2",
                    "Description "
                    span { class: "text-destructive", "*" }
                }
                textarea {
                    class: "w-full h-32 px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground text-sm resize-y",
                    placeholder: "Describe what's in this release...",
                    value: "{description}",
                    oninput: move |e| description.set(e.value()),
                }
            }
            div {
                label { class: "block text-sm font-medium mb-2",
                    "Asset URLs "
                    span { class: "text-muted-foreground font-normal", "(optional)" }
                }
                div { class: "space-y-2",
                    for (i , _) in asset_urls.read().iter().enumerate() {
                        div { key: "{i}", class: "flex gap-2",
                            input {
                                class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground text-sm",
                                r#type: "url",
                                placeholder: "https://example.com/asset.tar.gz",
                                value: "{asset_urls.read()[i]}",
                                oninput: move |e| {
                                    let mut urls = asset_urls.read().clone();
                                    urls[i] = e.value();
                                    asset_urls.set(urls);
                                },
                            }
                            button {
                                class: "px-2 py-2 text-muted-foreground hover:text-destructive transition rounded hover:bg-destructive/10",
                                r#type: "button",
                                onclick: move |_| {
                                    let mut urls = asset_urls.read().clone();
                                    urls.remove(i);
                                    if urls.is_empty() {
                                        urls.push(String::new());
                                    }
                                    asset_urls.set(urls);
                                },
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
                                    line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                    line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                }
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "text-sm text-primary hover:underline",
                        onclick: move |_| {
                            let mut urls = asset_urls.read().clone();
                            urls.push(String::new());
                            asset_urls.set(urls);
                        },
                        "+ Add asset URL"
                    }
                }
            }
            div { class: "flex items-center gap-2",
                input {
                    class: "rounded border-border",
                    r#type: "checkbox",
                    id: "prerelease",
                    checked: *prerelease.read(),
                    onchange: move |e| prerelease.set(e.checked()),
                }
                label { class: "text-sm", r#for: "prerelease", "This is a pre-release" }
            }
            button {
                r#type: "button",
                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                disabled: *is_publishing.read(),
                onclick: handle_submit,
                if *is_publishing.read() {
                    "Publishing..."
                } else {
                    "Publish Release"
                }
            }
        }
    }
}
#[component]
fn EmptyReleases() -> Element {
    rsx! {
        div { class: "text-center py-12",
            div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                svg {
                    class: "w-8 h-8 text-muted-foreground",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M12 2L2 7l10 5 10-5-10-5z" }
                    path { d: "M2 17l10 5 10-5" }
                    path { d: "M2 12l10 5 10-5" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Releases" }
            p { class: "text-muted-foreground text-sm", "This repository has no releases yet." }
        }
    }
}
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-4 animate-pulse",
            for i in 0..2 {
                div { key: "{i}", class: "p-4 border border-border rounded-lg",
                    div { class: "flex items-center gap-2 mb-3",
                        div { class: "h-5 bg-muted rounded w-16" }
                        div { class: "h-5 bg-muted rounded w-1/3" }
                    }
                    div { class: "h-3 bg-muted rounded w-2/3 mb-2" }
                    div { class: "h-3 bg-muted rounded w-1/3" }
                }
            }
        }
    }
}
