use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, Filter, Kind};
use crate::routes::Route;
use crate::stores::bookmarks;
use crate::components::{ArticleContent, icons::*, ThreadedComment, CommentComposer, ClientInitializing, ShareModal};
use crate::utils::article_meta::{
    get_title, get_summary, get_image, get_published_at,
    get_hashtags, calculate_read_time
};
use crate::utils::build_thread_tree;
use std::time::Duration;

#[component]
pub fn ArticleDetail(naddr: String) -> Element {
    // State for the article
    let mut article = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut comments = use_signal(|| Vec::<NostrEvent>::new());
    let mut loading_comments = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);
    let mut show_share_modal = use_signal(|| false);

    // Like button state
    let mut is_liking = use_signal(|| false);
    let mut is_liked = use_signal(|| false);
    let mut like_count = use_signal(|| 0usize);

    let has_signer = *crate::stores::nostr_client::HAS_SIGNER.read();

    // Decode naddr and fetch article - wait for client to be initialized
    use_effect(move || {
        let naddr_str = naddr.clone();
        let client_initialized = *crate::stores::nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            log::info!("Waiting for client initialization before loading article...");
            return;
        }

        spawn(async move {
            loading.set(true);
            error.set(None);

            // Decode the naddr
            match decode_naddr(&naddr_str) {
                Ok((pubkey, identifier)) => {
                    // Clear only this author's profile from cache to prevent stale metadata
                    crate::stores::profiles::PROFILE_CACHE.write().pop(&pubkey);

                    // Fetch the article
                    match crate::stores::nostr_client::fetch_article_by_coordinate(
                        pubkey.clone(),
                        identifier
                    ).await {
                        Ok(Some(event)) => {
                            article.set(Some(event.clone()));
                            loading.set(false);

                            // Prefetch author metadata using optimized utility
                            use crate::utils::profile_prefetch;
                            spawn(async move {
                                profile_prefetch::prefetch_event_authors(&[event]).await;

                                // Update author_metadata signal after prefetch
                                if let Some(profile) = crate::stores::profiles::get_cached_profile(&pubkey) {
                                    let mut metadata = nostr_sdk::Metadata::new();
                                    if let Some(name) = profile.name {
                                        metadata = metadata.name(name);
                                    }
                                    if let Some(display_name) = profile.display_name {
                                        metadata = metadata.display_name(display_name);
                                    }
                                    if let Some(about) = profile.about {
                                        metadata = metadata.about(about);
                                    }
                                    if let Some(picture) = profile.picture {
                                        if let Ok(url) = nostr_sdk::Url::parse(&picture) {
                                            metadata = metadata.picture(url);
                                        }
                                    }
                                    author_metadata.set(Some(metadata));
                                }
                            });
                        }
                        Ok(None) => {
                            error.set(Some("Article not found".to_string()));
                            loading.set(false);
                        }
                        Err(e) => {
                            error.set(Some(e));
                            loading.set(false);
                        }
                    }
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Fetch NIP-22 comments for the article
    use_effect(move || {
        let article_data = article.read();

        if let Some(event) = article_data.as_ref() {
            let event_id = event.id;

            spawn(async move {
                loading_comments.set(true);

                // Fetch Kind 1111 (NIP-22 Comment) events that reference this article
                let filter = Filter::new()
                    .kind(Kind::Comment)
                    .event(event_id)
                    .limit(500);

                match crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                    Ok(mut comment_events) => {
                        comment_events.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                        log::info!("Loaded {} NIP-22 comments", comment_events.len());
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

    // Fetch reactions (likes) for the article
    use_effect(move || {
        let article_data = article.read();

        if let Some(event) = article_data.as_ref() {
            let event_id = event.id;

            // Get current user pubkey to detect if they've liked
            let current_user_pubkey = crate::stores::signer::SIGNER_INFO.read()
                .as_ref()
                .map(|info| info.public_key.clone());

            spawn(async move {
                // Fetch Kind 7 (Reaction) events for this article
                let filter = Filter::new()
                    .kind(Kind::Reaction)
                    .event(event_id)
                    .limit(500);

                match crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                    Ok(reaction_events) => {
                        // Count likes (reactions with "+" or emoji content)
                        let mut likes = 0;
                        let mut user_has_liked = false;

                        for reaction in &reaction_events {
                            // Count positive reactions (+ or emoji)
                            if reaction.content != "-" {
                                likes += 1;
                            }

                            // Check if current user has liked
                            if let Some(ref user_pk) = current_user_pubkey {
                                if reaction.pubkey.to_hex() == *user_pk && reaction.content != "-" {
                                    user_has_liked = true;
                                }
                            }
                        }

                        like_count.set(likes);
                        is_liked.set(user_has_liked);
                        log::info!("Loaded {} reactions for article, user has liked: {}", likes, user_has_liked);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch reactions: {}", e);
                    }
                }
            });
        }
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Back button header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3",
                    Link {
                        to: Route::Articles {},
                        class: "inline-flex items-center gap-2 text-sm hover:text-primary transition",
                        span { "←" }
                        span { "Back to Articles" }
                    }
                }
            }

            // Content
            div {
                class: "max-w-4xl mx-auto px-4 py-8",

                // Loading state
                if !*crate::stores::nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && article.read().is_none()) {
                    // Show client initializing animation during:
                    // 1. Client initialization
                    // 2. Initial article load (loading + no article, regardless of error state)
                    ClientInitializing {}
                }

                // Error state
                if let Some(err) = error.read().as_ref() {
                    div {
                        class: "text-center py-12",
                        div {
                            class: "text-6xl mb-4",
                            "❌"
                        }
                        h3 {
                            class: "text-xl font-semibold mb-2",
                            "Error Loading Article"
                        }
                        p {
                            class: "text-muted-foreground mb-4",
                            "{err}"
                        }
                        Link {
                            to: Route::Articles {},
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 inline-block",
                            "Back to Articles"
                        }
                    }
                }

                // Article content
                if let Some(event) = article.read().as_ref().cloned() {
                    {
                        let title = get_title(&event);
                        let summary = get_summary(&event);
                        let image_url = get_image(&event);
                        let published_at = get_published_at(&event);
                        let hashtags = get_hashtags(&event);
                        let read_time = calculate_read_time(&event.content);
                        let author_pubkey = event.pubkey.to_hex();
                        let event_id = event.id.to_string();

                        let display_name = author_metadata.read().as_ref()
                            .and_then(|m| m.display_name.clone().or(m.name.clone()))
                            .unwrap_or_else(|| format!("{}...{}", &author_pubkey[..8], &author_pubkey[author_pubkey.len()-8..]));

                        let profile_picture = author_metadata.read().as_ref()
                            .and_then(|m| m.picture.clone());

                        let avatar_letter = display_name.chars().next()
                            .unwrap_or('?')
                            .to_uppercase()
                            .to_string();

                        let timestamp = format_timestamp(published_at);

                        rsx! {
                            article {
                                class: "space-y-6",

                                // Title
                                h1 {
                                    class: "text-4xl md:text-5xl font-bold leading-tight",
                                    "{title}"
                                }

                                // Summary
                                if let Some(sum) = summary {
                                    p {
                                        class: "text-xl text-muted-foreground leading-relaxed",
                                        "{sum}"
                                    }
                                }

                                // Author info and metadata
                                div {
                                    class: "flex items-center justify-between py-4 border-y border-border",

                                    // Author
                                    Link {
                                        to: Route::Profile { pubkey: author_pubkey.clone() },
                                        class: "flex items-center gap-3 hover:opacity-80 transition",

                                        // Avatar
                                        div {
                                            class: "w-12 h-12 rounded-full overflow-hidden bg-muted flex items-center justify-center",
                                            if let Some(pic_url) = profile_picture {
                                                img {
                                                    src: "{pic_url}",
                                                    alt: "{display_name}",
                                                    class: "w-full h-full object-cover",
                                                }
                                            } else {
                                                span {
                                                    class: "text-lg font-semibold text-muted-foreground",
                                                    "{avatar_letter}"
                                                }
                                            }
                                        }

                                        // Name and date
                                        div {
                                            div {
                                                class: "font-semibold",
                                                "{display_name}"
                                            }
                                            div {
                                                class: "text-sm text-muted-foreground",
                                                "{timestamp} · {read_time} min read"
                                            }
                                        }
                                    }
                                }

                                // Hashtags
                                if !hashtags.is_empty() {
                                    div {
                                        class: "flex flex-wrap gap-2",
                                        for tag in hashtags.clone() {
                                            Link {
                                                to: Route::Hashtag { tag: tag.clone() },
                                                class: "px-3 py-1 text-sm rounded-full bg-primary/10 text-primary font-medium hover:bg-primary/20 transition",
                                                "#{tag}"
                                            }
                                        }
                                    }
                                }

                                // Cover image
                                if let Some(img_url) = image_url {
                                    div {
                                        class: "rounded-lg overflow-hidden",
                                        img {
                                            src: "{img_url}",
                                            alt: "{title}",
                                            class: "w-full h-auto",
                                        }
                                    }
                                }

                                // Article content (markdown)
                                ArticleContent {
                                    content: event.content.clone(),
                                }

                                // Footer with action buttons
                                div {
                                    class: "pt-8 border-t border-border",
                                    div {
                                        class: "flex items-center justify-center gap-4",

                                        // Like button with optimistic update
                                        {
                                            let event_id_like = event_id.clone();
                                            let author_pubkey_like = author_pubkey.clone();
                                            rsx! {
                                                button {
                                                    class: if *is_liked.read() {
                                                        "flex items-center gap-2 px-4 py-2 rounded-lg bg-red-500/10 text-red-500 transition"
                                                    } else {
                                                        "flex items-center gap-2 px-4 py-2 rounded-lg hover:bg-accent transition"
                                                    },
                                                    disabled: !has_signer || *is_liking.read() || *is_liked.read(),
                                                    onclick: move |_| {
                                                        if !has_signer || *is_liking.read() || *is_liked.read() {
                                                            return;
                                                        }

                                                        let event_id_clone = event_id_like.clone();
                                                        let author_pubkey_clone = author_pubkey_like.clone();

                                                        is_liking.set(true);

                                                        spawn(async move {
                                                            match crate::stores::nostr_client::publish_reaction(
                                                                event_id_clone,
                                                                author_pubkey_clone,
                                                                "+".to_string(),
                                                                None
                                                            ).await {
                                                                Ok(reaction_id) => {
                                                                    log::info!("Liked article, reaction ID: {}", reaction_id);
                                                                    is_liked.set(true);
                                                                    // Optimistic update - increment count
                                                                    let current_count = *like_count.read();
                                                                    like_count.set(current_count.saturating_add(1));
                                                                }
                                                                Err(e) => {
                                                                    log::error!("Failed to like article: {}", e);
                                                                }
                                                            }
                                                            is_liking.set(false);
                                                        });
                                                    },
                                                    HeartIcon {
                                                        class: "w-5 h-5",
                                                        filled: *is_liked.read()
                                                    }
                                                    span {
                                                        if *is_liking.read() {
                                                            "..."
                                                        } else if *like_count.read() > 0 {
                                                            "{like_count.read()}"
                                                        } else {
                                                            "Like"
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // Bookmark button
                                        button {
                                            class: "flex items-center gap-2 px-4 py-2 rounded-lg hover:bg-accent transition",
                                            onclick: move |_| {
                                                let event_id_clone = event_id.clone();
                                                spawn(async move {
                                                    if bookmarks::is_bookmarked(&event_id_clone) {
                                                        let _ = bookmarks::unbookmark_event(event_id_clone).await;
                                                    } else {
                                                        let _ = bookmarks::bookmark_event(event_id_clone).await;
                                                    }
                                                });
                                            },
                                            BookmarkIcon { class: "w-5 h-5" }
                                            span {
                                                if bookmarks::is_bookmarked(&event_id) {
                                                    "Bookmarked"
                                                } else {
                                                    "Bookmark"
                                                }
                                            }
                                        }

                                        // Share button
                                        button {
                                            class: "flex items-center gap-2 px-4 py-2 rounded-lg hover:bg-accent transition",
                                            onclick: move |_| show_share_modal.set(true),
                                            ShareIcon { class: "w-5 h-5" }
                                            span { "Share" }
                                        }
                                    }
                                }

                                // Comments section
                                div {
                                    class: "pt-8 mt-8 border-t border-border",
                                    div {
                                        class: "flex items-center justify-between mb-6",
                                        h3 {
                                            class: "text-2xl font-bold",
                                            "Comments"
                                        }
                                        button {
                                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center gap-2",
                                            onclick: move |_| show_comment_composer.set(true),
                                            MessageCircleIcon { class: "w-4 h-4".to_string(), filled: false }
                                            span { "Add Comment" }
                                        }
                                    }

                                    if *loading_comments.read() {
                                        div {
                                            class: "flex items-center justify-center py-10",
                                            div {
                                                class: "text-center",
                                                div {
                                                    class: "animate-spin text-4xl mb-2",
                                                    "⚡"
                                                }
                                                p {
                                                    class: "text-muted-foreground",
                                                    "Loading comments..."
                                                }
                                            }
                                        }
                                    } else {
                                        // Only build thread tree after loading completes to avoid caching empty results
                                        {
                                            let comment_vec = comments.read().clone();
                                            let thread_tree = build_thread_tree(comment_vec, &event.id);

                                            rsx! {
                                                if thread_tree.is_empty() {
                                                    div {
                                                        class: "flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground",
                                                        p { "No comments yet" }
                                                        p {
                                                            class: "text-sm",
                                                            "Be the first to comment!"
                                                        }
                                                    }
                                                } else {
                                                    div {
                                                        class: "divide-y divide-border",
                                                        for node in thread_tree {
                                                            ThreadedComment {
                                                                node: node.clone(),
                                                                depth: 0
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Comment composer modal
                                if *show_comment_composer.read() {
                                    CommentComposer {
                                        comment_on: event.clone(),
                                        parent_comment: None,
                                        on_close: move |_| show_comment_composer.set(false),
                                        on_success: move |_| {
                                            show_comment_composer.set(false);
                                            // Refresh comments
                                            let event_id = event.id;
                                            spawn(async move {
                                                loading_comments.set(true);
                                                let filter = Filter::new()
                                                    .kind(Kind::Comment)
                                                    .event(event_id)
                                                    .limit(500);

                                                if let Ok(mut comment_events) = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                                                    comment_events.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                                                    comments.set(comment_events);
                                                }
                                                loading_comments.set(false);
                                            });
                                        }
                                    }
                                }

                                // Share modal
                                if *show_share_modal.read() {
                                    ShareModal {
                                        event: event.clone(),
                                        on_close: move |_| show_share_modal.set(false)
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

/// Decode naddr to extract pubkey and identifier
fn decode_naddr(naddr: &str) -> Result<(String, String), String> {
    use nostr::nips::nip19::{Nip19Coordinate, FromBech32};

    // Decode naddr string to Nip19Coordinate
    // This preserves relay hints if present in the naddr
    match Nip19Coordinate::from_bech32(naddr) {
        Ok(nip19_coord) => {
            // Extract coordinate fields
            let pubkey = nip19_coord.public_key.to_hex();
            let identifier = nip19_coord.identifier.clone();

            // Log relay hints if present
            if !nip19_coord.relays.is_empty() {
                log::debug!("Article naddr contains {} relay hints", nip19_coord.relays.len());
                for relay in &nip19_coord.relays {
                    log::debug!("  Relay hint: {}", relay);
                }
            }

            Ok((pubkey, identifier))
        }
        Err(e) => Err(format!("Invalid naddr format: {}", e)),
    }
}

/// Format timestamp to human-readable string
fn format_timestamp(timestamp: u64) -> String {
    use chrono::{DateTime, Utc};

    let dt = DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or_else(|| Utc::now());

    dt.format("%B %d, %Y").to_string()
}
