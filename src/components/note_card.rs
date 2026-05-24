use crate::components::icons::{
    BlueskyIcon, BookmarkIcon, ExternalLinkIcon, GlobeIcon, LockIcon, MastodonIcon, MessageCircleIcon,
    Repeat2Icon, RssIcon, ZapIcon,
};
use crate::components::{
    ConfirmModal, EditStatus, ExternalContentList, NoteMenu, ReactionButton, ReplyComposer,
    RichContent, SensitiveContent, ZapModal,
};
use crate::hooks::use_reaction;
use crate::hooks::use_global_interaction::{get_global_interaction, UseGlobalInteraction};
use crate::routes::Route;
use crate::services::aggregation::InteractionCounts;
use crate::stores::bookmarks;
use crate::stores::edit_cache;
use crate::stores::nostr_client::{self, delete_repost, get_client, publish_repost, HAS_SIGNER};
use crate::stores::signer::SIGNER_INFO;
use crate::utils::{
    format_relative_time_or, format_sats_compact, is_valid_http_url, nip48, nip73, truncate_pubkey,
};
use crate::utils::nip36;
use dioxus::prelude::*;
use nostr::nips::nip48::Protocol;
use nostr_sdk::nips::nip19::Nip19Event;
use nostr_sdk::{Event as NostrEvent, Filter, Kind, PublicKey, Timestamp, ToBech32};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;

#[cfg(feature = "web")]
const INTERACTIVE_ELEMENT_SELECTOR: &str =
    "a, button, input, textarea, select, summary, [role='button'], [role='link'], [contenteditable='true'], video, audio, iframe, [data-interactive]";

trait ProfileMetadataView {
    fn name(&self) -> Option<&str>;
    fn display_name(&self) -> Option<&str>;
    fn about(&self) -> Option<&str>;
    fn picture(&self) -> Option<&str>;
    fn banner(&self) -> Option<&str>;
    fn website(&self) -> Option<&str>;
    fn nip05(&self) -> Option<&str>;
    fn lud16(&self) -> Option<&str>;
    fn lud06(&self) -> Option<&str>;
}

impl ProfileMetadataView for crate::stores::profiles::Profile {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
    fn about(&self) -> Option<&str> {
        self.about.as_deref()
    }
    fn picture(&self) -> Option<&str> {
        self.picture.as_deref()
    }
    fn banner(&self) -> Option<&str> {
        self.banner.as_deref()
    }
    fn website(&self) -> Option<&str> {
        self.website.as_deref()
    }
    fn nip05(&self) -> Option<&str> {
        self.nip05.as_deref()
    }
    fn lud16(&self) -> Option<&str> {
        self.lud16.as_deref()
    }
    fn lud06(&self) -> Option<&str> {
        self.lud06.as_deref()
    }
}

impl ProfileMetadataView for nostr_sdk::Metadata {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
    fn about(&self) -> Option<&str> {
        self.about.as_deref()
    }
    fn picture(&self) -> Option<&str> {
        self.picture.as_deref()
    }
    fn banner(&self) -> Option<&str> {
        self.banner.as_deref()
    }
    fn website(&self) -> Option<&str> {
        self.website.as_deref()
    }
    fn nip05(&self) -> Option<&str> {
        self.nip05.as_deref()
    }
    fn lud16(&self) -> Option<&str> {
        self.lud16.as_deref()
    }
    fn lud06(&self) -> Option<&str> {
        self.lud06.as_deref()
    }
}

