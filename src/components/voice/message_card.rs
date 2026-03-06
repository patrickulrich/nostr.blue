use crate::components::icons::{MessageCircleIcon, Repeat2Icon, ZapIcon};
use super::reply_composer::VoiceReplyComposer;
use crate::components::{ReactionButton, ZapModal};
use crate::hooks::use_reaction;
use crate::routes::Route;
use crate::stores::nostr_client::get_client;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::signer::SIGNER_INFO;
use crate::stores::{nostr_client, voice_messages_store};
use crate::services::aggregation::InteractionCounts;
use crate::utils::truncate_pubkey;
use dioxus::events::{MediaData, MouseData};
use dioxus::prelude::*;
#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
use nostr_sdk::{Event as NostrEvent, EventId, Filter, Kind, PublicKey};
use std::time::Duration;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[component]
pub fn VoiceMessageCard(
    event: NostrEvent,
    #[props(default = None)]
    precomputed_counts: Option<InteractionCounts>,
) -> Element {
    let author_pubkey = event.pubkey.to_string();
    let audio_url = event.content.clone();
    let created_at = event.created_at;
    let event_id = event.id;
    let event_id_str = event_id.to_string();
    let event_clone = event.clone();
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    #[allow(unused_mut)]
    let mut duration = use_signal(|| 0.0);
    let mut current_time = use_signal(|| 0.0);
    #[allow(unused_variables, unused_mut)]
    let mut is_loading = use_signal(|| true);
    let mut show_reply_modal = use_signal(|| false);
    let mut show_zap_modal = use_signal(|| false);
    let mut is_reposting = use_signal(|| false);
    let has_signer = *HAS_SIGNER.read();
    let reaction = use_reaction(event_id_str.clone(), author_pubkey.clone(), precomputed_counts.as_ref());
    let mut reply_count = use_signal(|| 0usize);
    let mut repost_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);
    let mut is_reposted = use_signal(|| false);
    let mut is_zapped = use_signal(|| false);
    let audio_id = format!("voice-audio-{}", event_id_str);
    let imeta_duration = event
        .tags
        .iter()
        .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("imeta"))
        .and_then(|tag| {
            let fields = tag.as_slice();
            if fields.len() < 2 {
                return None;
            }
            fields
                .iter()
                .skip(1)
                .find_map(|field| {
                    let field_str = field.as_str();
                    if field_str.starts_with("duration ") {
                        field_str
                            .strip_prefix("duration ")
                            .and_then(|d| d.parse::<f64>().ok())
                    } else if field_str.starts_with("duration=") {
                        field_str
                            .strip_prefix("duration=")
                            .and_then(|d| d.parse::<f64>().ok())
                    } else {
                        None
                    }
                })
        });
    use_effect(
        use_reactive(
            (&author_pubkey, &*nostr_client::CLIENT_INITIALIZED.read()),
            move |(pubkey, client_ready)| {
                if !client_ready {
                    return;
                }
                spawn(async move {
                    match PublicKey::parse(&pubkey) {
                        Ok(pk) => {
                            if let Some(client) = nostr_client::get_client() {
                                if let Ok(Some(metadata)) = client
                                    .fetch_metadata(pk, Duration::from_secs(5))
                                    .await
                                {
                                    author_metadata.set(Some(metadata));
                                }
                            }
                        }
                        Err(e) => {
                            log::error!(
                                "Failed to parse author_pubkey '{}': {}", pubkey, e
                            );
                        }
                    }
                });
            },
        ),
    );
    use_effect(
        use_reactive(
            &precomputed_counts,
            move |counts_opt| {
                if let Some(counts) = counts_opt {
                    reply_count.set(counts.replies);
                    repost_count.set(counts.reposts);
                    zap_amount_sats.set(counts.zap_amount_sats);
                    is_reposted.set(counts.user_reposted.unwrap_or(false));
                    is_zapped.set(counts.user_zapped.unwrap_or(false));
                }
            },
        ),
    );
    let has_precomputed = precomputed_counts.is_some();
    use_effect(
        use_reactive(
            &(event_id_str.clone(), has_precomputed),
            move |(event_id_for_counts, has_precomputed)| {
                if has_precomputed {
                    return;
                }
                spawn(async move {
                    let client = match get_client() {
                        Some(c) => c,
                        None => return,
                    };
                    let event_id_parsed = match EventId::from_hex(&event_id_for_counts) {
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
                        let current_user_pubkey = SIGNER_INFO
                            .read()
                            .as_ref()
                            .map(|info| info.public_key.clone());
                        let mut user_has_zapped = false;
                        let total_sats: u64 = zaps
                            .iter()
                            .filter_map(|zap_event| {
                                if let Some(ref user_pk) = current_user_pubkey {
                                    let zap_sender_pubkey = zap_event
                                        .tags
                                        .iter()
                                        .find_map(|tag| {
                                            let tag_vec = tag.clone().to_vec();
                                            if tag_vec.len() >= 2 && tag_vec.first()?.as_str() == "P" {
                                                Some(tag_vec.get(1)?.as_str().to_string())
                                            } else {
                                                None
                                            }
                                        });
                                    if let Some(zap_sender) = zap_sender_pubkey {
                                        if zap_sender == *user_pk {
                                            user_has_zapped = true;
                                        }
                                    }
                                }
                                zap_event
                                    .tags
                                    .iter()
                                    .find_map(|tag| {
                                        let tag_vec = tag.clone().to_vec();
                                        if tag_vec.first()?.as_str() == "description" {
                                            let zap_request_json = tag_vec.get(1)?.as_str();
                                            if let Ok(zap_request) = serde_json::from_str::<
                                                serde_json::Value,
                                            >(zap_request_json) {
                                                if let Some(tags) = zap_request
                                                    .get("tags")
                                                    .and_then(|t| t.as_array())
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
                                                                    if let Ok(millisats) = amount_str.parse::<u64>() {
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
        ),
    );
    let toggle_play = move |e: MouseEvent| {
        e.stop_propagation();
        voice_messages_store::toggle_voice_message(event_id);
    };
    let audio_id_for_effect = audio_id.clone();
    use_effect(move || {
        let global_state = voice_messages_store::VOICE_PLAYBACK.read();
        let _is_playing = global_state.currently_playing == Some(event_id);
        let _audio_id_clone = audio_id_for_effect.clone();
        #[cfg(feature = "web")]
        {
            let is_playing = _is_playing;
            let audio_id_clone = _audio_id_clone;
            let window = match web_sys::window() {
                Some(w) => w,
                None => {
                    log::error!("Failed to get window object");
                    return;
                }
            };
            let document = match window.document() {
                Some(d) => d,
                None => {
                    log::error!("Failed to get document object");
                    return;
                }
            };
            let element = match document.get_element_by_id(&audio_id_clone) {
                Some(e) => e,
                None => {
                    log::debug!("Audio element {} not found yet", audio_id_clone);
                    return;
                }
            };
            let audio: web_sys::HtmlAudioElement = match element.dyn_into() {
                Ok(a) => a,
                Err(e) => {
                    log::error!("Element is not an HtmlAudioElement: {:?}", e);
                    return;
                }
            };
            if is_playing {
                let _ = audio
                    .play()
                    .map_err(|e| {
                        log::debug!("Play failed: {:?}", e);
                    });
            } else if let Err(e) = audio.pause() {
                log::debug!("Pause failed: {:?}", e);
            }
        }
    });
    #[cfg(feature = "web")]
    let _handle_timeupdate = move |_evt: Event<MediaData>| {
        if let Some(target) = _evt.data.as_web_event().target() {
            if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                let time = audio.current_time();
                if !time.is_nan() {
                    current_time.set(time);
                    if voice_messages_store::is_playing(&event_id) {
                        voice_messages_store::set_current_time(time);
                    }
                }
            }
        }
    };
    #[cfg(not(feature = "web"))]
    let handle_timeupdate = move |_: Event<MediaData>| {};
    #[cfg(feature = "web")]
    let _handle_loadedmetadata = move |_evt: Event<MediaData>| {
        if let Some(target) = _evt.data.as_web_event().target() {
            if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                let dur = audio.duration();
                if !dur.is_nan() {
                    duration.set(dur);
                    if voice_messages_store::is_playing(&event_id) {
                        voice_messages_store::set_duration(dur);
                    }
                }
                is_loading.set(false);
            }
        }
    };
    #[cfg(not(feature = "web"))]
    let handle_loadedmetadata = move |_: Event<MediaData>| {};
    let _handle_ended = move |_: Event<MediaData>| {
        voice_messages_store::pause_voice_message();
        current_time.set(0.0);
    };
    let event_id_for_repost = event_id_str.clone();
    let handle_repost = move |e: MouseEvent| {
        e.stop_propagation();
        let event_id_copy = event_id_for_repost.clone();
        is_reposting.set(true);
        spawn(async move {
            match nostr_client::publish_repost(event_id_copy, None).await {
                Ok(_) => {
                    log::info!("Repost published successfully");
                    is_reposted.set(true);
                    let current_count = *repost_count.read();
                    repost_count.set(current_count + 1);
                }
                Err(e) => log::error!("Failed to publish repost: {}", e),
            }
            is_reposting.set(false);
        });
    };
    let current_time_str = voice_messages_store::format_time(*current_time.read());
    let duration_val = imeta_duration.unwrap_or(*duration.read());
    let duration_str = voice_messages_store::format_time(duration_val);
    let progress_percent = if duration_val > 0.0 {
        *current_time.read() / duration_val * 100.0
    } else {
        0.0
    };
    let author_name = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.display_name.clone().or_else(|| m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));
    let author_username = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.name.clone())
        .unwrap_or_default();
    let author_avatar = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone())
        .unwrap_or_default();
    let time_ago = {
        let now = crate::platform::timestamp::now_secs() as f64;
        let diff = now - created_at.as_secs() as f64;
        if diff < 60.0 {
            format!("{}s", diff as u32)
        } else if diff < 3600.0 {
            format!("{}m", (diff / 60.0) as u32)
        } else if diff < 86400.0 {
            format!("{}h", (diff / 3600.0) as u32)
        } else {
            format!("{}d", (diff / 86400.0) as u32)
        }
    };
    let navigator: Option<Box<dyn Fn()>> = {
        #[cfg(feature = "web")]
        {
            use crate::routes::Route;
            let nav = use_navigator();
            let voice_id = event_id_str.clone();
            Some(Box::new(move || {
                let _ = nav.push(Route::Note {
                    note_id: voice_id.clone(),
                    from_voice: Some("true".to_string()),
                });
            }))
        }
        #[cfg(not(feature = "web"))]
        {
            None
        }
    };
    #[cfg(feature = "web")]
    let is_clickable = true;
    #[cfg(not(feature = "web"))]
    let is_clickable = false;
    let card_class = if is_clickable {
        "p-4 hover:bg-accent/50 transition cursor-pointer border-b border-border"
    } else {
        "p-4 border-b border-border opacity-60"
    };
    let tooltip_text = if !is_clickable { "Not supported on this platform" } else { "" };
    let handle_click = move |_evt: Event<MouseData>| {
        if is_clickable {
            if let Some(nav) = &navigator {
                nav();
            }
        }
    };
    rsx! {
        div {
            class: "{card_class}",
            title: "{tooltip_text}",
            onclick: handle_click,
            div { class: "flex items-start gap-3 mb-3",
                Link {
                    to: Route::Profile {
                        pubkey: author_pubkey.clone(),
                    },
                    class: "shrink-0",
                    onclick: |e: MouseEvent| e.stop_propagation(),
                    if !author_avatar.is_empty() {
                        img {
                            src: "{author_avatar}",
                            alt: "Avatar",
                            class: "w-12 h-12 rounded-full object-cover bg-muted",
                        }
                    } else {
                        div { class: "w-12 h-12 rounded-full bg-gradient-to-br from-primary to-secondary flex items-center justify-center text-primary-foreground font-bold text-lg",
                            {author_name.chars().next().unwrap_or('?').to_string().to_uppercase()}
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    Link {
                        to: Route::Profile {
                            pubkey: author_pubkey.clone(),
                        },
                        class: "hover:underline",
                        onclick: |e: MouseEvent| e.stop_propagation(),
                        div { class: "flex items-center gap-2",
                            span { class: "font-semibold text-foreground truncate", "{author_name}" }
                            if !author_username.is_empty() && author_username != author_name {
                                span { class: "text-muted-foreground text-sm truncate",
                                    "@{author_username}"
                                }
                            }
                            span { class: "text-muted-foreground text-sm shrink-0", "· {time_ago}" }
                        }
                    }
                }
            }
            div { class: "mb-3",
                audio {
                    id: "{audio_id}",
                    src: "{audio_url}",
                    preload: "metadata",
                    style: "display: none;",
                    ontimeupdate: move |_| {},
                    onloadedmetadata: move |_| {},
                    onended: move |_| {},
                }
                if cfg!(feature = "web") {
                    div { class: "flex items-center gap-4 bg-muted/30 rounded-lg p-3",
                        button {
                            class: "shrink-0 w-10 h-10 rounded-full bg-primary text-primary-foreground hover:bg-primary/90 transition flex items-center justify-center",
                            onclick: toggle_play,
                            if voice_messages_store::VOICE_PLAYBACK.read().currently_playing == Some(event_id) {
                                svg {
                                    class: "w-5 h-5",
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
                                    class: "w-5 h-5 ml-0.5",
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
                } else {
                    div { class: "flex items-center gap-4 bg-muted/30 rounded-lg p-3",
                        div { class: "shrink-0 w-10 h-10 rounded-full bg-muted flex items-center justify-center",
                            span { class: "text-muted-foreground text-xs", "N/A" }
                        }
                        div { class: "flex-1 text-sm text-muted-foreground",
                            "Playback not supported on this platform"
                        }
                    }
                }
            }
            div { class: "flex items-center justify-between text-muted-foreground",
                button {
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        show_reply_modal.set(true);
                    },
                    MessageCircleIcon { class: "w-4 h-4 group-hover:scale-110 transition" }
                    if *reply_count.read() > 0 {
                        span { class: "text-sm", "{reply_count.read()}" }
                    }
                }
                button {
                    class: if *is_reposted.read() { "flex items-center gap-1 text-green-500 transition group" } else { "flex items-center gap-1 hover:text-green-500 transition group" },
                    onclick: handle_repost,
                    disabled: *is_reposting.read() || *is_reposted.read(),
                    Repeat2Icon { class: "w-4 h-4 group-hover:scale-110 transition" }
                    if *repost_count.read() > 0 {
                        span { class: "text-sm", "{repost_count.read()}" }
                    }
                }
                ReactionButton {
                    reaction: reaction.clone(),
                    has_signer,
                    icon_class: "w-4 h-4".to_string(),
                    count_class: "text-sm".to_string(),
                }
                button {
                    class: if *is_zapped.read() { "flex items-center gap-1 text-yellow-500 transition group" } else { "flex items-center gap-1 hover:text-yellow-500 transition group" },
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        show_zap_modal.set(true);
                    },
                    ZapIcon { class: "w-4 h-4 group-hover:scale-110 transition" }
                    if *zap_amount_sats.read() > 0 {
                        span { class: "text-sm", "{zap_amount_sats.read()}" }
                    }
                }
            }
            if *show_reply_modal.read() {
                VoiceReplyComposer {
                    reply_to: event_clone.clone(),
                    on_close: move |_| show_reply_modal.set(false),
                    on_success: move |_| {
                        show_reply_modal.set(false);
                    },
                }
            }
            if *show_zap_modal.read() {
                ZapModal {
                    recipient_pubkey: author_pubkey.clone(),
                    recipient_name: author_name.clone(),
                    lud16: author_metadata.read().as_ref().and_then(|m| m.lud16.clone()),
                    lud06: author_metadata.read().as_ref().and_then(|m| m.lud06.clone()),
                    event_id: Some(event_id_str.clone()),
                    on_close: move |_| show_zap_modal.set(false),
                }
            }
        }
    }
}
