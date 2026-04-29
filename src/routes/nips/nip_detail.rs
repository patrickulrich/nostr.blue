use crate::components::{
    ArticleContent, ClientInitializing, ReplyComposer, ShareModal, ThreadedComment,
};
use crate::hooks::{use_author_metadata, use_relay_subscription};
use crate::routes::Route;
use crate::services::github_nips;
use crate::stores::nostr_client;
use crate::utils::{build_thread_tree, truncate_pubkey};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::time::Duration;
fn extract_title_from_content(content: &str) -> Option<String> {
    content
        .lines()
        .find(|l| l.starts_with("# ") || l.starts_with("## "))
        .map(|l| l.trim_start_matches('#').trim().to_string())
}

/// Format a spec title, stripping duplicate prefix if the extracted title already contains it.
/// e.g. prefix="NUT", num="00", title="NUT-00: Notation..." → "NUT-00: Notation..."
fn format_spec_title(prefix: &str, num: &str, extracted_title: &str) -> String {
    let prefix_pattern = format!("{}-{}: ", prefix, num);
    let clean = extracted_title
        .strip_prefix(&prefix_pattern)
        .unwrap_or(extracted_title);
    format!("{}-{}: {}", prefix, num, clean)
}

/// Load a protocol spec document and update the UI signals.
#[allow(clippy::too_many_arguments)]
fn load_spec(
    prefix: &str,
    num: Option<&str>,
    result: std::result::Result<String, String>,
    mut is_custom: Signal<bool>,
    mut nip_title: Signal<String>,
    mut nip_content: Signal<Option<String>>,
    mut loading: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    is_custom.set(false);
    nip_title.set(match num {
        Some(n) => format!("{}-{}", prefix, n),
        None => prefix.to_string(),
    });
    match result {
        Ok(content) => {
            if let Some(title) = extract_title_from_content(&content) {
                nip_title.set(match num {
                    Some(n) => format_spec_title(prefix, n, &title),
                    None => title,
                });
            }
            nip_content.set(Some(content));
        }
        Err(e) => error.set(Some(e)),
    }
    loading.set(false);
}

