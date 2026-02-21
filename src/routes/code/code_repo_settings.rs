//! Repository Settings Page
//!
//! Manage repository settings for NIP-34 repositories.
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, publish_repository_with_extras};
use crate::services::git_hosting::milestones::milestones_to_tags;
use crate::stores::profiles::PROFILE_CACHE;
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::Repository;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::PublicKey;
use nostr_sdk::prelude::{Tag, TagKind};
use std::borrow::Cow;
/// Repository settings page component
#[component]
pub fn CodeRepoSettings(naddr: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let nav = use_navigator();
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let naddr_for_effect = naddr.clone();
    use_effect(move || {
        let n = naddr_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            let result = fetch_repository(&n).await;
            repo_result.set(Some(result));
        });
    });
    let mut repo_name = use_signal(String::new);
    let mut repo_description = use_signal(String::new);
    let mut clone_url = use_signal(String::new);
    let mut web_url = use_signal(String::new);
    let mut relay_list = use_signal(Vec::<String>::new);
    let mut new_relay_url = use_signal(String::new);
    let mut maintainer_list = use_signal(Vec::<String>::new);
    let mut new_maintainer = use_signal(String::new);
    let mut form_initialized = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| None::<String>);
    let mut save_success = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);
    let mut zap_splits = use_signal(Vec::<(String, String, u32)>::new);
    let mut new_split_pubkey = use_signal(String::new);
    let mut new_split_weight = use_signal(|| 50u32);
    let mut milestones = use_signal(Vec::<crate::services::git_hosting::milestones::Milestone>::new);
    let mut new_milestone_name = use_signal(String::new);
    let mut required_approvals = use_signal(|| 0u32);
    let mut topics_list = use_signal(Vec::<String>::new);
    let mut new_topic = use_signal(String::new);
    if let Some(Ok(r)) = repo_result.read().as_ref() {
        if !*form_initialized.read() {
            repo_name.set(r.name.clone().unwrap_or_default());
            repo_description.set(r.description.clone().unwrap_or_default());
            clone_url.set(r.clone.first().cloned().unwrap_or_default());
            web_url.set(r.web.first().cloned().unwrap_or_default());
            relay_list.set(r.relays.clone());
            maintainer_list.set(r.maintainers.clone());
            // Initialize zap splits from parsed event data, or default to owner at 100%
            if !r.zap_splits.is_empty() {
                zap_splits.set(r.zap_splits.clone());
            } else {
                zap_splits.set(vec![(r.pubkey.clone(), String::new(), 100)]);
            }
            milestones.set(r.milestones.clone());
            required_approvals.set(r.required_approvals);
            topics_list.set(r.topics.clone());
            form_initialized.set(true);
        }
    }
    let is_owner = if let Some(Ok(r)) = repo_result.read().as_ref() {
        auth.pubkey.as_ref().map(|pk| pk == &r.pubkey).unwrap_or(false)
    } else {
        false
    };
    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState { naddr: naddr.clone() }
        };
    }
    if !is_owner && repo_result.read().is_some() {
        return rsx! {
            NotOwnerState { naddr: naddr.clone() }
        };
    }
    let handle_save = {
        let _naddr = naddr.clone();
        move |_| {
            let repo_data = match repo_result.read().as_ref() {
                Some(Ok(r)) => r.clone(),
                _ => return,
            };
            let name = repo_name.read().clone();
            let description = repo_description.read().clone();
            let clone = clone_url.read().clone();
            let web = web_url.read().clone();
            spawn(async move {
                is_saving.set(true);
                save_error.set(None);
                save_success.set(false);
                let clone_urls: Vec<&str> = if clone.is_empty() {
                    vec![]
                } else {
                    vec![clone.as_str()]
                };
                let web_urls: Vec<&str> = if web.is_empty() {
                    vec![]
                } else {
                    vec![web.as_str()]
                };
                let relay_snapshot: Vec<String> = relay_list.read().clone();
                let relays: Vec<&str> = relay_snapshot.iter().map(|s| s.as_str()).collect();
                let maintainer_snapshot: Vec<String> = maintainer_list.read().clone();
                let maintainer_keys: Vec<PublicKey> = maintainer_snapshot
                    .iter()
                    .filter_map(|s| PublicKey::parse(s).ok())
                    .collect();
                let name_opt = if name.is_empty() { None } else { Some(name.as_str()) };
                let desc_opt = if description.is_empty() {
                    None
                } else {
                    Some(description.as_str())
                };
                // Validate zap split weights total 100%
                let splits_snapshot: Vec<(String, String, u32)> = zap_splits.read().clone();
                let total_weight: u32 = splits_snapshot.iter().map(|(_, _, w)| *w).sum();
                if !splits_snapshot.is_empty() && total_weight != 100 {
                    save_error.set(Some(format!("Zap split weights must total 100% (currently {}%)", total_weight)));
                    is_saving.set(false);
                    return;
                }
                // Build extra tags for zap splits, milestones, required approvals
                let mut extra_tags: Vec<Tag> = Vec::new();
                // Zap split tags — use each split's stored relay, falling back to first repo relay
                let default_relay = relay_snapshot.first()
                    .map(|r| r.to_string())
                    .unwrap_or_default();
                for (pubkey, relay, weight) in &splits_snapshot {
                    if let Ok(pk) = PublicKey::parse(pubkey) {
                        let relay_to_use = if relay.is_empty() {
                            default_relay.clone()
                        } else {
                            relay.clone()
                        };
                        extra_tags.push(Tag::custom(
                            TagKind::Custom(Cow::Borrowed("zap")),
                            [pk.to_hex(), relay_to_use, weight.to_string()],
                        ));
                    }
                }
                // Milestone tags
                let milestones_snapshot = milestones.read().clone();
                for tag_values in milestones_to_tags(&milestones_snapshot) {
                    if tag_values.len() >= 2 {
                        let kind = TagKind::Custom(Cow::Owned(tag_values[0].clone()));
                        let values: Vec<String> = tag_values[1..].to_vec();
                        extra_tags.push(Tag::custom(kind, values));
                    }
                }
                // Topic tags
                let topics_snapshot: Vec<String> = topics_list.read().clone();
                for topic in &topics_snapshot {
                    extra_tags.push(Tag::hashtag(topic.as_str()));
                }
                // Required approvals tag
                let req_approvals = *required_approvals.read();
                if req_approvals > 0 {
                    extra_tags.push(Tag::custom(
                        TagKind::Custom(Cow::Borrowed("required-approvals")),
                        [req_approvals.to_string()],
                    ));
                }
                match publish_repository_with_extras(
                        &repo_data.id,
                        name_opt,
                        desc_opt,
                        &clone_urls,
                        &web_urls,
                        &relays,
                        &maintainer_keys,
                        &extra_tags,
                    )
                    .await
                {
                    Ok(_) => {
                        save_success.set(true);
                    }
                    Err(e) => {
                        save_error.set(Some(e));
                    }
                }
                is_saving.set(false);
            });
        }
    };
    let repo_id = match &*repo_result.read() {
        Some(Ok(r)) => r.id.clone(),
        _ => "".to_string(),
    };
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
                            dangerous_inner_html: icons::ARROW_LEFT,
                        }
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
                                circle { cx: "12", cy: "12", r: "3" }
                                path { d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" }
                            }
                            "Settings"
                        }
                    }
                    button {
                        class: "px-4 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                        disabled: *is_saving.read(),
                        onclick: handle_save,
                        if *is_saving.read() {
                            svg {
                                class: "w-4 h-4 animate-spin",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                circle {
                                    class: "opacity-25",
                                    cx: "12",
                                    cy: "12",
                                    r: "10",
                                    stroke: "currentColor",
                                    stroke_width: "4",
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z",
                                }
                            }
                            "Saving..."
                        } else {
                            "Save Changes"
                        }
                    }
                }
            }
            div { class: "p-4 max-w-2xl mx-auto space-y-6",
                if *save_success.read() {
                    div { class: "p-4 bg-green-500/10 border border-green-500/20 rounded-lg text-green-600 dark:text-green-400 text-sm flex items-center gap-2",
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
                            path { d: "M22 11.08V12a10 10 0 1 1-5.93-9.14" }
                            polyline { points: "22 4 12 14.01 9 11.01" }
                        }
                        "Settings saved successfully!"
                    }
                }
                if let Some(error) = save_error.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }
                match &*repo_result.read() {
                    None => rsx! {
                        LoadingSkeleton {}
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                            "Failed to load repository: {e}"
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        div { class: "space-y-4",
                            h2 { class: "font-semibold text-lg", "General" }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Repository ID" }
                                input {
                                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono text-muted-foreground cursor-not-allowed",
                                    r#type: "text",
                                    value: "{repo_id}",
                                    disabled: true,
                                }
                                p { class: "text-xs text-muted-foreground mt-1",
                                    "This is your repository's unique identifier and cannot be changed."
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Display Name" }
                                input {
                                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "My Project",
                                    value: "{repo_name}",
                                    oninput: move |e| repo_name.set(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Description" }
                                textarea {
                                    class: "w-full h-24 px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary resize-y",
                                    placeholder: "A brief description of your project...",
                                    value: "{repo_description}",
                                    oninput: move |e| repo_description.set(e.value()),
                                }
                            }
                        }
                        div { class: "space-y-4 pt-6 border-t border-border",
                            h2 { class: "font-semibold text-lg", "URLs" }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Clone URL" }
                                input {
                                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "https://github.com/user/repo.git",
                                    value: "{clone_url}",
                                    oninput: move |e| clone_url.set(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Web URL" }
                                input {
                                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "https://github.com/user/repo",
                                    value: "{web_url}",
                                    oninput: move |e| web_url.set(e.value()),
                                }
                            }
                        }
                        // Relays section
                        div { class: "space-y-4 pt-6 border-t border-border",
                            h2 { class: "font-semibold text-lg flex items-center gap-2",
                                "Relays"
                                span { class: "text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded-full",
                                    "{relay_list.read().len()}"
                                }
                            }
                            p { class: "text-sm text-muted-foreground",
                                "Relays that index and serve events for this repository."
                            }
                            // Current relays list
                            div { class: "space-y-2",
                                for (index, relay) in relay_list.read().iter().enumerate() {
                                    div {
                                        key: "{relay}",
                                        class: "flex items-center gap-2 p-2 bg-muted rounded-lg group",
                                        svg {
                                            class: "w-4 h-4 text-muted-foreground shrink-0",
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
                                        span { class: "flex-1 text-sm font-mono truncate", "{relay}" }
                                        button {
                                            class: "p-1 text-muted-foreground hover:text-destructive hover:bg-accent rounded opacity-0 group-hover:opacity-100 transition",
                                            onclick: move |_| {
                                                let mut list = relay_list.write();
                                                list.remove(index);
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
                            }
                            // Add new relay
                            div { class: "flex gap-2",
                                input {
                                    class: "flex-1 px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "wss://relay.example.com",
                                    value: "{new_relay_url}",
                                    oninput: move |e| new_relay_url.set(e.value()),
                                    onkeypress: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter {
                                            let url = new_relay_url.read().trim().to_string();
                                            if !url.is_empty() && url.starts_with("wss://") && !relay_list.read().contains(&url) {
                                                relay_list.write().push(url);
                                                new_relay_url.set(String::new());
                                            }
                                        }
                                    },
                                }
                                button {
                                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-90 transition disabled:opacity-50",
                                    disabled: new_relay_url.read().trim().is_empty() || !new_relay_url.read().starts_with("wss://"),
                                    onclick: move |_| {
                                        let url = new_relay_url.read().trim().to_string();
                                        if !url.is_empty() && url.starts_with("wss://") && !relay_list.read().contains(&url) {
                                            relay_list.write().push(url);
                                            new_relay_url.set(String::new());
                                        }
                                    },
                                    "Add"
                                }
                            }
                            if relay_list.read().is_empty() {
                                p { class: "text-xs text-muted-foreground italic",
                                    "No relays configured. Add relays to help others discover your repository."
                                }
                            }
                        }
                        // Maintainers section
                        div { class: "space-y-4 pt-6 border-t border-border",
                            h2 { class: "font-semibold text-lg flex items-center gap-2",
                                "Maintainers"
                                span { class: "text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded-full",
                                    "{maintainer_list.read().len()}"
                                }
                            }
                            p { class: "text-sm text-muted-foreground",
                                "Users who can manage issues and pull requests for this repository."
                            }
                            // Current maintainers list
                            div { class: "space-y-2",
                                for (index, pubkey) in maintainer_list.read().iter().enumerate() {
                                    {
                                        let display_name = {
                                            let cache = PROFILE_CACHE.read();
                                            cache.peek(pubkey).and_then(|p| {
                                                p.display_name.clone().or(p.name.clone())
                                            }).unwrap_or_else(|| truncate_pubkey(pubkey))
                                        };
                                        rsx! {
                                            div {
                                                key: "{pubkey}",
                                                class: "flex items-center gap-2 p-2 bg-muted rounded-lg group",
                                                svg {
                                                    class: "w-4 h-4 text-muted-foreground shrink-0",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                                                    circle { cx: "12", cy: "7", r: "4" }
                                                }
                                                div { class: "flex-1 min-w-0",
                                                    span { class: "text-sm font-medium truncate block", "{display_name}" }
                                                    span { class: "text-xs text-muted-foreground font-mono truncate block", "{truncate_pubkey(pubkey)}" }
                                                }
                                                button {
                                                    class: "p-1 text-muted-foreground hover:text-destructive hover:bg-accent rounded opacity-0 group-hover:opacity-100 transition",
                                                    onclick: move |_| {
                                                        let mut list = maintainer_list.write();
                                                        list.remove(index);
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
                                    }
                                }
                            }
                            // Add new maintainer
                            div { class: "flex gap-2",
                                input {
                                    class: "flex-1 px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "npub or hex pubkey",
                                    value: "{new_maintainer}",
                                    oninput: move |e| new_maintainer.set(e.value()),
                                    onkeypress: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter {
                                            let key = new_maintainer.read().trim().to_string();
                                            if !key.is_empty() {
                                                match PublicKey::parse(&key) {
                                                    Ok(pk) => {
                                                        let hex_key = pk.to_hex();
                                                        if !maintainer_list.read().contains(&hex_key) {
                                                            maintainer_list.write().push(hex_key);
                                                            new_maintainer.set(String::new());
                                                        }
                                                    }
                                                    Err(_) => {
                                                        save_error.set(Some("Invalid public key. Enter a valid npub or hex pubkey.".to_string()));
                                                    }
                                                }
                                            }
                                        }
                                    },
                                }
                                button {
                                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-90 transition disabled:opacity-50",
                                    disabled: new_maintainer.read().trim().is_empty(),
                                    onclick: move |_| {
                                        let key = new_maintainer.read().trim().to_string();
                                        if !key.is_empty() {
                                            match PublicKey::parse(&key) {
                                                Ok(pk) => {
                                                    let hex_key = pk.to_hex();
                                                    if !maintainer_list.read().contains(&hex_key) {
                                                        maintainer_list.write().push(hex_key);
                                                        new_maintainer.set(String::new());
                                                    }
                                                }
                                                Err(_) => {
                                                    save_error.set(Some("Invalid public key. Enter a valid npub or hex pubkey.".to_string()));
                                                }
                                            }
                                        }
                                    },
                                    "Add"
                                }
                            }
                            if maintainer_list.read().is_empty() {
                                p { class: "text-xs text-muted-foreground italic",
                                    "No maintainers configured. Add maintainers to allow others to manage this repository."
                                }
                            }
                        }
                        // Topics section
                        div { class: "space-y-4 pt-6 border-t border-border",
                            h2 { class: "font-semibold text-lg flex items-center gap-2",
                                "Topics"
                                span { class: "text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded-full",
                                    "{topics_list.read().len()}"
                                }
                            }
                            p { class: "text-sm text-muted-foreground",
                                "Tags that help others discover your repository."
                            }
                            div { class: "flex flex-wrap gap-2",
                                for (index, topic) in topics_list.read().iter().enumerate() {
                                    span {
                                        key: "{topic}",
                                        class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-primary/10 text-primary text-sm group",
                                        "{topic}"
                                        button {
                                            class: "ml-1 text-primary/50 hover:text-destructive transition",
                                            onclick: move |_| {
                                                let mut list = topics_list.write();
                                                list.remove(index);
                                            },
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
                                                line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                                line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "flex gap-2",
                                input {
                                    class: "flex-1 px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "e.g., rust, nostr, git",
                                    value: "{new_topic}",
                                    oninput: move |e| new_topic.set(e.value()),
                                    onkeypress: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter {
                                            let topic = new_topic.read().trim().to_lowercase();
                                            if !topic.is_empty() && !topics_list.read().contains(&topic) {
                                                topics_list.write().push(topic);
                                                new_topic.set(String::new());
                                            }
                                        }
                                    },
                                }
                                button {
                                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-90 transition disabled:opacity-50",
                                    disabled: new_topic.read().trim().is_empty(),
                                    onclick: move |_| {
                                        let topic = new_topic.read().trim().to_lowercase();
                                        if !topic.is_empty() && !topics_list.read().contains(&topic) {
                                            topics_list.write().push(topic);
                                            new_topic.set(String::new());
                                        }
                                    },
                                    "Add"
                                }
                            }
                        }
                        // Zap Split Configuration
                        div { class: "space-y-4 pt-6 border-t border-border",
                            h2 { class: "font-semibold text-lg flex items-center gap-2",
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
                                    polygon { points: "13 2 3 14 12 14 11 22 21 10 12 10 13 2" }
                                }
                                "Zap Split"
                            }
                            p { class: "text-sm text-muted-foreground",
                                "Configure how incoming zaps are distributed among contributors."
                            }
                            // Current splits display
                            div { class: "space-y-2",
                                for (idx, (pubkey, _relay, weight)) in zap_splits.read().iter().enumerate() {
                                    {
                                        let display_name = {
                                            let cache = PROFILE_CACHE.read();
                                            cache.peek(pubkey).and_then(|p| {
                                                p.display_name.clone().or(p.name.clone())
                                            }).unwrap_or_else(|| truncate_pubkey(pubkey))
                                        };
                                        let pk_short = truncate_pubkey(pubkey);
                                        let w = *weight;
                                        let can_remove = zap_splits.read().len() > 1;
                                        rsx! {
                                            div {
                                                key: "{pubkey}_{idx}",
                                                class: "flex items-center gap-3 p-3 bg-muted/50 rounded-lg group",
                                                svg {
                                                    class: "w-4 h-4 text-muted-foreground shrink-0",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                                                    circle { cx: "12", cy: "7", r: "4" }
                                                }
                                                div { class: "flex-1 min-w-0",
                                                    span { class: "text-sm font-medium truncate block", "{display_name}" }
                                                    span { class: "text-xs text-muted-foreground font-mono truncate block", "{pk_short}" }
                                                }
                                                span { class: "text-sm font-mono font-medium", "{w}%" }
                                                if can_remove {
                                                    button {
                                                        class: "p-1 text-muted-foreground hover:text-destructive hover:bg-accent rounded opacity-0 group-hover:opacity-100 transition",
                                                        onclick: move |_| {
                                                            let mut splits = zap_splits.write();
                                                            splits.remove(idx);
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
                                        }
                                    }
                                }
                            }
                            // Add new split
                            div { class: "flex gap-2",
                                input {
                                    class: "flex-1 px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "npub or hex pubkey",
                                    value: "{new_split_pubkey}",
                                    oninput: move |e| new_split_pubkey.set(e.value()),
                                    onkeypress: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter {
                                            let pk = new_split_pubkey.read().trim().to_string();
                                            let w = *new_split_weight.read();
                                            if !pk.is_empty() {
                                                match PublicKey::parse(&pk) {
                                                    Ok(parsed) => {
                                                        let hex_key = parsed.to_hex();
                                                        if zap_splits.read().iter().any(|(pk, _, _)| pk == &hex_key) {
                                                            save_error.set(Some("This pubkey is already in the zap split list.".to_string()));
                                                        } else {
                                                            let mut splits = zap_splits.write();
                                                            splits.push((hex_key, String::new(), w));
                                                            drop(splits);
                                                            new_split_pubkey.set(String::new());
                                                            new_split_weight.set(50);
                                                        }
                                                    }
                                                    Err(_) => {
                                                        save_error.set(Some("Invalid public key. Enter a valid npub or hex pubkey.".to_string()));
                                                    }
                                                }
                                            }
                                        }
                                    },
                                }
                                input {
                                    class: "w-20 px-3 py-2 bg-muted rounded-lg text-sm text-center font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "number",
                                    min: "1",
                                    max: "100",
                                    value: "{new_split_weight}",
                                    oninput: move |e| {
                                        if let Ok(w) = e.value().parse::<u32>() {
                                            new_split_weight.set(w.clamp(1, 100));
                                        }
                                    },
                                }
                                button {
                                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-90 transition disabled:opacity-50",
                                    disabled: new_split_pubkey.read().trim().is_empty(),
                                    onclick: move |_| {
                                        let pk = new_split_pubkey.read().trim().to_string();
                                        let w = *new_split_weight.read();
                                        if !pk.is_empty() {
                                            match PublicKey::parse(&pk) {
                                                Ok(parsed) => {
                                                    let hex_key = parsed.to_hex();
                                                    if zap_splits.read().iter().any(|(pk, _, _)| pk == &hex_key) {
                                                        save_error.set(Some("This pubkey is already in the zap split list.".to_string()));
                                                    } else {
                                                        let mut splits = zap_splits.write();
                                                        splits.push((hex_key, String::new(), w));
                                                        drop(splits);
                                                        new_split_pubkey.set(String::new());
                                                        new_split_weight.set(50);
                                                    }
                                                }
                                                Err(_) => {
                                                    save_error.set(Some("Invalid public key. Enter a valid npub or hex pubkey.".to_string()));
                                                }
                                            }
                                        }
                                    },
                                    "Add"
                                }
                            }
                            // Total weight indicator
                            {
                                let total: u32 = zap_splits.read().iter().map(|(_, _, w)| w).sum();
                                let color = if total == 100 { "text-green-500" } else { "text-orange-500" };
                                rsx! {
                                    p { class: "text-xs {color}",
                                        "Total: {total}% "
                                        if total != 100 {
                                            span { "(should equal 100%)" }
                                        }
                                    }
                                }
                            }
                        }
                        // Milestones section
                        div { class: "space-y-4 pt-6 border-t border-border",
                            h2 { class: "font-semibold text-lg flex items-center gap-2",
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
                                    path { d: "M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z" }
                                    line { x1: "4", y1: "22", x2: "4", y2: "15" }
                                }
                                "Milestones"
                                span { class: "text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded-full",
                                    "{milestones.read().len()}"
                                }
                            }
                            p { class: "text-sm text-muted-foreground",
                                "Track project goals and organize issues by milestone."
                            }
                            div { class: "space-y-2",
                                for (idx , ms) in milestones.read().iter().enumerate() {
                                    div {
                                        key: "{ms.id}",
                                        class: "flex items-center gap-3 p-3 bg-muted/50 rounded-lg group",
                                        svg {
                                            class: "w-4 h-4 text-muted-foreground shrink-0",
                                            xmlns: "http://www.w3.org/2000/svg",
                                            width: "24",
                                            height: "24",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            path { d: "M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z" }
                                            line { x1: "4", y1: "22", x2: "4", y2: "15" }
                                        }
                                        div { class: "flex-1 min-w-0",
                                            span { class: "text-sm font-medium truncate block", "{ms.name}" }
                                            if !ms.description.is_empty() {
                                                span { class: "text-xs text-muted-foreground truncate block", "{ms.description}" }
                                            }
                                        }
                                        button {
                                            class: "p-1 text-muted-foreground hover:text-destructive hover:bg-accent rounded opacity-0 group-hover:opacity-100 transition",
                                            onclick: move |_| {
                                                let mut list = milestones.write();
                                                list.remove(idx);
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
                            }
                            div { class: "flex gap-2",
                                input {
                                    class: "flex-1 px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "Milestone name (e.g. v1.0)",
                                    value: "{new_milestone_name}",
                                    oninput: move |e| new_milestone_name.set(e.value()),
                                    onkeypress: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter {
                                            let name = new_milestone_name.read().trim().to_string();
                                            if !name.is_empty() {
                                                if milestones.read().iter().any(|m| m.name == name) {
                                                    return;
                                                }
                                                use crate::services::git_hosting::milestones::{Milestone, generate_milestone_id};
                                                let id = generate_milestone_id(&name);
                                                let ms = Milestone {
                                                    id,
                                                    name,
                                                    description: String::new(),
                                                    due_date: None,
                                                };
                                                milestones.write().push(ms);
                                                new_milestone_name.set(String::new());
                                            }
                                        }
                                    },
                                }
                                button {
                                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-90 transition disabled:opacity-50",
                                    disabled: new_milestone_name.read().trim().is_empty(),
                                    onclick: move |_| {
                                        let name = new_milestone_name.read().trim().to_string();
                                        if !name.is_empty() {
                                            if milestones.read().iter().any(|m| m.name == name) {
                                                return;
                                            }
                                            use crate::services::git_hosting::milestones::{Milestone, generate_milestone_id};
                                            let id = generate_milestone_id(&name);
                                            let ms = Milestone {
                                                id,
                                                name,
                                                description: String::new(),
                                                due_date: None,
                                            };
                                            milestones.write().push(ms);
                                            new_milestone_name.set(String::new());
                                        }
                                    },
                                    "Add"
                                }
                            }
                        }
                        // Required Approvals section
                        div { class: "space-y-4 pt-6 border-t border-border",
                            h2 { class: "font-semibold text-lg", "Merge Policy" }
                            p { class: "text-sm text-muted-foreground",
                                "Set the number of approvals required before a pull request can be merged."
                            }
                            div { class: "flex items-center gap-3",
                                label { class: "text-sm font-medium", "Required approvals:" }
                                input {
                                    class: "w-20 px-3 py-2 bg-muted rounded-lg text-sm text-center font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "number",
                                    min: "0",
                                    max: "10",
                                    value: "{required_approvals}",
                                    oninput: move |e| {
                                        if let Ok(v) = e.value().parse::<u32>() {
                                            required_approvals.set(v.min(10));
                                        }
                                    },
                                }
                                span { class: "text-xs text-muted-foreground",
                                    if *required_approvals.read() == 0 {
                                        "No approval requirement"
                                    } else {
                                        "approvals needed to merge"
                                    }
                                }
                            }
                        }
                        // Danger Zone
                        div { class: "space-y-4 pt-6 border-t border-border",
                            h2 { class: "font-semibold text-lg text-destructive", "Danger Zone" }
                            div { class: "p-4 border border-destructive/20 rounded-lg",
                                div { class: "flex items-center justify-between",
                                    div {
                                        h3 { class: "font-medium", "Delete Repository" }
                                        p { class: "text-sm text-muted-foreground",
                                            "Permanently delete this repository announcement. This cannot be undone."
                                        }
                                    }
                                    button {
                                        class: "px-4 py-2 bg-destructive text-destructive-foreground rounded-lg text-sm hover:opacity-90 transition",
                                        onclick: move |_| show_delete_confirm.set(true),
                                        "Delete"
                                    }
                                }
                            }
                        }
                    },
                }
            }
            if *show_delete_confirm.read() {
                DeleteConfirmModal {
                    repo_name: repo_name.read().clone(),
                    on_cancel: move |_| show_delete_confirm.set(false),
                    on_confirm: move |_| {
                        let _ = nav.push(Route::CodeRepositories {});
                    },
                }
            }
        }
    }
}
#[component]
fn DeleteConfirmModal(
    repo_name: String,
    on_cancel: EventHandler<MouseEvent>,
    on_confirm: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |e| on_cancel.call(e),
            div {
                class: "bg-background border border-border rounded-lg p-6 max-w-md w-full mx-4 shadow-xl",
                onclick: move |e| e.stop_propagation(),
                div { class: "w-12 h-12 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
                    svg {
                        class: "w-6 h-6 text-destructive",
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
                        path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" }
                        path { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
                    }
                }
                h3 { class: "text-lg font-semibold text-center mb-2", "Remove Repository?" }
                p { class: "text-sm text-muted-foreground text-center mb-6",
                    "Remove \""
                    span { class: "font-medium text-foreground", "{repo_name}" }
                    "\" from your list? The repository will remain on the Nostr network."
                }
                div { class: "flex gap-3",
                    button {
                        class: "flex-1 py-2 border border-border rounded-lg font-medium hover:bg-muted transition",
                        onclick: move |e| on_cancel.call(e),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 py-2 bg-destructive text-destructive-foreground rounded-lg font-medium hover:opacity-90 transition",
                        onclick: move |e| on_confirm.call(e),
                        "Remove"
                    }
                }
            }
        }
    }
}
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-6 animate-pulse",
            div { class: "h-6 bg-muted rounded w-24" }
            div { class: "space-y-3" }
            div { class: "h-10 bg-muted rounded" }
            div { class: "h-24 bg-muted rounded" }
            div { class: "h-10 bg-muted rounded" }
        }
    }
}
#[component]
fn NotAuthenticatedState(naddr: String) -> Element {
    rsx! {
        div { class: "min-h-screen flex items-center justify-center p-4",
            div { class: "text-center max-w-md",
                div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
                    svg {
                        class: "w-10 h-10 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                        circle { cx: "12", cy: "7", r: "4" }
                    }
                }
                h2 { class: "font-semibold text-xl mb-2", "Sign In Required" }
                p { class: "text-muted-foreground mb-6",
                    "Connect with your Nostr identity to manage repository settings."
                }
                Link {
                    to: Route::CodeRepo { naddr },
                    class: "text-primary hover:underline",
                    "Back to Repository"
                }
            }
        }
    }
}
#[component]
fn NotOwnerState(naddr: String) -> Element {
    rsx! {
        div { class: "min-h-screen flex items-center justify-center p-4",
            div { class: "text-center max-w-md",
                div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
                    svg {
                        class: "w-10 h-10 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        rect {
                            x: "3",
                            y: "11",
                            width: "18",
                            height: "11",
                            rx: "2",
                            ry: "2",
                        }
                        path { d: "M7 11V7a5 5 0 0 1 10 0v4" }
                    }
                }
                h2 { class: "font-semibold text-xl mb-2", "Access Denied" }
                p { class: "text-muted-foreground mb-6",
                    "You don't have permission to manage this repository's settings."
                }
                Link {
                    to: Route::CodeRepo { naddr },
                    class: "text-primary hover:underline",
                    "Back to Repository"
                }
            }
        }
    }
}
