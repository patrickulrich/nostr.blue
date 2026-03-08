use crate::components::icons::{BookmarkIcon, MessageCircleIcon, Repeat2Icon, ZapIcon};
use crate::components::{ReactionButton, ZapModal};
use crate::hooks::use_reaction;
use crate::routes::Route;
use crate::services::aggregation::InteractionCounts;
use crate::stores::bookmarks;
use crate::stores::nostr_client::{get_client, publish_note_tracked, publish_repost, HAS_SIGNER, CLIENT_INITIALIZED};
use crate::stores::signer::SIGNER_INFO;
use crate::utils::{format_relative_time_or, format_sats_compact, truncate_pubkey};
use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, FromBech32, Kind, PublicKey};
use std::time::Duration;
/// Skeleton loader for PhotoCard - prevents layout shift during loading
#[component]
pub fn PhotoCardSkeleton() -> Element {
    rsx! {
        div { class: "border-b border-border bg-background mb-4 animate-pulse",
            div { class: "p-3 flex items-center gap-3",
                div { class: "w-8 h-8 rounded-full bg-gray-300 dark:bg-gray-700" }
                div { class: "flex-1",
                    div { class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-1/4" }
                }
                div { class: "h-3 bg-gray-300 dark:bg-gray-700 rounded w-16" }
            }
            div { class: "relative bg-gray-300 dark:bg-gray-700 aspect-square max-h-[600px]" }
            div { class: "flex items-center gap-4 px-3 py-2",
                for _ in 0..4 {
                    div { class: "w-6 h-6 bg-gray-300 dark:bg-gray-700 rounded" }
                }
            }
            div { class: "px-3 pb-2",
                div { class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-20" }
            }
            div { class: "px-3 pb-3 space-y-2",
                div { class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-full" }
                div { class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-3/4" }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct ImageMeta {
    pub url: String,
    pub alt: Option<String>,
    pub blurhash: Option<String>,
    pub dim: Option<(u32, u32)>,
}
/// Parse imeta tags from NIP-68 picture events
pub fn parse_imeta_tags(event: &Event) -> Vec<ImageMeta> {
    let mut images = Vec::new();
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("imeta") {
            let mut image = ImageMeta {
                url: String::new(),
                alt: None,
                blurhash: None,
                dim: None,
            };
            for field in tag_vec.iter().skip(1) {
                if let Some((key, value)) = field.split_once(' ') {
                    match key {
                        "url" => image.url = value.to_string(),
                        "alt" => image.alt = Some(value.to_string()),
                        "blurhash" => image.blurhash = Some(value.to_string()),
                        "dim" => {
                            if let Some((w, h)) = value.split_once('x') {
                                if let (Ok(width), Ok(height)) = (w.parse(), h.parse()) {
                                    image.dim = Some((width, height));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            if !image.url.is_empty() {
                images.push(image);
            }
        }
    }
    images
}
/// Get the title from NIP-68 picture events
pub fn get_title(event: &Event) -> Option<String> {
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("title") {
            return tag_vec.get(1).map(|s| s.to_string());
        }
    }
    None
}
#[component]
pub fn PhotoCard(
    event: Event,
    #[props(default = None)] precomputed_counts: Option<InteractionCounts>,
) -> Element {
    let images = parse_imeta_tags(&event);
    let title = get_title(&event);
    let description = &event.content;
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_fetch = author_pubkey.clone();
    let author_pubkey_like = author_pubkey.clone();
    let author_pubkey_comment = author_pubkey.clone();
    let author_pubkey_comment_btn = author_pubkey.clone();
    let created_at = event.created_at;
    let event_id = event.id.to_string();
    let event_id_like = event_id.clone();
    let event_id_bookmark = event_id.clone();
    let event_id_memo = event_id.clone();
    let event_id_counts = event_id.clone();
    let event_id_comment = event_id.clone();
    let event_id_comment_btn = event_id.clone();
    let event_id_link = event_id.clone();
    let event_id_repost = event_id.clone();
    let images_carousel = images.clone();
    let mut is_zapped = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    let is_bookmarked = bookmarks::is_bookmarked(&event_id_memo);
    let has_signer = *HAS_SIGNER.read();
    let reaction = use_reaction(
        event_id_like.clone(),
        author_pubkey_like.clone(),
        precomputed_counts.as_ref(),
    );
    let mut current_image_index = use_signal(|| 0usize);
    let mut reply_count = use_signal(|| 0usize);
    let mut repost_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);
    let mut is_reposting = use_signal(|| false);
    let mut is_reposted = use_signal(|| false);
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut comment_text = use_signal(String::new);
    let mut is_posting_comment = use_signal(|| false);
    let mut show_zap_modal = use_signal(|| false);
    if images.is_empty() {
        return rsx! {
            div { class: "hidden" }
        };
    }
    use_effect(use_reactive(&precomputed_counts, move |counts_opt| {
        if let Some(counts) = counts_opt {
            reply_count.set(counts.replies);
            repost_count.set(counts.reposts);
            zap_amount_sats.set(counts.zap_amount_sats);
            is_reposted.set(counts.user_reposted.unwrap_or(false));
            is_zapped.set(counts.user_zapped.unwrap_or(false));
        }
    }));
    let has_precomputed = precomputed_counts.is_some();
    let client_initialized = *CLIENT_INITIALIZED.read();
    use_effect(use_reactive(
        &(event_id_counts, has_precomputed, client_initialized),
        move |(event_id_for_counts, has_precomputed, client_ready)| {
            if has_precomputed || !client_ready {
                return;
            }
            spawn(async move {
                let client = match get_client() {
                    Some(c) => c,
                    None => return,
                };
                let event_id_parsed = match nostr_sdk::EventId::from_hex(&event_id_for_counts) {
                    Ok(id) => id,
                    Err(_) => return,
                };
                let event_id_hex = event_id_parsed.to_hex();
                let reply_filter_lower = Filter::new()
                    .kinds(vec![Kind::TextNote, Kind::Comment, Kind::Repost])
                    .event(event_id_parsed)
                    .limit(500);
                let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
                let reply_filter_upper = Filter::new()
                    .kind(Kind::Comment)
                    .custom_tag(upper_e_tag, event_id_hex)
                    .limit(500);
                let mut all_replies = Vec::new();
                if let Ok(lower_replies) = client
                    .fetch_events(reply_filter_lower, Duration::from_secs(5))
                    .await
                {
                    all_replies.extend(lower_replies.into_iter());
                }
                if let Ok(upper_replies) = client
                    .fetch_events(reply_filter_upper, Duration::from_secs(5))
                    .await
                {
                    all_replies.extend(upper_replies.into_iter());
                }
                let mut seen_ids = std::collections::HashSet::new();
                let unique_events: Vec<_> = all_replies
                    .into_iter()
                    .filter(|event| seen_ids.insert(event.id))
                    .collect();
                let current_user_pubkey = SIGNER_INFO
                    .read()
                    .as_ref()
                    .map(|info| info.public_key.clone());
                let mut replies = 0usize;
                let mut reposts = 0usize;
                let mut user_has_reposted = false;
                for event in unique_events.iter() {
                    match event.kind {
                        Kind::TextNote | Kind::Comment => replies += 1,
                        Kind::Repost => {
                            reposts += 1;
                            if let Some(ref user_pk) = current_user_pubkey {
                                if event.pubkey.to_string() == *user_pk {
                                    user_has_reposted = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                reply_count.set(replies);
                repost_count.set(reposts);
                is_reposted.set(user_has_reposted);
                let zap_filter = Filter::new()
                    .kind(Kind::ZapReceipt)
                    .event(event_id_parsed)
                    .limit(500);
                if let Ok(zaps) = client
                    .fetch_events(zap_filter, Duration::from_secs(5))
                    .await
                {
                    let current_user_pubkey = SIGNER_INFO
                        .read()
                        .as_ref()
                        .map(|info| info.public_key.clone());
                    let mut user_has_zapped = false;
                    let total_sats: u64 = zaps
                        .iter()
                        .filter_map(|zap_event| {
                            if let Some(ref user_pk) = current_user_pubkey {
                                let mut zap_sender_pubkey = zap_event.tags.iter().find_map(|tag| {
                                    let tag_vec = tag.clone().to_vec();
                                    if tag_vec.len() >= 2 && tag_vec.first()?.as_str() == "P" {
                                        Some(tag_vec.get(1)?.as_str().to_string())
                                    } else {
                                        None
                                    }
                                });
                                if zap_sender_pubkey.is_none() {
                                    zap_sender_pubkey = zap_event.tags.iter().find_map(|tag| {
                                        let tag_vec = tag.clone().to_vec();
                                        if tag_vec.first()?.as_str() == "description" {
                                            let zap_request_json = tag_vec.get(1)?.as_str();
                                            if let Ok(zap_request) =
                                                serde_json::from_str::<serde_json::Value>(
                                                    zap_request_json,
                                                )
                                            {
                                                return zap_request
                                                    .get("pubkey")
                                                    .and_then(|p| p.as_str())
                                                    .map(|s| s.to_string());
                                            }
                                        }
                                        None
                                    });
                                }
                                if let Some(zap_sender) = zap_sender_pubkey {
                                    if zap_sender == *user_pk {
                                        user_has_zapped = true;
                                    }
                                }
                            }
                            zap_event.tags.iter().find_map(|tag| {
                                let tag_vec = tag.clone().to_vec();
                                if tag_vec.first()?.as_str() == "description" {
                                    let zap_request_json = tag_vec.get(1)?.as_str();
                                    if let Ok(zap_request) =
                                        serde_json::from_str::<serde_json::Value>(zap_request_json)
                                    {
                                        if let Some(tags) =
                                            zap_request.get("tags").and_then(|t| t.as_array())
                                        {
                                            for tag_array in tags {
                                                if let Some(tag_vals) = tag_array.as_array() {
                                                    if tag_vals.first().and_then(|v| v.as_str())
                                                        == Some("amount")
                                                    {
                                                        if let Some(amount_str) =
                                                            tag_vals.get(1).and_then(|v| v.as_str())
                                                        {
                                                            if let Ok(millisats) =
                                                                amount_str.parse::<u64>()
                                                            {
                                                                return Some(millisats / 1000);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                None
                            })
                        })
                        .sum();
                    zap_amount_sats.set(total_sats);
                    is_zapped.set(user_has_zapped);
                }
            });
        },
    ));
    use_effect(use_reactive(
        &(author_pubkey_for_fetch, client_initialized),
        move |(pubkey_str, client_ready)| {
            if !client_ready {
                return;
            }
            spawn(async move {
            let pubkey = match PublicKey::from_hex(&pubkey_str)
                .or_else(|_| PublicKey::from_bech32(&pubkey_str))
            {
                Ok(pk) => pk,
                Err(_) => return,
            };
            let client = match get_client() {
                Some(c) => c,
                None => return,
            };
            let filter = Filter::new().author(pubkey).kind(Kind::Metadata).limit(1);
            if let Ok(events) = client.fetch_events(filter, Duration::from_secs(5)).await {
                if let Some(event) = events.into_iter().next() {
                    if let Ok(metadata) =
                        serde_json::from_str::<nostr_sdk::Metadata>(&event.content)
                    {
                        author_metadata.set(Some(metadata));
                    }
                }
            }
        });
    }));
    let timestamp = format_relative_time_or(created_at.as_secs(), "just now");
    let display_name = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));
    let picture_url = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone());
    let nav = use_navigator();
    let event_id_nav = event_id.clone();
    rsx! {
        div {
            class: "border-b border-border bg-background mb-4 cursor-pointer hover:bg-accent/5 transition",
            onclick: move |_| {
                nav.push(Route::PhotoDetail {
                    photo_id: event_id_nav.clone(),
                });
            },
            div { class: "p-3 flex items-center gap-3",
                Link {
                    to: Route::Profile {
                        pubkey: author_pubkey.clone(),
                    },
                    class: "shrink-0",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    if let Some(pic) = picture_url {
                        img {
                            class: "w-8 h-8 rounded-full object-cover",
                            src: "{pic}",
                            alt: "Profile",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-8 h-8 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white font-bold text-sm",
                            "{display_name.chars().next().unwrap_or('?').to_ascii_uppercase()}"
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    Link {
                        to: Route::Profile {
                            pubkey: author_pubkey.clone(),
                        },
                        class: "font-semibold hover:underline text-sm",
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        "{display_name}"
                    }
                }
                span { class: "text-xs text-muted-foreground", "{timestamp}" }
            }
            div { class: "relative bg-black",
                img {
                    class: "w-full max-h-[600px] object-contain",
                    src: "{images[*current_image_index.read()].url}",
                    alt: "{images[*current_image_index.read()].alt.as_deref().unwrap_or(\"Photo\")}",
                    loading: "lazy",
                }
                if images.len() > 1 {
                    div { class: "absolute bottom-3 left-1/2 -translate-x-1/2 flex gap-1.5",
                        for (idx , _) in images.iter().enumerate() {
                            button {
                                class: if idx == *current_image_index.read() { "w-2 h-2 rounded-full bg-white" } else { "w-2 h-2 rounded-full bg-white/50" },
                                onclick: move |_| current_image_index.set(idx),
                            }
                        }
                    }
                    if *current_image_index.read() > 0 {
                        button {
                            class: "absolute left-2 top-1/2 -translate-y-1/2 bg-black/50 text-white rounded-full p-2 hover:bg-black/70 transition",
                            onclick: move |_| {
                                let current = *current_image_index.read();
                                if current > 0 {
                                    current_image_index.set(current - 1);
                                }
                            },
                            "‹"
                        }
                    }
                    if *current_image_index.read() < images_carousel.len() - 1 {
                        button {
                            class: "absolute right-2 top-1/2 -translate-y-1/2 bg-black/50 text-white rounded-full p-2 hover:bg-black/70 transition",
                            onclick: move |_| {
                                let current = *current_image_index.read();
                                if current < images_carousel.len() - 1 {
                                    current_image_index.set(current + 1);
                                }
                            },
                            "›"
                        }
                    }
                }
            }
            div { class: "flex items-center gap-4 px-3 py-2",
                ReactionButton {
                    reaction: reaction.clone(),
                    has_signer,
                    icon_class: "w-6 h-6".to_string(),
                    count_class: "text-sm".to_string(),
                }
                Link {
                    to: Route::PhotoDetail {
                        photo_id: event_id_link.clone(),
                    },
                    class: "flex items-center gap-1 hover:text-blue-500 transition",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    MessageCircleIcon { class: "w-6 h-6".to_string(), filled: false }
                    if *reply_count.read() > 0 {
                        span { class: "text-sm",
                            {
                                let count = *reply_count.read();
                                if count > 0 { count.to_string() } else { "".to_string() }
                            }
                        }
                    }
                }
                button {
                    class: if *is_reposted.read() { "flex items-center gap-1 text-green-500 transition" } else { "flex items-center gap-1 hover:text-green-500 transition" },
                    disabled: !has_signer || *is_reposting.read(),
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        if !has_signer || *is_reposting.read() {
                            return;
                        }
                        let event_id_clone = event_id_repost.clone();
                        is_reposting.set(true);
                        spawn(async move {
                            match publish_repost(event_id_clone, None).await {
                                Ok(repost_id) => {
                                    log::info!("Reposted photo, repost ID: {}", repost_id);
                                    is_reposted.set(true);
                                    is_reposting.set(false);
                                }
                                Err(e) => {
                                    log::error!("Failed to repost photo: {}", e);
                                    is_reposting.set(false);
                                }
                            }
                        });
                    },
                    Repeat2Icon { class: "w-6 h-6".to_string(), filled: false }
                    if *repost_count.read() > 0 {
                        span { class: "text-sm",
                            {
                                let count = *repost_count.read();
                                if count > 500 { "500+".to_string() } else { count.to_string() }
                            }
                        }
                    }
                }
                {
                    let has_lightning = author_metadata
                        .read()
                        .as_ref()
                        .and_then(|m| m.lud16.as_ref().or(m.lud06.as_ref()))
                        .is_some();
                    if has_lightning {
                        rsx! {
                            button {
                                class: if *is_zapped.read() { "flex items-center gap-1 text-yellow-500 transition" } else { "flex items-center gap-1 hover:text-yellow-500 transition" },
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_zap_modal.set(true);
                                },
                                ZapIcon { class: "w-6 h-6".to_string(), filled: *is_zapped.read() }
                                span { class: "text-sm",
                                    {
                                        let amount = *zap_amount_sats.read();
                                        if amount > 0 { format_sats_compact(amount) } else { "".to_string() }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
                button {
                    class: if is_bookmarked { "flex items-center gap-1 text-blue-500" } else { "flex items-center gap-1 hover:text-blue-500 transition ml-auto" },
                    disabled: !has_signer || *is_bookmarking.read(),
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        if !has_signer || *is_bookmarking.read() {
                            return;
                        }
                        let event_id_clone = event_id_bookmark.clone();
                        is_bookmarking.set(true);
                        spawn(async move {
                            let result = if bookmarks::is_bookmarked(&event_id_clone) {
                                bookmarks::unbookmark_event(event_id_clone).await
                            } else {
                                bookmarks::bookmark_event(event_id_clone).await
                            };
                            match result {
                                Ok(_) => {
                                    is_bookmarking.set(false);
                                }
                                Err(e) => {
                                    log::error!("Failed to bookmark: {}", e);
                                    is_bookmarking.set(false);
                                }
                            }
                        });
                    },
                    BookmarkIcon { class: "w-6 h-6".to_string(), filled: is_bookmarked }
                }
            }
            div { class: "px-3 pb-2",
                if *reaction.like_count.read() > 0 {
                    span { class: "font-semibold text-sm",
                        {
                            let count = *reaction.like_count.read();
                            if count == 1 { format!("{} like", count) } else { format!("{} likes", count) }
                        }
                    }
                }
            }
            div { class: "px-3 pb-2",
                if let Some(title_text) = &title {
                    div { class: "mb-1",
                        span { class: "font-semibold text-sm mr-2", "{display_name}" }
                        span { class: "font-bold", "{title_text}" }
                    }
                }
                if !description.is_empty() {
                    div {
                        if title.is_none() {
                            span { class: "font-semibold text-sm mr-2", "{display_name}" }
                        }
                        span { class: "text-sm", "{description}" }
                    }
                }
            }
            if *reply_count.read() > 0 {
                Link {
                    to: Route::PhotoDetail {
                        photo_id: event_id_link.clone(),
                    },
                    class: "px-3 pb-2 block text-sm text-muted-foreground hover:underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    {
                        let count = *reply_count.read();
                        if count == 1 {
                            "View 1 comment".to_string()
                        } else {
                            format!("View all {} comments", count)
                        }
                    }
                }
            }
            if has_signer {
                div {
                    class: "px-3 pb-3 flex items-center gap-2",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    input {
                        class: "flex-1 bg-transparent border-none outline-hidden text-sm placeholder:text-muted-foreground",
                        r#type: "text",
                        placeholder: "Add a comment...",
                        value: "{comment_text}",
                        oninput: move |evt| comment_text.set(evt.value()),
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter && !comment_text.read().is_empty()
                                && !*is_posting_comment.read()
                            {
                                let text = comment_text.read().clone();
                                let event_id_clone = event_id_comment.clone();
                                let author_clone = author_pubkey_comment.clone();
                                is_posting_comment.set(true);
                                comment_text.set(String::new());
                                spawn(async move {
                                    let tags = vec![
                                        vec!["e".to_string(), event_id_clone],
                                        vec!["p".to_string(), author_clone],
                                    ];
                                    match publish_note_tracked(text, tags).await {
                                        Ok(result) => {
                                            log::info!(
                                                "Photo comment published: {} ({}/{} relays)", result
                                                .event_id, result.success_count(), result.total_attempted()
                                            );
                                            if result.has_failures() {
                                                for (relay, error) in &result.failed_relays {
                                                    log::warn!("Relay {} failed: {}", relay, error);
                                                }
                                            }
                                            let current_count = *reply_count.read();
                                            reply_count.set(current_count + 1);
                                            is_posting_comment.set(false);
                                        }
                                        Err(e) => {
                                            log::error!("Failed to post comment: {}", e);
                                            is_posting_comment.set(false);
                                        }
                                    }
                                });
                            }
                        },
                    }
                    if !comment_text.read().is_empty() {
                        button {
                            class: "text-blue-500 font-semibold text-sm hover:text-blue-600 disabled:opacity-50",
                            disabled: *is_posting_comment.read(),
                            onclick: move |_| {
                                if comment_text.read().is_empty() || *is_posting_comment.read() {
                                    return;
                                }
                                let text = comment_text.read().clone();
                                let event_id_clone = event_id_comment_btn.clone();
                                let author_clone = author_pubkey_comment_btn.clone();
                                is_posting_comment.set(true);
                                comment_text.set(String::new());
                                spawn(async move {
                                    let tags = vec![
                                        vec!["e".to_string(), event_id_clone],
                                        vec!["p".to_string(), author_clone],
                                    ];
                                    match publish_note_tracked(text, tags).await {
                                        Ok(result) => {
                                            log::info!(
                                                "Photo comment published: {} ({}/{} relays)", result.event_id,
                                                result.success_count(), result.total_attempted()
                                            );
                                            if result.has_failures() {
                                                for (relay, error) in &result.failed_relays {
                                                    log::warn!("Relay {} failed: {}", relay, error);
                                                }
                                            }
                                            let current_count = *reply_count.read();
                                            reply_count.set(current_count + 1);
                                            is_posting_comment.set(false);
                                        }
                                        Err(e) => {
                                            log::error!("Failed to post comment: {}", e);
                                            is_posting_comment.set(false);
                                        }
                                    }
                                });
                            },
                            "Post"
                        }
                    }
                }
            }
        }
        if *show_zap_modal.read() {
            ZapModal {
                recipient_pubkey: author_pubkey.clone(),
                recipient_name: display_name.clone(),
                lud16: author_metadata.read().as_ref().and_then(|m| m.lud16.clone()),
                lud06: author_metadata.read().as_ref().and_then(|m| m.lud06.clone()),
                event_id: Some(event_id.clone()),
                on_close: move |_| {
                    show_zap_modal.set(false);
                },
            }
        }
    }
}
