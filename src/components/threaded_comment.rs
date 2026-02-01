use crate::components::icons::{
    BookmarkIcon, MessageCircleIcon, Repeat2Icon, ShareIcon, ZapIcon,
};
use crate::components::{ReactionButton, ReplyComposer, RichContent, ZapModal};
use crate::hooks::{use_author_metadata, use_reaction};
use crate::routes::Route;
use crate::stores::bookmarks;
use crate::stores::nostr_client::{get_client, publish_repost, HAS_SIGNER};
use crate::stores::signer::SIGNER_INFO;
use crate::stores::voice_messages_store;
use crate::utils::format_sats_compact;
use crate::utils::time::format_relative_time_ex;
use crate::utils::{event::is_voice_message, ThreadNode};
use dioxus::events::MediaData;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use nostr_sdk::{Event as NostrEvent, Filter, Kind};
use std::time::Duration;
use wasm_bindgen::JsCast;

const MAX_DEPTH: usize = 8;

#[component]
pub fn ThreadedComment(
    node: ThreadNode,
    depth: usize,
    #[props(default)] on_reply: Option<EventHandler<NostrEvent>>,
) -> Element {
    let event = &node.event;
    let children = &node.children;
    let author_pubkey = event.pubkey;

    let event_id = event.id.to_string();
    let event_id_like = event_id.clone();
    let event_id_repost = event_id.clone();
    let event_id_bookmark = event_id.clone();
    let event_id_memo = event_id.clone();
    let event_id_counts = event_id.clone();

    let author_pubkey_str = author_pubkey.to_string();
    let author_pubkey_like = author_pubkey_str.clone();
    let author_metadata = use_author_metadata(author_pubkey_str.clone());

    let mut is_reposting = use_signal(|| false);
    let mut is_reposted = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    let is_bookmarked = use_memo(move || bookmarks::is_bookmarked(&event_id_memo));

    let has_signer = *HAS_SIGNER.read();
    let mut show_reply_modal = use_signal(|| false);
    let mut show_zap_modal = use_signal(|| false);

    let reaction = use_reaction(event_id_like.clone(), author_pubkey_like.clone(), None);
    let mut reply_count = use_signal(|| 0usize);
    let mut repost_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);

    let is_voice = is_voice_message(event);
    let audio_url = if is_voice {
        event.content.clone()
    } else {
        String::new()
    };
    let audio_id = format!("voice-comment-{}", event_id);
    let mut duration = use_signal(|| 0.0f64);
    let mut current_time = use_signal(|| 0.0f64);
    let event_id_parsed = event.id;

    let imeta_duration: Option<f64> = if is_voice {
        event
            .tags
            .iter()
            .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("imeta"))
            .and_then(|tag| {
                for field in tag.as_slice().iter().skip(1) {
                    let field_str = field.as_str();
                    if let Some(val) = field_str.strip_prefix("duration ") {
                        if let Ok(d) = val.parse::<f64>() {
                            return Some(d);
                        }
                    }
                }
                None
            })
    } else {
        None
    };

    use_effect(move || {
        let event_id_for_counts = event_id_counts.clone();
        spawn(async move {
            let client = match get_client() {
                Some(c) => c,
                None => return,
            };
            let event_id_parsed =
                match nostr_sdk::EventId::from_hex(&event_id_for_counts) {
                    Ok(id) => id,
                    Err(_) => return,
                };
            let reply_filter = Filter::new()
                .kind(Kind::TextNote)
                .event(event_id_parsed)
                .limit(500);
            if let Ok(replies) = client
                .fetch_events(reply_filter, Duration::from_secs(5))
                .await
            {
                reply_count.set(replies.len());
            }
            let repost_filter = Filter::new()
                .kind(Kind::Repost)
                .event(event_id_parsed)
                .limit(500);
            if let Ok(reposts) = client
                .fetch_events(repost_filter, Duration::from_secs(5))
                .await
            {
                let current_user_pubkey = SIGNER_INFO
                    .read()
                    .as_ref()
                    .map(|info| info.public_key.clone());
                let mut user_has_reposted = false;
                if let Some(ref user_pk) = current_user_pubkey {
                    for repost in reposts.iter() {
                        if repost.pubkey.to_string() == *user_pk {
                            user_has_reposted = true;
                            break;
                        }
                    }
                }
                repost_count.set(reposts.len());
                is_reposted.set(user_has_reposted);
            }
            let zap_filter = Filter::new()
                .kind(Kind::ZapReceipt)
                .event(event_id_parsed)
                .limit(500);
            if let Ok(zaps) = client
                .fetch_events(zap_filter, Duration::from_secs(5))
                .await
            {
                let total_sats: u64 = zaps
                    .iter()
                    .filter_map(|zap_event| {
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
            }
        });
    });

    let repost_button_class = if *is_reposted.read() {
        "flex items-center text-green-500 hover:text-green-600 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-green-500 transition"
    };
    let bookmark_button_class = if *is_bookmarked.read() {
        "flex items-center text-blue-500 hover:text-blue-600 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-blue-500 transition"
    };

    let audio_id_for_effect = audio_id.clone();
    let audio_id_for_handlers = audio_id.clone();

    use_effect(move || {
        if !is_voice {
            return;
        }
        let global_state = voice_messages_store::VOICE_PLAYBACK.read();
        let is_playing = global_state.currently_playing == Some(event_id_parsed);
        let audio_id_clone = audio_id_for_effect.clone();
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let document = match window.document() {
            Some(d) => d,
            None => return,
        };
        let audio_element = match document.get_element_by_id(&audio_id_clone) {
            Some(el) => el,
            None => return,
        };
        let audio = match audio_element.dyn_ref::<web_sys::HtmlAudioElement>() {
            Some(a) => a,
            None => return,
        };
        if is_playing {
            let _ = audio.play().map_err(|e| {
                log::debug!("Play failed: {:?}", e);
            });
        } else if let Err(e) = audio.pause() {
            log::debug!("Pause failed: {:?}", e);
        }
    });

    let handle_timeupdate = move |evt: Event<MediaData>| {
        if let Some(target) = evt.data.as_web_event().target() {
            if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                let time = audio.current_time();
                current_time.set(time);
                if voice_messages_store::is_playing(&event_id_parsed) {
                    voice_messages_store::set_current_time(time);
                }
            }
        }
    };

    let handle_loadedmetadata = move |evt: Event<MediaData>| {
        if let Some(target) = evt.data.as_web_event().target() {
            if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                let dur = audio.duration();
                if !dur.is_nan() {
                    duration.set(dur);
                    if voice_messages_store::is_playing(&event_id_parsed) {
                        voice_messages_store::set_duration(dur);
                    }
                }
            }
        }
    };

    let handle_ended = move |_| {
        voice_messages_store::pause_voice_message();
        current_time.set(0.0);
    };

    let toggle_play = move |_| {
        voice_messages_store::toggle_voice_message(event_id_parsed);
    };

    let duration_val = imeta_duration.unwrap_or(*duration.read());
    let current_time_str = voice_messages_store::format_time(*current_time.read());
    let duration_str = voice_messages_store::format_time(duration_val);
    let progress_percent = if duration_val > 0.0 {
        *current_time.read() / duration_val * 100.0
    } else {
        0.0
    };

    let indent_level = depth.min(MAX_DEPTH);
    let margin_left = indent_level * 4;

    let event_id_nav = event.id.to_hex();
    let nav = use_navigator();

    rsx! {
        div { class: "comment-thread", style: "margin-left: {margin_left}px;",
            div {
                class: "border-l-2 border-border pl-3 py-2 hover:bg-accent/20 transition cursor-pointer",
                onclick: {
                    let event_id_click = event_id_nav.clone();
                    let navigator = nav;
                    move |_| {
                        navigator.push(Route::Note {
                            note_id: event_id_click.clone(),
                            from_voice: None,
                        });
                    }
                },
                div { class: "flex items-start gap-2 mb-2",
                    Link {
                        to: Route::Profile {
                            pubkey: author_pubkey.to_string(),
                        },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        if let Some(metadata) = author_metadata.read().as_ref() {
                            if let Some(picture) = &metadata.picture {
                                img {
                                    class: "w-8 h-8 rounded-full shrink-0",
                                    src: "{picture}",
                                    alt: "Avatar",
                                    loading: "lazy",
                                }
                            } else {
                                div { class: "w-8 h-8 rounded-full bg-blue-500 flex items-center justify-center text-white text-xs font-bold shrink-0",
                                    if let Some(name) = &metadata.name {
                                        "{name.chars().next().unwrap_or('?').to_uppercase()}"
                                    } else {
                                        "?"
                                    }
                                }
                            }
                        } else {
                            div { class: "w-8 h-8 rounded-full bg-gray-400 flex items-center justify-center text-white text-xs shrink-0",
                                "?"
                            }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "flex items-baseline gap-2 flex-wrap",
                            Link {
                                to: Route::Profile {
                                    pubkey: author_pubkey.to_string(),
                                },
                                class: "font-semibold text-sm hover:underline truncate",
                                onclick: move |e: MouseEvent| e.stop_propagation(),
                                if let Some(metadata) = author_metadata.read().as_ref() {
                                    if let Some(display_name) = &metadata.display_name {
                                        "{display_name}"
                                    } else if let Some(name) = &metadata.name {
                                        "{name}"
                                    } else {
                                        span { class: "font-mono text-xs",
                                            "{author_pubkey.to_string().chars().take(16).collect::<String>()}..."
                                        }
                                    }
                                } else {
                                    span { class: "font-mono text-xs",
                                        "{author_pubkey.to_string().chars().take(16).collect::<String>()}..."
                                    }
                                }
                            }
                            span { class: "text-xs text-muted-foreground",
                                "{format_relative_time_ex(event.created_at, true, true)}"
                            }
                        }
                        div { class: "text-sm mt-1",
                            if is_voice {
                                div { class: "my-2",
                                    audio {
                                        id: "{audio_id_for_handlers}",
                                        src: "{audio_url}",
                                        preload: "metadata",
                                        style: "display: none;",
                                        ontimeupdate: handle_timeupdate,
                                        onloadedmetadata: handle_loadedmetadata,
                                        onended: handle_ended,
                                        onerror: move |_| {
                                            log::warn!("Failed to load voice message audio");
                                        },
                                    }
                                    div {
                                        class: "flex items-center gap-3 bg-muted/30 rounded-lg p-2",
                                        onclick: move |e: MouseEvent| e.stop_propagation(),
                                        button {
                                            class: "shrink-0 w-8 h-8 rounded-full bg-primary text-primary-foreground hover:bg-primary/90 transition flex items-center justify-center",
                                            onclick: toggle_play,
                                            if voice_messages_store::VOICE_PLAYBACK.read().currently_playing
                                                == Some(event_id_parsed)
                                            {
                                                svg {
                                                    class: "w-4 h-4",
                                                    view_box: "0 0 24 24",
                                                    fill: "currentColor",
                                                    rect {
                                                        x: "6",
                                                        y: "4",
                                                        width: "4",
                                                        height: "16",
                                                    }
                                                    rect {
                                                        x: "14",
                                                        y: "4",
                                                        width: "4",
                                                        height: "16",
                                                    }
                                                }
                                            } else {
                                                svg {
                                                    class: "w-4 h-4 ml-0.5",
                                                    view_box: "0 0 24 24",
                                                    fill: "currentColor",
                                                    polygon { points: "8,5 19,12 8,19" }
                                                }
                                            }
                                        }
                                        div { class: "flex-1",
                                            div { class: "w-full h-1 bg-muted rounded-full overflow-hidden mb-1",
                                                div {
                                                    class: "h-full bg-primary transition-all",
                                                    style: "width: {progress_percent}%",
                                                }
                                            }
                                            div { class: "flex justify-between text-xs text-muted-foreground",
                                                span { "{current_time_str}" }
                                                span { "{duration_str}" }
                                            }
                                        }
                                    }
                                }
                            } else {
                                RichContent {
                                    content: event.content.clone(),
                                    tags: event.tags.clone().to_vec(),
                                }
                            }
                        }
                        div { class: "flex items-center justify-between max-w-md mt-2 -ml-2",
                            button {
                                class: "flex items-center gap-1 hover:text-blue-500 hover:bg-blue-500/10 transition px-2 py-1.5 rounded",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_reply_modal.set(true);
                                },
                                MessageCircleIcon { class: "h-4 w-4".to_string(), filled: false }
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
                            button {
                                class: "{repost_button_class} hover:bg-green-500/10 gap-1 px-2 py-1.5 rounded",
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
                                                log::info!("Reposted event, repost ID: {}", repost_id);
                                                is_reposted.set(true);
                                                let current_count = *repost_count.read();
                                                repost_count.set(current_count.saturating_add(1));
                                                is_reposting.set(false);
                                            }
                                            Err(e) => {
                                                log::error!("Failed to repost event: {}", e);
                                                is_reposting.set(false);
                                            }
                                        }
                                    });
                                },
                                Repeat2Icon { class: "h-4 w-4".to_string(), filled: false }
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
                            ReactionButton {
                                reaction: reaction.clone(),
                                has_signer,
                                icon_class: "h-4 w-4".to_string(),
                                count_class: "text-xs".to_string(),
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
                                            class: "flex items-center gap-1 text-muted-foreground hover:text-yellow-500 hover:bg-yellow-500/10 transition px-2 py-1.5 rounded",
                                            onclick: move |e: MouseEvent| {
                                                e.stop_propagation();
                                                show_zap_modal.set(true);
                                            },
                                            ZapIcon { class: "h-4 w-4".to_string(), filled: false }
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
                                class: "{bookmark_button_class} hover:bg-blue-500/10 gap-1 px-2 py-1.5 rounded",
                                disabled: *is_bookmarking.read(),
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    if *is_bookmarking.read() {
                                        return;
                                    }
                                    let event_id_clone = event_id_bookmark.clone();
                                    is_bookmarking.set(true);
                                    spawn(async move {
                                        let currently_bookmarked = bookmarks::is_bookmarked(&event_id_clone);
                                        if currently_bookmarked {
                                            if let Err(e) = bookmarks::unbookmark_event(event_id_clone).await {
                                                log::error!("Failed to unbookmark: {}", e);
                                            }
                                        } else if let Err(e) = bookmarks::bookmark_event(event_id_clone).await {
                                            log::error!("Failed to bookmark: {}", e);
                                        }
                                        is_bookmarking.set(false);
                                    });
                                },
                                BookmarkIcon {
                                    class: "h-4 w-4".to_string(),
                                    filled: *is_bookmarked.read(),
                                }
                            }
                            button {
                                class: "flex items-center gap-1 text-muted-foreground hover:text-blue-500 hover:bg-blue-500/10 transition px-2 py-1.5 rounded",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    log::info!("Share button clicked for event");
                                },
                                ShareIcon { class: "h-4 w-4".to_string(), filled: false }
                            }
                        }
                    }
                }
            }
            if !children.is_empty() && depth < MAX_DEPTH {
                div { class: "space-y-1 mt-1",
                    for child in children {
                        ThreadedComment {
                            key: "{child.event.id}",
                            node: child.clone(),
                            depth: depth + 1,
                            on_reply,
                        }
                    }
                }
            } else if !children.is_empty() && depth >= MAX_DEPTH {
                div { class: "ml-4 mt-2",
                    Link {
                        to: Route::Note {
                            note_id: event.id.to_hex(),
                            from_voice: None,
                        },
                        class: "text-xs text-blue-500 hover:underline",
                        "→ Continue thread ({children.len()} more replies)"
                    }
                }
            }
        }
        if *show_reply_modal.read() {
            ReplyComposer {
                reply_to: event.clone(),
                on_close: move |_| {
                    show_reply_modal.set(false);
                },
                on_success: move |reply_event: NostrEvent| {
                    show_reply_modal.set(false);
                    let current = *reply_count.read();
                    reply_count.set(current + 1);
                    // Bubble up the reply event for optimistic update
                    if let Some(handler) = on_reply.as_ref() {
                        handler.call(reply_event);
                    }
                },
            }
        }
        if *show_zap_modal.read() {
            {
                let zap_display_name = author_metadata
                    .read()
                    .as_ref()
                    .and_then(|m| m.display_name.clone().or(m.name.clone()))
                    .unwrap_or_else(|| {
                        author_pubkey.to_string().chars().take(16).collect::<String>() + "..."
                    });
                rsx! {
                    ZapModal {
                        recipient_pubkey: author_pubkey_str.clone(),
                        recipient_name: zap_display_name,
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
    }
}