/// NIP detail page - displays either an official NIP from GitHub or a custom NIP from Nostr
#[component]
pub fn NipDetail(nip_id: String) -> Element {
    let mut nip_content = use_signal(|| None::<String>);
    let mut nip_title = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut is_custom = use_signal(|| false);
    let mut custom_event = use_signal(|| None::<NostrEvent>);
    let mut related_kinds = use_signal(Vec::<String>::new);
    let author_pubkey = use_memo(move || custom_event.read().as_ref().map(|e| e.pubkey.to_hex()));
    let author_metadata = use_author_metadata(author_pubkey.read().clone().unwrap_or_default());
    let mut comments = use_signal(Vec::<NostrEvent>::new);
    let mut loading_comments = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);
    let mut show_share_modal = use_signal(|| false);
    let mut is_liking = use_signal(|| false);
    let mut is_liked = use_signal(|| false);
    let mut like_count = use_signal(|| 0usize);
    let has_signer = *nostr_client::HAS_SIGNER.read();
    let nip_id_for_render = nip_id.clone();
    use_effect(move || {
        let id = nip_id.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        spawn(async move {
            loading.set(true);
            error.set(None);
            if let Some(num) = id.strip_prefix("nut-") {
                let result = github_nips::fetch_nut_content(num).await;
                load_spec(
                    "NUT",
                    Some(num),
                    result,
                    is_custom,
                    nip_title,
                    nip_content,
                    loading,
                    error,
                );
            } else if let Some(num) = id.strip_prefix("bud-") {
                let result = github_nips::fetch_bud_content(num).await;
                load_spec(
                    "BUD",
                    Some(num),
                    result,
                    is_custom,
                    nip_title,
                    nip_content,
                    loading,
                    error,
                );
            } else if let Some(num) = id.strip_prefix("nkbip-") {
                let result = github_nips::fetch_nkbip_content(num).await;
                load_spec(
                    "NKBIP",
                    Some(num),
                    result,
                    is_custom,
                    nip_title,
                    nip_content,
                    loading,
                    error,
                );
            } else if id == "market-spec" {
                let result = github_nips::fetch_market_spec().await;
                load_spec(
                    "Market Specification",
                    None,
                    result,
                    is_custom,
                    nip_title,
                    nip_content,
                    loading,
                    error,
                );
            } else if id.starts_with("naddr") {
                is_custom.set(true);
                if !client_initialized {
                    log::info!("Waiting for client initialization before loading custom NIP...");
                    return;
                }
                match nostr_client::fetch_custom_nip_by_naddr(&id).await {
                    Ok(Some(event)) => {
                        let title = event
                            .tags
                            .iter()
                            .find(|t| t.kind() == TagKind::Title)
                            .and_then(|t| t.content().map(|s| s.to_string()))
                            .unwrap_or_else(|| "Custom NIP".to_string());
                        let kinds: Vec<String> = event
                            .tags
                            .iter()
                            .filter(|t| {
                                t.kind()
                                    == TagKind::SingleLetter(SingleLetterTag::lowercase(
                                        Alphabet::K,
                                    ))
                            })
                            .filter_map(|t| t.content().map(|s| s.to_string()))
                            .collect();
                        nip_title.set(title);
                        nip_content.set(Some(event.content.clone()));
                        related_kinds.set(kinds);
                        custom_event.set(Some(event.clone()));
                        loading.set(false);
                    }
                    Ok(None) => {
                        error.set(Some("Custom NIP not found".to_string()));
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(e));
                        loading.set(false);
                    }
                }
            } else {
                // Official NIP
                is_custom.set(false);
                nip_title.set(format!("NIP-{}", id));
                match github_nips::fetch_nip_content(&id).await {
                    Ok(content) => {
                        if let Some(title) = extract_title_from_content(&content) {
                            nip_title.set(format_spec_title("NIP", &id, &title));
                        }
                        nip_content.set(Some(content));
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(e));
                        loading.set(false);
                    }
                }
            }
        });
    });
    use_effect(move || {
        let event = custom_event.read();
        if let Some(e) = event.as_ref() {
            let event_id = e.id;
            spawn(async move {
                loading_comments.set(true);
                let filter = Filter::new().kind(Kind::Comment).event(event_id).limit(500);
                match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                    Ok(mut comment_events) => {
                        comment_events.sort_by_key(|a| a.created_at);
                        log::info!("Loaded {} comments for custom NIP", comment_events.len());
                        comments.set(comment_events);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch comments: {}", e);
                    }
                }
                loading_comments.set(false);
            });
        }
    });

    {
        let comment_filter = custom_event.read().as_ref().map(|event| {
            Filter::new()
                .kind(Kind::Comment)
                .event(event.id)
                .since(Timestamp::now())
                .limit(0)
        });
        use_relay_subscription(comment_filter, move |event: &nostr::Event| {
            let already_exists = comments.read().iter().any(|e| e.id == event.id);
            if !already_exists {
                log::info!(
                    "New comment received via streaming: {}",
                    event.id.to_hex()
                );
                comments.write().push(event.clone());
            }
        });
    }

    use_effect(move || {
        let event = custom_event.read();
        if let Some(e) = event.as_ref() {
            let event_id = e.id;
            let current_user_pubkey = crate::stores::signer::SIGNER_INFO
                .read()
                .as_ref()
                .map(|info| info.public_key.clone());
            spawn(async move {
                let filter = Filter::new()
                    .kind(Kind::Reaction)
                    .event(event_id)
                    .limit(1000);
                match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                    Ok(reactions) => {
                        let positive_count = reactions
                            .iter()
                            .filter(|r| {
                                r.content == "+"
                                    || r.content == "❤️"
                                    || r.content == "👍"
                                    || r.content.is_empty()
                            })
                            .count();
                        like_count.set(positive_count);
                        if let Some(user_pk) = current_user_pubkey {
                            let user_has_liked = reactions.iter().any(|r| {
                                r.pubkey.to_hex() == user_pk
                                    && (r.content == "+"
                                        || r.content == "❤️"
                                        || r.content == "👍"
                                        || r.content.is_empty())
                            });
                            is_liked.set(user_has_liked);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to fetch reactions: {}", e);
                    }
                }
            });
        }
    });
    let handle_like = move |_| {
        if !has_signer || *is_liking.read() {
            return;
        }
        let event = custom_event.read();
        if let Some(e) = event.as_ref() {
            let event_id = e.id;
            let event_pubkey = e.pubkey;
            is_liking.set(true);
            spawn(async move {
                match nostr_client::publish_reaction(
                    event_id.to_hex(),
                    event_pubkey.to_hex(),
                    "+".to_string(),
                    None,
                )
                .await
                {
                    Ok(_) => {
                        is_liked.set(true);
                        let new_count = *like_count.peek() + 1;
                        like_count.set(new_count);
                    }
                    Err(e) => {
                        log::error!("Failed to like: {}", e);
                    }
                }
                is_liking.set(false);
            });
        }
    };
    let comment_tree = use_memo(move || {
        let event = custom_event.read();
        if let Some(e) = event.as_ref() {
            build_thread_tree(comments.read().clone(), &e.id)
        } else {
            Vec::new()
        }
    });
    let author_display = use_memo(move || {
        let event = custom_event.read();
        if let Some(e) = event.as_ref() {
            let pubkey = e.pubkey.to_hex();
            author_metadata
                .read()
                .as_ref()
                .and_then(|m| m.display_name.clone().or(m.name.clone()))
                .unwrap_or_else(|| truncate_pubkey(&pubkey))
        } else {
            String::new()
        }
    });
    let author_picture = use_memo(move || {
        author_metadata
            .read()
            .as_ref()
            .and_then(|m| m.picture.clone())
            .map(|u| u.to_string())
    });
    let timestamp = use_memo(move || {
        let event = custom_event.read();
        if let Some(e) = event.as_ref() {
            let ts = e.created_at.as_secs();
            chrono::DateTime::from_timestamp(ts as i64, 0)
                .map(|dt| dt.format("%B %d, %Y").to_string())
                .unwrap_or_else(|| "Unknown date".to_string())
        } else {
            String::new()
        }
    });
    if *loading.read() {
        return rsx! {
            div { class: "min-h-screen",
                div { class: "p-4", ClientInitializing {} }
            }
        };
    }
    if let Some(err) = error.read().as_ref() {
        return rsx! {
            div { class: "min-h-screen p-4",
                div { class: "max-w-4xl mx-auto",
                    div { class: "p-6 bg-destructive/10 border border-destructive rounded-lg text-center",
                        h2 { class: "text-xl font-semibold mb-2 text-destructive",
                            "Error Loading Specification"
                        }
                        p { class: "text-muted-foreground mb-4", "{err}" }
                        Link {
                            to: Route::NipsHome {},
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "← Back to Docs"
                        }
                    }
                }
            }
        };
    }
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: Route::NipsHome {},
                        class: "p-2 rounded-lg hover:bg-accent transition",
                        "← Back"
                    }
                    h1 { class: "text-lg font-bold truncate flex-1", "{nip_title}" }
                    button {
                        class: "p-2 rounded-lg hover:bg-accent transition",
                        onclick: move |_| show_share_modal.set(true),
                        crate::components::icons::ShareIcon { class: "w-5 h-5" }
                    }
                }
            }
            div { class: "max-w-4xl mx-auto p-4",
                if *is_custom.read() {
                    div { class: "flex items-center gap-4 mb-6 pb-6 border-b border-border",
                        if let Some(pic) = author_picture().as_ref() {
                            img {
                                src: "{pic}",
                                alt: "{author_display}",
                                class: "w-12 h-12 rounded-full object-cover",
                            }
                        } else {
                            div { class: "w-12 h-12 rounded-full bg-primary/20 flex items-center justify-center text-xl font-medium",
                                "{author_display().chars().next().unwrap_or('?').to_uppercase()}"
                            }
                        }
                        div { class: "flex-1",
                            p { class: "font-medium", "{author_display}" }
                            p { class: "text-sm text-muted-foreground", "{timestamp}" }
                        }
                        span { class: "px-3 py-1 rounded-full bg-primary/10 text-primary text-sm font-medium",
                            "Custom NIP"
                        }
                    }
                    if !related_kinds.read().is_empty() {
                        div { class: "flex flex-wrap gap-2 mb-6",
                            span { class: "text-sm text-muted-foreground mr-2", "Defines kinds:" }
                            for kind in related_kinds.read().iter() {
                                span { class: "px-2 py-1 rounded bg-muted text-muted-foreground font-mono text-sm",
                                    "{kind}"
                                }
                            }
                        }
                    }
                }
                if let Some(content) = nip_content.read().as_ref() {
                    div { class: "mb-8",
                        ArticleContent { content: content.clone() }
                    }
                }
                if *is_custom.read() {
                    div { class: "flex items-center gap-4 py-4 border-t border-b border-border mb-8",
                        button {
                            class: format!(
                                "flex items-center gap-2 px-4 py-2 rounded-lg transition {}",
                                if *is_liked.read() { "text-red-500 bg-red-500/10" } else { "hover:bg-accent" },
                            ),
                            disabled: !has_signer || *is_liking.read(),
                            onclick: handle_like,
                            crate::components::icons::HeartIcon { class: "w-5 h-5", filled: *is_liked.read() }
                            span { "{like_count}" }
                        }
                        button {
                            class: "flex items-center gap-2 px-4 py-2 rounded-lg hover:bg-accent transition",
                            onclick: move |_| {
                                let current = *show_comment_composer.read();
                                show_comment_composer.set(!current);
                            },
                            crate::components::icons::MessageCircleIcon { class: "w-5 h-5" }
                            span { "{comments.read().len()}" }
                        }
                        button {
                            class: "flex items-center gap-2 px-4 py-2 rounded-lg hover:bg-accent transition",
                            onclick: move |_| show_share_modal.set(true),
                            crate::components::icons::ShareIcon { class: "w-5 h-5" }
                            span { "Share" }
                        }
                    }
                    if *show_comment_composer.read() && has_signer {
                        if let Some(event) = custom_event.read().clone() {
                            div { class: "mb-6",
                                ReplyComposer {
                                    target: event,
                                    root_event: None,
                                    on_close: move |_| show_comment_composer.set(false),
                                    on_success: move |_| show_comment_composer.set(false),
                                }
                            }
                        }
                    }
                    if !comments.read().is_empty() || *loading_comments.read() {
                        div { class: "mt-8",
                            h3 { class: "text-lg font-semibold mb-4",
                                "Comments ({comments.read().len()})"
                            }
                            if *loading_comments.read() {
                                div { class: "text-center py-4 text-muted-foreground",
                                    "Loading comments..."
                                }
                            } else {
                                div { class: "space-y-4",
                                    for node in comment_tree().iter() {
                                        ThreadedComment {
                                            key: "{node.event.id}",
                                            node: node.clone(),
                                            depth: 0,
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if !*is_custom.read() {
                    {
                        let docs_path = if nip_id_for_render.starts_with("nut-") {
                            "/docs/nuts/"
                        } else if nip_id_for_render.starts_with("bud-") {
                            "/docs/blossom/"
                        } else if nip_id_for_render.starts_with("nkbip-") {
                            "/docs/NKBIPs/"
                        } else if nip_id_for_render == "market-spec" {
                            "/docs/market-spec/"
                        } else {
                            "/docs/nips/"
                        };
                        rsx! {
                            div { class: "mt-8 pt-8 border-t border-border text-center text-sm text-muted-foreground",
                                p {
                                    "This specification is from the "
                                    a {
                                        href: docs_path,
                                        class: "text-primary hover:underline",
                                        "nostr.blue documentation"
                                    }
                                    "."
                                }
                            }
                        }
                    }
                }
            }
            if *show_share_modal.read() {
                if let Some(event) = custom_event.read().clone() {
                    ShareModal {
                        event,
                        web_url: Some(format!("https://nostr.blue/nips/{}", nip_id_for_render)),
                        on_close: move |_| show_share_modal.set(false),
                    }
                }
            }
        }
    }
}
