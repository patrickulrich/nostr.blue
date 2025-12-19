use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, Filter, Kind, TagKind, SingleLetterTag, Alphabet};
use crate::routes::Route;
use crate::stores::nostr_client;
use crate::services::github_nips;
use crate::hooks::use_author_metadata;
use crate::components::{ClientInitializing, ThreadedComment, CommentComposer, ShareModal, ArticleContent};
use crate::utils::{build_thread_tree, merge_pending_into_tree};
use crate::stores::pending_comments::get_pending_comments;
use std::time::Duration;

/// NIP detail page - displays either an official NIP from GitHub or a custom NIP from Nostr
#[component]
pub fn NipDetail(nip_id: String) -> Element {
    // State
    let mut nip_content = use_signal(|| None::<String>);
    let mut nip_title = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Custom NIP specific state
    let mut is_custom = use_signal(|| false);
    let mut custom_event = use_signal(|| None::<NostrEvent>);
    let mut related_kinds = use_signal(Vec::<String>::new);

    // Fetch author metadata reactively using the hook
    let author_pubkey = use_memo(move || {
        custom_event.read().as_ref().map(|e| e.pubkey.to_hex())
    });
    let author_metadata = use_author_metadata(
        author_pubkey.read().clone().unwrap_or_default()
    );

    // Comments state
    let mut comments = use_signal(Vec::<NostrEvent>::new);
    let mut loading_comments = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);

    // Share modal state
    let mut show_share_modal = use_signal(|| false);

    // Like button state
    let mut is_liking = use_signal(|| false);
    let mut is_liked = use_signal(|| false);
    let mut like_count = use_signal(|| 0usize);

    let has_signer = *nostr_client::HAS_SIGNER.read();

    // Determine if this is an official or custom NIP and fetch content
    use_effect(move || {
        let id = nip_id.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        spawn(async move {
            loading.set(true);
            error.set(None);

            if id.starts_with("naddr") {
                // Custom NIP from Nostr
                is_custom.set(true);

                if !client_initialized {
                    log::info!("Waiting for client initialization before loading custom NIP...");
                    return;
                }

                match nostr_client::fetch_custom_nip_by_naddr(&id).await {
                    Ok(Some(event)) => {
                        // Extract title from tags
                        let title = event.tags.iter()
                            .find(|t| t.kind() == TagKind::Title)
                            .and_then(|t| t.content().map(|s| s.to_string()))
                            .unwrap_or_else(|| "Custom NIP".to_string());

                        // Extract related kinds from k tags
                        let kinds: Vec<String> = event.tags.iter()
                            .filter(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::K)))
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
                // Official NIP from GitHub
                is_custom.set(false);
                nip_title.set(format!("NIP-{}", id));

                match github_nips::fetch_nip_content(&id).await {
                    Ok(content) => {
                        // Try to extract title from first heading
                        if let Some(first_line) = content.lines().find(|l| l.starts_with("# ") || l.starts_with("## ")) {
                            let title = first_line.trim_start_matches('#').trim();
                            nip_title.set(format!("NIP-{}: {}", id, title));
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

    // Fetch comments for custom NIPs
    use_effect(move || {
        let event = custom_event.read();
        if let Some(e) = event.as_ref() {
            let event_id = e.id;

            spawn(async move {
                loading_comments.set(true);

                // Fetch NIP-22 comments
                let filter = Filter::new()
                    .kind(Kind::Comment)
                    .event(event_id)
                    .limit(500);

                match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                    Ok(mut comment_events) => {
                        comment_events.sort_by(|a, b| a.created_at.cmp(&b.created_at));
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

    // Fetch reactions for custom NIPs
    use_effect(move || {
        let event = custom_event.read();
        if let Some(e) = event.as_ref() {
            let event_id = e.id;
            let current_user_pubkey = crate::stores::signer::SIGNER_INFO.read()
                .as_ref()
                .map(|info| info.public_key.clone());

            spawn(async move {
                let filter = Filter::new()
                    .kind(Kind::Reaction)
                    .event(event_id)
                    .limit(1000);

                match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                    Ok(reactions) => {
                        let positive_count = reactions.iter()
                            .filter(|r| r.content == "+" || r.content == "❤️" || r.content == "👍" || r.content.is_empty())
                            .count();

                        like_count.set(positive_count);

                        // Check if current user has liked
                        if let Some(user_pk) = current_user_pubkey {
                            let user_has_liked = reactions.iter()
                                .any(|r| r.pubkey.to_hex() == user_pk &&
                                    (r.content == "+" || r.content == "❤️" || r.content == "👍" || r.content.is_empty()));
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

    // Handle like action
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
                ).await {
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

    // Build comment tree
    let comment_tree = use_memo(move || {
        let event = custom_event.read();
        if let Some(e) = event.as_ref() {
            let tree = build_thread_tree(comments.read().clone(), &e.id);
            let pending = get_pending_comments(&e.id);
            merge_pending_into_tree(tree, pending, &e.id)
        } else {
            Vec::new()
        }
    });

    // Get author info
    let author_display = use_memo(move || {
        let event = custom_event.read();
        if let Some(e) = event.as_ref() {
            let pubkey = e.pubkey.to_hex();
            author_metadata.read().as_ref()
                .and_then(|m| m.display_name.clone().or(m.name.clone()))
                .unwrap_or_else(|| {
                    if pubkey.len() > 16 {
                        format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..])
                    } else {
                        pubkey
                    }
                })
        } else {
            String::new()
        }
    });

    let author_picture = use_memo(move || {
        author_metadata.read().as_ref()
            .and_then(|m| m.picture.clone())
            .map(|u| u.to_string())
    });

    // Format timestamp
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

    // Loading state
    if *loading.read() {
        return rsx! {
            div {
                class: "min-h-screen",
                div {
                    class: "p-4",
                    ClientInitializing {}
                }
            }
        };
    }

    // Error state
    if let Some(err) = error.read().as_ref() {
        return rsx! {
            div {
                class: "min-h-screen p-4",
                div {
                    class: "max-w-4xl mx-auto",
                    div {
                        class: "p-6 bg-destructive/10 border border-destructive rounded-lg text-center",
                        h2 {
                            class: "text-xl font-semibold mb-2 text-destructive",
                            "Error Loading NIP"
                        }
                        p {
                            class: "text-muted-foreground mb-4",
                            "{err}"
                        }
                        Link {
                            to: Route::NipsHome {},
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "← Back to NIPs"
                        }
                    }
                }
            }
        };
    }

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: Route::NipsHome {},
                        class: "p-2 rounded-lg hover:bg-accent transition",
                        "← Back"
                    }
                    h1 {
                        class: "text-lg font-bold truncate flex-1",
                        "{nip_title}"
                    }

                    // Share button
                    button {
                        class: "p-2 rounded-lg hover:bg-accent transition",
                        onclick: move |_| show_share_modal.set(true),
                        crate::components::icons::ShareIcon { class: "w-5 h-5" }
                    }
                }
            }

            // Content
            div {
                class: "max-w-4xl mx-auto p-4",

                // Custom NIP author header
                if *is_custom.read() {
                    div {
                        class: "flex items-center gap-4 mb-6 pb-6 border-b border-border",

                        // Author avatar
                        if let Some(pic) = author_picture().as_ref() {
                            img {
                                src: "{pic}",
                                alt: "{author_display}",
                                class: "w-12 h-12 rounded-full object-cover",
                            }
                        } else {
                            div {
                                class: "w-12 h-12 rounded-full bg-primary/20 flex items-center justify-center text-xl font-medium",
                                "{author_display().chars().next().unwrap_or('?').to_uppercase()}"
                            }
                        }

                        div {
                            class: "flex-1",
                            p {
                                class: "font-medium",
                                "{author_display}"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "{timestamp}"
                            }
                        }

                        // Custom badge
                        span {
                            class: "px-3 py-1 rounded-full bg-blue-500/10 text-blue-500 text-sm font-medium",
                            "Custom NIP"
                        }
                    }

                    // Related kinds
                    if !related_kinds.read().is_empty() {
                        div {
                            class: "flex flex-wrap gap-2 mb-6",
                            span {
                                class: "text-sm text-muted-foreground mr-2",
                                "Defines kinds:"
                            }
                            for kind in related_kinds.read().iter() {
                                span {
                                    class: "px-2 py-1 rounded bg-muted text-muted-foreground font-mono text-sm",
                                    "{kind}"
                                }
                            }
                        }
                    }
                }

                // NIP content (markdown)
                if let Some(content) = nip_content.read().as_ref() {
                    div {
                        class: "mb-8",
                        ArticleContent {
                            content: content.clone(),
                        }
                    }
                }

                // Actions for custom NIPs
                if *is_custom.read() {
                    div {
                        class: "flex items-center gap-4 py-4 border-t border-b border-border mb-8",

                        // Like button
                        button {
                            class: format!(
                                "flex items-center gap-2 px-4 py-2 rounded-lg transition {}",
                                if *is_liked.read() { "text-red-500 bg-red-500/10" } else { "hover:bg-accent" }
                            ),
                            disabled: !has_signer || *is_liking.read(),
                            onclick: handle_like,
                            crate::components::icons::HeartIcon {
                                class: "w-5 h-5",
                                filled: *is_liked.read(),
                            }
                            span { "{like_count}" }
                        }

                        // Comment button
                        button {
                            class: "flex items-center gap-2 px-4 py-2 rounded-lg hover:bg-accent transition",
                            onclick: move |_| {
                                let current = *show_comment_composer.read();
                                show_comment_composer.set(!current);
                            },
                            crate::components::icons::MessageCircleIcon { class: "w-5 h-5" }
                            span { "{comments.read().len()}" }
                        }

                        // Share button
                        button {
                            class: "flex items-center gap-2 px-4 py-2 rounded-lg hover:bg-accent transition",
                            onclick: move |_| show_share_modal.set(true),
                            crate::components::icons::ShareIcon { class: "w-5 h-5" }
                            span { "Share" }
                        }
                    }

                    // Comment composer
                    if *show_comment_composer.read() && has_signer {
                        if let Some(event) = custom_event.read().clone() {
                            div {
                                class: "mb-6",
                                CommentComposer {
                                    comment_on: event,
                                    parent_comment: None,
                                    on_close: move |_| show_comment_composer.set(false),
                                    on_success: move |_| show_comment_composer.set(false),
                                }
                            }
                        }
                    }

                    // Comments section
                    if !comments.read().is_empty() || *loading_comments.read() {
                        div {
                            class: "mt-8",
                            h3 {
                                class: "text-lg font-semibold mb-4",
                                "Comments ({comments.read().len()})"
                            }

                            if *loading_comments.read() {
                                div {
                                    class: "text-center py-4 text-muted-foreground",
                                    "Loading comments..."
                                }
                            } else {
                                div {
                                    class: "space-y-4",
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

                // Footer for official NIPs
                if !*is_custom.read() {
                    div {
                        class: "mt-8 pt-8 border-t border-border text-center text-sm text-muted-foreground",
                        p {
                            "This NIP is from the official "
                            a {
                                href: "https://github.com/nostr-protocol/nips",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "text-primary hover:underline",
                                "nostr-protocol/nips"
                            }
                            " repository."
                        }
                    }
                }
            }

            // Share modal (only for custom NIPs that have an event)
            if *show_share_modal.read() {
                if let Some(event) = custom_event.read().clone() {
                    ShareModal {
                        event: event,
                        on_close: move |_| show_share_modal.set(false),
                    }
                }
            }
        }
    }
}