fn metadata_from_profile_like<T: ProfileMetadataView>(profile: &T) -> nostr_sdk::Metadata {
    let mut metadata = nostr_sdk::Metadata::new();
    if let Some(name) = profile.name() {
        metadata = metadata.name(name);
    }
    if let Some(display_name) = profile.display_name() {
        metadata = metadata.display_name(display_name);
    }
    if let Some(about) = profile.about() {
        metadata = metadata.about(about);
    }
    if let Some(picture) = profile.picture() {
        if let Ok(url) = nostr_sdk::Url::parse(picture) {
            metadata = metadata.picture(url);
        }
    }
    if let Some(banner) = profile.banner() {
        if let Ok(url) = nostr_sdk::Url::parse(banner) {
            metadata = metadata.banner(url);
        }
    }
    if let Some(website) = profile.website() {
        if let Ok(url) = nostr_sdk::Url::parse(website) {
            metadata = metadata.website(url);
        }
    }
    if let Some(nip05) = profile.nip05() {
        metadata = metadata.nip05(nip05);
    }
    if let Some(lud16) = profile.lud16() {
        metadata = metadata.lud16(lud16);
    }
    if let Some(lud06) = profile.lud06() {
        metadata = metadata.lud06(lud06);
    }
    metadata
}
#[component]
pub fn NoteCard(
    event: NostrEvent,
    #[props(default = None)] repost_info: Option<(PublicKey, Timestamp)>,
    #[props(default = None)] precomputed_counts: Option<InteractionCounts>,
    #[props(default = true)] collapsible: bool,
    #[props(default = None)] cached_muted_posts: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_blocked_users: Option<Rc<HashSet<String>>>,
    #[props(default = None)] on_reply: Option<EventHandler<NostrEvent>>,
    #[props(default = None)] root_event: Option<NostrEvent>,
) -> Element {
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_like = author_pubkey.clone();
    let author_pubkey_for_fetch = author_pubkey.clone();
    let content = event.content.clone();
    let created_at = event.created_at;
    let event_id = event.id.to_string();
    let event_id_repost = event_id.clone();
    let event_id_like = event_id.clone();
    let event_id_bookmark = event_id.clone();
    let event_id_memo = event_id.clone();
    let event_id_counts = event_id.clone();
    let mut is_reposting = use_signal(|| false);
    let mut is_reposted = use_signal(|| false);
    let mut user_repost_id = use_signal(|| None::<String>);
    let mut show_undo_repost_confirm = use_signal(|| false);
    let mut is_zapped = use_signal(|| false);
    let mut show_reply_modal = use_signal(|| false);
    let mut show_zap_modal = use_signal(|| false);
    let mut show_repost_menu = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    let is_bookmarked = bookmarks::is_bookmarked(&event_id_memo);
    let has_signer = *HAS_SIGNER.read();
    let mut is_muted = use_signal(|| None::<bool>);
    let mut is_author_blocked = use_signal(|| None::<bool>);
    let mut show_hidden_anyway = use_signal(|| false);
    let mut reply_count = use_signal(|| 0usize);
    let mut repost_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);
    let mut count_request_gen = use_signal(|| 0u32);
    let reaction = use_reaction(
        event_id_like.clone(),
        author_pubkey_like.clone(),
        precomputed_counts.as_ref(),
    );
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut reposter_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut author_metadata_gen = use_signal(|| 0u32);
    let mut reposter_metadata_gen = use_signal(|| 0u32);
    use_effect(use_reactive(&precomputed_counts, move |counts_opt| {
        if let Some(counts) = counts_opt {
            reply_count.set(counts.replies);
            repost_count.set(counts.reposts);
            zap_amount_sats.set(counts.zap_amount_sats);
            is_reposted.set(counts.user_reposted.unwrap_or(false));
            user_repost_id.set(counts.user_repost_id.clone());
            is_zapped.set(counts.user_zapped.unwrap_or(false));
        }
    }));
    let has_precomputed = precomputed_counts.is_some();
    let event_id_for_global = event_id.clone();
    use_effect(use_reactive(
        &(event_id_counts, has_precomputed),
        move |(event_id_for_counts, has_precomputed)| {
            let current_gen = count_request_gen.peek().wrapping_add(1);
            count_request_gen.set(current_gen);
            if has_precomputed {
                return;
            }
            if let Some(global_counts) = get_global_interaction(&event_id_for_counts) {
                reply_count.set(global_counts.replies);
                repost_count.set(global_counts.reposts);
                zap_amount_sats.set(global_counts.zap_amount_sats);
                is_reposted.set(global_counts.user_reposted.unwrap_or(false));
                user_repost_id.set(global_counts.user_repost_id.clone());
                is_zapped.set(global_counts.user_zapped.unwrap_or(false));
                return;
            }
            reply_count.set(0);
            repost_count.set(0);
            zap_amount_sats.set(0);
            is_reposted.set(false);
            user_repost_id.set(None);
            is_zapped.set(false);
            // Read SIGNER_INFO synchronously so the effect tracks viewer changes
            let current_user_pubkey = SIGNER_INFO
                .read()
                .as_ref()
                .map(|info| info.public_key.clone());
            spawn(async move {
                let client = match get_client() {
                    Some(c) => c,
                    None => return,
                };
                let event_id_parsed = match nostr_sdk::EventId::from_hex(&event_id_for_counts) {
                    Ok(id) => id,
                    Err(_) => return,
                };
                let combined_filter = Filter::new()
                    .kinds(vec![Kind::TextNote, Kind::Repost, Kind::ZapReceipt])
                    .event(event_id_parsed)
                    .limit(2000);
                if let Ok(events) = client
                    .fetch_events(combined_filter, Duration::from_secs(5))
                    .await
                {
                    if *count_request_gen.peek() != current_gen {
                        return;
                    }
                    let mut replies = 0;
                    let mut reposts = 0;
                    let mut total_sats = 0u64;
                    let mut user_has_reposted = false;
                    let mut user_repost_event_id: Option<String> = None;
                    let mut user_has_zapped = false;
                    for event in events {
                        match event.kind {
                            Kind::TextNote => replies += 1,
                            Kind::Repost => {
                                reposts += 1;
                                if let Some(ref user_pk) = current_user_pubkey {
                                    if event.pubkey.to_string() == *user_pk {
                                        user_has_reposted = true;
                                        user_repost_event_id = Some(event.id.to_hex());
                                    }
                                }
                            }
                            Kind::ZapReceipt => {
                                if let Some(ref user_pk) = current_user_pubkey {
                                    let zap_sender_pubkey = event.tags.iter().find_map(|tag| {
                                        let slice = tag.as_slice();
                                        if slice.first()?.as_str() == "description" {
                                            let zap_request_json = slice.get(1)?.as_str();
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
                                    if let Some(zap_sender) = zap_sender_pubkey {
                                        if zap_sender == *user_pk {
                                            user_has_zapped = true;
                                        }
                                    }
                                }
                                if let Some(amount) = event.tags.iter().find_map(|tag| {
                                    let slice = tag.as_slice();
                                    if slice.first()?.as_str() == "description" {
                                        let zap_request_json = slice.get(1)?.as_str();
                                        if let Ok(zap_request) =
                                            serde_json::from_str::<serde_json::Value>(
                                                zap_request_json,
                                            )
                                        {
                                            if let Some(tags) =
                                                zap_request.get("tags").and_then(|t| t.as_array())
                                            {
                                                for tag_array in tags {
                                                    if let Some(tag_vals) = tag_array.as_array() {
                                                        if tag_vals.first().and_then(|v| v.as_str())
                                                            == Some("amount")
                                                        {
                                                            if let Some(amount_str) = tag_vals
                                                                .get(1)
                                                                .and_then(|v| v.as_str())
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
                                }) {
                                    total_sats += amount;
                                }
                            }
                            _ => {}
                        }
                    }
                    let current_replies = *reply_count.peek();
                    let current_reposts = *repost_count.peek();
                    let current_zaps = *zap_amount_sats.peek();
                    if replies != current_replies {
                        reply_count.set(replies);
                    }
                    if reposts != current_reposts {
                        repost_count.set(reposts);
                    }
                    if total_sats != current_zaps {
                        zap_amount_sats.set(total_sats);
                    }
                    is_reposted.set(user_has_reposted);
                    user_repost_id.set(user_repost_event_id);
                    is_zapped.set(user_has_zapped);
                }
            });
        },
    ));
    use_effect(use_reactive(&author_pubkey_for_fetch, move |pubkey_str| {
        let current_gen = author_metadata_gen.peek().wrapping_add(1);
        author_metadata_gen.set(current_gen);
        author_metadata.set(None);
        spawn(async move {
            if let Some(cached_profile) = crate::stores::profiles::get_cached_profile(&pubkey_str) {
                let metadata = metadata_from_profile_like(&cached_profile);
                if *author_metadata_gen.peek() != current_gen {
                    return;
                }
                author_metadata.set(Some(metadata));
                return;
            }
            match crate::stores::profiles::fetch_profile(pubkey_str.clone()).await {
                Ok(profile) => {
                    let metadata = metadata_from_profile_like(&profile);
                    if *author_metadata_gen.peek() != current_gen {
                        return;
                    }
                    author_metadata.set(Some(metadata));
                }
                Err(e) => {
                    log::debug!("Failed to fetch profile for {}: {}", pubkey_str, e);
                }
            }
        });
    }));
    use_effect(use_reactive(&repost_info, move |info_opt| {
        let current_gen = reposter_metadata_gen.peek().wrapping_add(1);
        reposter_metadata_gen.set(current_gen);
        reposter_metadata.set(None);
        if let Some((reposter_pubkey, _timestamp)) = info_opt {
            let reposter_pubkey_str = reposter_pubkey.to_string();
            spawn(async move {
                if let Some(cached_profile) =
                    crate::stores::profiles::get_cached_profile(&reposter_pubkey_str)
                {
                    let metadata = metadata_from_profile_like(&cached_profile);
                    if *reposter_metadata_gen.peek() != current_gen {
                        return;
                    }
                    reposter_metadata.set(Some(metadata));
                    return;
                }
                match crate::stores::profiles::fetch_profile(reposter_pubkey_str.clone()).await {
                    Ok(profile) => {
                        let metadata = metadata_from_profile_like(&profile);
                        if *reposter_metadata_gen.peek() != current_gen {
                            return;
                        }
                        reposter_metadata.set(Some(metadata));
                    }
                    Err(e) => {
                        log::debug!("Failed to fetch reposter profile: {}", e);
                    }
                }
            });
        }
    }));
    let event_id_mute_check = event_id.clone();
    let author_pubkey_block_check = author_pubkey.clone();
    use_effect(use_reactive!(|(
        cached_muted_posts,
        cached_blocked_users,
        event_id_mute_check,
        author_pubkey_block_check,
    )| {
        let event_id = event_id_mute_check.clone();
        let author_pubkey = author_pubkey_block_check.clone();
        // Check cached values first - these give us definitive Known(true/false)
        if let Some(ref muted_set) = cached_muted_posts {
            if let Ok(muted) = nostr_client::is_post_muted_cached(&event_id, muted_set) {
                is_muted.set(Some(muted));
            }
        }
        if let Some(ref blocked_set) = cached_blocked_users {
            if let Ok(blocked) = nostr_client::is_user_blocked_cached(&author_pubkey, blocked_set) {
                is_author_blocked.set(Some(blocked));
            }
        }
        // Only spawn async if we don't have cached values (Unknown -> Known transition)
        if cached_muted_posts.is_none() || cached_blocked_users.is_none() {
            let need_muted = cached_muted_posts.is_none();
            let need_blocked = cached_blocked_users.is_none();
            spawn(async move {
                if need_muted {
                    match nostr_client::is_post_muted(event_id.clone()).await {
                        Ok(muted) => is_muted.set(Some(muted)),
                        Err(_) => is_muted.set(Some(false)),
                    }
                }
                if need_blocked {
                    match nostr_client::is_user_blocked(author_pubkey).await {
                        Ok(blocked) => is_author_blocked.set(Some(blocked)),
                        Err(_) => is_author_blocked.set(Some(false)),
                    }
                }
            });
        }
    }));
    let timestamp = format_relative_time_or(created_at.as_secs(), "just now");
    let display_name = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));
    let username = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.name.clone())
        .unwrap_or_else(|| {
            if let Ok(pk) = PublicKey::from_hex(&author_pubkey) {
                match pk.to_bech32() {
                    Ok(npub) => {
                        if npub.len() > 18 {
                            format!("{}...{}", &npub[..12], &npub[npub.len() - 6..])
                        } else {
                            npub
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to encode pubkey to bech32: {}, using hex fallback",
                            e
                        );
                        truncate_pubkey(&author_pubkey)
                    }
                }
            } else {
                "unknown".to_string()
            }
        });
    let profile_picture = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone());
    let reposter_display_info = repost_info.map(|(reposter_pubkey, repost_timestamp)| {
        let reposter_pubkey_str = reposter_pubkey.to_string();
        let reposter_display = reposter_metadata
            .read()
            .as_ref()
            .and_then(|m| m.display_name.clone().or_else(|| m.name.clone()))
            .unwrap_or_else(|| truncate_pubkey(&reposter_pubkey_str));
        let repost_time = format_relative_time_or(repost_timestamp.as_secs(), "just now");
        (reposter_pubkey_str, reposter_display, repost_time)
    });
    let repost_button_class = if *is_reposted.read() {
        "flex items-center text-green-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-green-500 transition"
    };
    let zap_button_class = if *is_zapped.read() {
        "flex items-center gap-1 text-yellow-500 transition px-2 py-1.5 rounded"
    } else {
        "flex items-center gap-1 text-muted-foreground hover:text-yellow-500 hover:bg-yellow-500/10 transition px-2 py-1.5 rounded"
    };
    let bookmark_button_class = if is_bookmarked {
        "flex items-center text-blue-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-blue-500 transition"
    };
    let nav = use_navigator();
    let event_id_nav = event_id.clone();
    let is_hidden = (is_muted.read().unwrap_or(false) || is_author_blocked.read().unwrap_or(false))
        && !*show_hidden_anyway.read();
    rsx! {
        UseGlobalInteraction { event_id: event_id_for_global }
        article {
            "data-event-id": "{event.id}",
            class: "border-b border-border p-4 hover:bg-accent/50 transition-colors cursor-pointer",
            onclick: move |_evt: MouseEvent| {
                #[cfg(feature = "web")]
                {
                    if let Some(target) = _evt.data.as_web_event().target() {
                        if let Some(element) = target.dyn_ref::<web_sys::Element>() {
                            if element.closest(INTERACTIVE_ELEMENT_SELECTOR).ok().flatten().is_some()
                            {
                                return;
                            }
                        }
                    }
                }
                if !is_hidden {
                    nav.push(Route::AddressViewer {
                        address: crate::utils::nip19_urls::note_route_id(&event_id_nav, Some(&author_pubkey)),
                    });
                }
            },
            if is_hidden {
                div { class: "flex items-center gap-3 py-4",
                    div { class: "flex-1 text-muted-foreground text-sm",
                        if is_author_blocked.read().unwrap_or(false) {
                            "Post from blocked user"
                        } else if is_muted.read().unwrap_or(false) {
                            "Muted post"
                        }
                    }
                    button {
                        class: "px-3 py-1 text-sm text-primary hover:underline",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            show_hidden_anyway.set(true);
                        },
                        "Show anyway"
                    }
                }
            } else {
                if let Some((reposter_pubkey_str, reposter_display, repost_time)) = &reposter_display_info {
                    div { class: "flex items-center gap-2 text-sm text-muted-foreground mb-2",
                        Repeat2Icon { class: "w-4 h-4" }
                        Link {
                            to: Route::AddressViewer {
                                address: crate::utils::nip19_urls::profile_route_id(reposter_pubkey_str),
                            },
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            class: "hover:underline font-medium text-muted-foreground",
                            "{reposter_display} reposted"
                        }
                        span { "·" }
                        span { "{repost_time}" }
                    }
                }
                div { class: "flex gap-3",
                    div { class: "shrink-0",
                        Link {
                            to: Route::AddressViewer {
                                address: crate::utils::nip19_urls::profile_route_id(&author_pubkey),
                            },
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            if let Some(picture_url) = &profile_picture {
                                img {
                                    class: "w-12 h-12 rounded-full object-cover",
                                    src: "{picture_url}",
                                    alt: "Profile picture",
                                    loading: "lazy",
                                }
                            } else {
                                div { class: "w-12 h-12 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white font-bold text-lg",
                                    "{display_name.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_else(|| \"?\".to_string())}"
                                }
                            }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "flex items-start justify-between gap-2 mb-1",
                            div { class: "flex items-center gap-2 flex-wrap",
                                Link {
                                    to: Route::AddressViewer {
                                        address: crate::utils::nip19_urls::profile_route_id(&author_pubkey),
                                    },
                                    onclick: move |e: MouseEvent| e.stop_propagation(),
                                    class: "font-bold hover:underline",
                                    "{display_name}"
                                }
                                span { class: "text-muted-foreground text-sm", "@{username}" }
                                span { class: "text-muted-foreground text-sm", "·" }
                                span { class: "text-muted-foreground text-sm", "{timestamp}" }
                                {
                                    if event.is_protected() {
                                        rsx! {
                                            span { class: "text-muted-foreground text-sm", "·" }
                                            span {
                                                class: "inline-flex items-center text-muted-foreground",
                                                title: "Protected — only the author can publish to relays",
                                                LockIcon { class: "w-3.5 h-3.5".to_string() }
                                            }
                                        }
                                    } else {
                                        rsx! {}
                                    }
                                }
                                {
                                    let _v = edit_cache::EDIT_VERSION.read();
                                    let edit_info = edit_cache::get_latest_edit(&event_id);
                                    if let Some(info) = edit_info {
                                        rsx! { EditStatus { edit_info: info, event_id: event_id.clone() } }
                                    } else {
                                        rsx! {}
                                    }
                                }
                                {
                                    if let Some(proxy_info) = nip48::get_proxy_info(&event) {
                                        rsx! {
                                            span { class: "text-muted-foreground text-sm", "·" }
                                            ProxyBadge { proxy_info }
                                        }
                                    } else {
                                        rsx! {}
                                    }
                                }
                            }
                            NoteMenu {
                                author_pubkey: author_pubkey.clone(),
                                event_id: event_id.clone(),
                                event: event.clone(),
                            }
                        }
                        // Topic badge for kind 1111 posts with NIP-73 hashtag
                        {
                            if event.kind == Kind::Comment {
                                if let Some(topic_name) = crate::stores::topic_store::extract_topic_name(&event) {
                                    rsx! {
                                        div { class: "mb-1",
                                            crate::components::topic::TopicBadge { topic: topic_name }
                                        }
                                    }
                                } else {
                                    rsx! {}
                                }
                            } else {
                                rsx! {}
                            }
                        }
                        {
                            let _v = edit_cache::EDIT_VERSION.read();
                            let edit_info = edit_cache::get_latest_edit(&event_id);
                            let display_content = edit_info
                                .as_ref()
                                .map(|e| e.edited_content.clone())
                                .unwrap_or_else(|| content.clone());
                            let content_warning = nip36::get_content_warning(&event.tags);
                            rsx! {
                                div { class: "mb-3",
                                    if let Some(reason) = content_warning {
                                        SensitiveContent { reason,
                                            RichContent {
                                                content: display_content,
                                                tags: event.tags.iter().cloned().collect(),
                                                collapsible,
                                                interactive_media: true,
                                            }
                                        }
                                    } else {
                                        RichContent {
                                            content: display_content,
                                            tags: event.tags.iter().cloned().collect(),
                                            collapsible,
                                            interactive_media: true,
                                        }
                                    }
                                }
                            }
                        }
                        {
                            let external_contents = nip73::extract_external_content(&event);
                            if !external_contents.is_empty() {
                                let contents_for_display: Vec<_> = external_contents
                                    .into_iter()
                                    .map(|(content, hint)| (content, hint.map(|u| u.to_string())))
                                    .collect();
                                rsx! {
                                    ExternalContentList { contents: contents_for_display, compact: true }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                        div { class: "flex items-center justify-between max-w-md mt-2 -ml-2",
                            button {
                                class: "flex items-center gap-1 hover:text-blue-500 hover:bg-blue-500/10 transition px-2 py-1.5 rounded",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_reply_modal.set(true);
                                },
                                MessageCircleIcon {
                                    class: "h-4 w-4".to_string(),
                                    filled: false,
                                }
                                span { class: "text-xs",
                                    {
                                        let count = *reply_count.read();
                                        if count > 500 {
                                            "500+".to_string()
                                        } else if count > 0 {
                                            count.to_string()
                                        } else {
                                            "".to_string()
                                        }
                                    }
                                }
                            }
                            div { class: "relative",
                                button {
                                    class: "{repost_button_class} hover:bg-green-500/10 gap-1 px-2 py-1.5 rounded",
                                    disabled: !has_signer || *is_reposting.read(),
                                    onclick: move |e: MouseEvent| {
                                        e.stop_propagation();
                                        if has_signer && !*is_reposting.read() {
                                            show_repost_menu.toggle();
                                        }
                                    },
                                    Repeat2Icon {
                                        class: "h-4 w-4".to_string(),
                                        filled: false,
                                    }
                                    span { class: "text-xs",
                                        {
                                            let count = *repost_count.read();
                                            if count > 500 {
                                                "500+".to_string()
                                            } else if count > 0 {
                                                count.to_string()
                                            } else {
                                                "".to_string()
                                            }
                                        }
                                    }
                                }
                                if *show_repost_menu.read() {
                                    div {
                                        class: "fixed inset-0 z-40",
                                        onclick: move |e: MouseEvent| {
                                            e.stop_propagation();
                                            show_repost_menu.set(false);
                                        },
                                    }
                                    div {
                                        class: "absolute bottom-full left-0 mb-1 bg-card border border-border rounded-lg shadow-lg py-1 min-w-[120px] z-50",
                                        onclick: move |e: MouseEvent| e.stop_propagation(),
                                        button {
                                            class: "w-full px-3 py-2 text-left hover:bg-accent text-sm flex items-center gap-2",
                                            onclick: move |e: MouseEvent| {
                                                e.stop_propagation();
                                                show_repost_menu.set(false);
                                                if *is_reposted.read() {
                                                    show_undo_repost_confirm.set(true);
                                                } else {
                                                    let event_id_clone = event_id_repost.clone();
                                                    let next_gen = count_request_gen
                                                        .peek()
                                                        .wrapping_add(1);
                                                    count_request_gen.set(next_gen);
                                                    is_reposting.set(true);
                                                    spawn(async move {
                                                        match publish_repost(event_id_clone, None).await {
                                                            Ok(repost_id) => {
                                                                log::info!("Reposted event, repost ID: {}", repost_id);
                                                                is_reposted.set(true);
                                                                user_repost_id.set(Some(repost_id));
                                                                let current_count = *repost_count.peek();
                                                                repost_count.set(current_count + 1);
                                                                is_reposting.set(false);
                                                            }
                                                            Err(e) => {
                                                                log::error!("Failed to repost event: {}", e);
                                                                is_reposting.set(false);
                                                            }
                                                        }
                                                    });
                                                }
                                            },
                                            Repeat2Icon {
                                                class: "h-4 w-4".to_string(),
                                                filled: false,
                                            }
                                            if *is_reposted.read() {
                                                "Undo Repost"
                                            } else {
                                                "Repost"
                                            }
                                        }
                                        button {
                                            class: "w-full px-3 py-2 text-left hover:bg-accent text-sm flex items-center gap-2",
                                            onclick: move |e: MouseEvent| {
                                                e.stop_propagation();
                                                show_repost_menu.set(false);
                                                let nevent = Nip19Event::new(event.id).author(event.pubkey);
                                                match nevent.to_bech32() {
                                                    Ok(nevent_str) => {
                                                        nav.push(Route::NoteNew {
                                                            quote: Some(nevent_str),
                                                        });
                                                    }
                                                    Err(e) => {
                                                        log::warn!("Failed to encode nevent for quote: {}", e);
                                                    }
                                                }
                                            },
                                            svg {
                                                xmlns: "http://www.w3.org/2000/svg",
                                                class: "h-4 w-4",
                                                fill: "none",
                                                view_box: "0 0 24 24",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z",
                                                }
                                            }
                                            "Quote"
                                        }
                                    }
                                }
                            }
                            ReactionButton { reaction: reaction.clone(), has_signer }
                            {
                                let has_lightning = author_metadata
                                    .read()
                                    .as_ref()
                                    .and_then(|m| m.lud16.as_ref().or(m.lud06.as_ref()))
                                    .is_some();
                                if has_lightning {
                                    rsx! {
                                        button {
                                            class: "{zap_button_class}",
                                            onclick: move |e: MouseEvent| {
                                                e.stop_propagation();
                                                show_zap_modal.set(true);
                                            },
                                            ZapIcon { class: "h-4 w-4".to_string(), filled: *is_zapped.read() }
                                            span { class: "text-xs",
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
                                class: "{bookmark_button_class} hover:bg-blue-500/10 px-2 py-1.5 rounded",
                                disabled: !has_signer || *is_bookmarking.read(),
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    if !has_signer || *is_bookmarking.read() {
                                        return;
                                    }
                                    let event_id_clone = event_id_bookmark.clone();
                                    let currently_bookmarked = bookmarks::is_bookmarked(&event_id_clone);
                                    is_bookmarking.set(true);
                                    spawn(async move {
                                        let result = if currently_bookmarked {
                                            bookmarks::unbookmark_event(event_id_clone).await
                                        } else {
                                            bookmarks::bookmark_event(event_id_clone).await
                                        };
                                        match result {
                                            Ok(_) => {
                                                log::info!("Bookmark toggled successfully");
                                            }
                                            Err(e) => {
                                                log::error!("Failed to toggle bookmark: {}", e);
                                            }
                                        }
                                        is_bookmarking.set(false);
                                    });
                                },
                                BookmarkIcon {
                                    class: "h-4 w-4".to_string(),
                                    filled: is_bookmarked,
                                }
                            }

                        }
                    }
                }
            }
        }
        if *show_reply_modal.read() {
            ReplyComposer {
                target: event.clone(),
                root_event: root_event.clone(),
                on_close: move |_| {
                    show_reply_modal.set(false);
                },
                on_success: move |reply_event: NostrEvent| {
                    let next_gen = count_request_gen.peek().wrapping_add(1);
                    count_request_gen.set(next_gen);
                    // Increment reply count for immediate visual feedback
                    let current = *reply_count.read();
                    reply_count.set(current + 1);
                    // Bubble up the reply event for optimistic update
                    if let Some(handler) = on_reply.as_ref() {
                        handler.call(reply_event);
                    }
                    // Close modal LAST to ensure state updates happen first
                    show_reply_modal.set(false);
                },
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
        if *show_undo_repost_confirm.read() {
            ConfirmModal {
                title: "Delete Repost?".to_string(),
                message: "Are you sure you want to remove this repost?".to_string(),
                confirm_text: Some("Delete".to_string()),
                cancel_text: Some("Cancel".to_string()),
                on_cancel: move |_| show_undo_repost_confirm.set(false),
                on_confirm: move |_| {
                    show_undo_repost_confirm.set(false);
                    if let Some(repost_id) = user_repost_id.read().clone() {
                        let next_gen = count_request_gen.peek().wrapping_add(1);
                        count_request_gen.set(next_gen);
                        is_reposting.set(true);
                        spawn(async move {
                            match delete_repost(repost_id).await {
                                Ok(()) => {
                                    log::info!("Repost deleted successfully");
                                    is_reposted.set(false);
                                    let current_count = *repost_count.peek();
                                    repost_count.set(current_count.saturating_sub(1));
                                    user_repost_id.set(None);
                                    is_reposting.set(false);
                                }
                                Err(e) => {
                                    log::error!("Failed to delete repost: {}", e);
                                    is_reposting.set(false);
                                }
                            }
                        });
                    } else {
                        log::warn!("Undo repost triggered but no repost ID available");
                    }
                },
            }
        }
    }
}
/// Render protocol icon for NIP-48 proxy badges
fn ProtocolIcon(protocol: &Protocol) -> Element {
    match protocol {
        Protocol::ActivityPub => {
            rsx! {
                MastodonIcon { class: "w-3.5 h-3.5" }
            }
        }
        Protocol::ATProto => {
            rsx! {
                BlueskyIcon { class: "w-3.5 h-3.5" }
            }
        }
        Protocol::Rss => {
            rsx! {
                RssIcon { class: "w-3.5 h-3.5" }
            }
        }
        Protocol::Web => {
            rsx! {
                GlobeIcon { class: "w-3.5 h-3.5" }
            }
        }
        Protocol::Custom(_) => {
            rsx! {
                ExternalLinkIcon { class: "w-3.5 h-3.5" }
            }
        }
    }
}
/// NIP-48 Proxy Badge - shows origin for bridged content
/// Displays a small icon linking to the original source on another protocol
#[component]
fn ProxyBadge(proxy_info: nip48::ProxyInfo) -> Element {
    let display_name = proxy_info.display_name();
    let source_url = proxy_info.id.clone();
    if !is_valid_http_url(&source_url) {
        return rsx! {
            span {
                class: "inline-flex items-center text-muted-foreground",
                title: "Bridged from {display_name}",
                {ProtocolIcon(&proxy_info.protocol)}
            }
        };
    }
    rsx! {
        a {
            href: "{source_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center text-muted-foreground hover:text-foreground transition-colors",
            title: "View original on {display_name}",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            {ProtocolIcon(&proxy_info.protocol)}
        }
    }
}
#[component]
pub fn NoteCardSkeleton() -> Element {
    rsx! {
        div { class: "border-b border-gray-200 dark:border-gray-800 p-4 animate-pulse",
            div { class: "flex gap-3",
                div { class: "w-12 h-12 rounded-full bg-gray-300 dark:bg-gray-700" }
                div { class: "flex-1 space-y-2",
                    div { class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-1/4" }
                    div { class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-3/4" }
                    div { class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-1/2" }
                }
            }
        }
    }
}
