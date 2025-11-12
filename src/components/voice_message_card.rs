use dioxus::prelude::*;
use dioxus::events::MediaData;
use dioxus::web::WebEventExt;
use nostr_sdk::{Event as NostrEvent, PublicKey, Filter, Kind, EventId};
use crate::routes::Route;
use crate::stores::{nostr_client, voice_messages_store};
use crate::stores::nostr_client::get_client;
use crate::stores::signer::SIGNER_INFO;
use crate::components::{ZapModal, VoiceReplyComposer};
use crate::components::icons::{HeartIcon, MessageCircleIcon, Repeat2Icon, ZapIcon};
use wasm_bindgen::JsCast;
use std::time::Duration;
use js_sys;

#[component]
pub fn VoiceMessageCard(event: NostrEvent) -> Element {
    // Clone values for closures
    let author_pubkey = event.pubkey.to_string();
    let audio_url = event.content.clone();
    let created_at = event.created_at;
    let event_id = event.id;
    let event_id_str = event_id.to_string();
    let event_clone = event.clone();

    // State
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut duration = use_signal(|| 0.0);
    let mut current_time = use_signal(|| 0.0);
    let mut is_loading = use_signal(|| true);
    let mut show_reply_modal = use_signal(|| false);
    let mut show_zap_modal = use_signal(|| false);
    let mut is_liking = use_signal(|| false);
    let mut is_reposting = use_signal(|| false);

    // Reaction counts and states
    let mut reply_count = use_signal(|| 0usize);
    let mut like_count = use_signal(|| 0usize);
    let mut repost_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);
    let mut is_liked = use_signal(|| false);
    let mut is_reposted = use_signal(|| false);
    let mut is_zapped = use_signal(|| false);

    // Audio element ID
    let audio_id = format!("voice-audio-{}", event_id_str);

    // Parse imeta tags for duration per NIP-92/NIP-94
    // Expected format: ["imeta", "url <value>", "m <mime-type>", "duration <seconds>", ...]
    // Each field is a key-value pair separated by space or follows "key value" format
    let imeta_duration = event.tags.iter()
        .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("imeta"))
        .and_then(|tag| {
            // Validate and parse imeta tag structure
            let fields = tag.as_slice();
            if fields.len() < 2 {
                return None; // Invalid imeta tag structure
            }

            // Search for duration field in the expected key-value format
            fields.iter().skip(1).find_map(|field| {
                let field_str = field.as_str();
                // Check for "duration <value>" or "duration=<value>" format
                if field_str.starts_with("duration ") {
                    field_str.strip_prefix("duration ")
                        .and_then(|d| d.parse::<f64>().ok())
                } else if field_str.starts_with("duration=") {
                    field_str.strip_prefix("duration=")
                        .and_then(|d| d.parse::<f64>().ok())
                } else {
                    None
                }
            })
        });

    // Fetch author profile - reactive to both pubkey and client initialization
    use_effect(use_reactive((&author_pubkey, &*nostr_client::CLIENT_INITIALIZED.read()), move |(pubkey, client_ready)| {
        // Only fetch if client is ready
        if !client_ready {
            return;
        }

        spawn(async move {
            match PublicKey::parse(&pubkey) {
                Ok(pk) => {
                    if let Some(client) = nostr_client::get_client() {
                        if let Ok(Some(metadata)) = client.fetch_metadata(pk, Duration::from_secs(5)).await {
                            author_metadata.set(Some(metadata));
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to parse author_pubkey '{}': {}", pubkey, e);
                }
            }
        });
    }));

    // Fetch reaction counts
    use_effect(use_reactive(&event_id_str, move |event_id_for_counts| {
        spawn(async move {
            let client = match get_client() {
                Some(c) => c,
                None => return,
            };

            let event_id_parsed = match EventId::from_hex(&event_id_for_counts) {
                Ok(id) => id,
                Err(_) => return,
            };

            // Fetch reply count (kind 1 replies)
            let reply_filter = Filter::new()
                .kind(Kind::TextNote)
                .event(event_id_parsed)
                .limit(500);

            if let Ok(replies) = client.fetch_events(reply_filter, Duration::from_secs(5)).await {
                reply_count.set(replies.len());
            }

            // Fetch like count (kind 7 reactions)
            let like_filter = Filter::new()
                .kind(Kind::Reaction)
                .event(event_id_parsed)
                .limit(500);

            if let Ok(likes) = client.fetch_events(like_filter, Duration::from_secs(5)).await {
                let current_user_pubkey = SIGNER_INFO.read().as_ref().map(|info| info.public_key.clone());
                let mut user_has_liked = false;
                let mut positive_likes = 0;

                // Check if current user has liked and count only positive reactions
                if let Some(ref user_pk) = current_user_pubkey {
                    for like in likes.iter() {
                        // Per NIP-25, only count reactions with content != "-" as likes
                        if like.content.trim() != "-" {
                            positive_likes += 1;
                            if like.pubkey.to_string() == *user_pk {
                                user_has_liked = true;
                            }
                        }
                    }
                } else {
                    // If no user logged in, still count only positive reactions
                    positive_likes = likes.iter().filter(|like| like.content.trim() != "-").count();
                }

                like_count.set(positive_likes);
                is_liked.set(user_has_liked);
            }

            // Fetch repost count (kind 6 reposts)
            let repost_filter = Filter::new()
                .kind(Kind::Repost)
                .event(event_id_parsed)
                .limit(500);

            if let Ok(reposts) = client.fetch_events(repost_filter, Duration::from_secs(5)).await {
                let current_user_pubkey = SIGNER_INFO.read().as_ref().map(|info| info.public_key.clone());
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

            // Fetch zap receipts (kind 9735)
            let zap_filter = Filter::new()
                .kind(Kind::from(9735))
                .event(event_id_parsed)
                .limit(500);

            if let Ok(zaps) = client.fetch_events(zap_filter, Duration::from_secs(5)).await {
                let current_user_pubkey = SIGNER_INFO.read().as_ref().map(|info| info.public_key.clone());
                let mut user_has_zapped = false;

                let total_sats: u64 = zaps.iter().filter_map(|zap_event| {
                    // Check if this zap is from the current user
                    if let Some(ref user_pk) = current_user_pubkey {
                        let zap_sender_pubkey = zap_event.tags.iter().find_map(|tag| {
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

                    // Parse zap amount from description tag
                    zap_event.tags.iter().find_map(|tag| {
                        let tag_vec = tag.clone().to_vec();
                        if tag_vec.first()?.as_str() == "description" {
                            let zap_request_json = tag_vec.get(1)?.as_str();
                            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(zap_request_json) {
                                if let Some(tags) = zap_request.get("tags").and_then(|t| t.as_array()) {
                                    for tag_array in tags {
                                        if let Some(tag_vals) = tag_array.as_array() {
                                            if tag_vals.first().and_then(|v| v.as_str()) == Some("amount") {
                                                if let Some(amount_str) = tag_vals.get(1).and_then(|v| v.as_str()) {
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
                }).sum();

                zap_amount_sats.set(total_sats);
                is_zapped.set(user_has_zapped);
            }
        });
    }));

    // Handle play/pause toggle
    let toggle_play = move |_| {
        voice_messages_store::toggle_voice_message(event_id);
    };

    // Control audio element based on playback state
    let audio_id_for_effect = audio_id.clone();
    use_effect(move || {
        // Read the global state to make this effect reactive
        let global_state = voice_messages_store::VOICE_PLAYBACK.read();
        let is_playing = global_state.currently_playing == Some(event_id);
        let audio_id_clone = audio_id_for_effect.clone();

        // Execute DOM operations synchronously without spawning
        // Use direct web-sys calls instead of eval
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
            // play() returns a Promise, but we can ignore errors in the Promise
            let _ = audio.play().map_err(|e| {
                log::debug!("Play failed: {:?}", e);
            });
        } else {
            if let Err(e) = audio.pause() {
                log::debug!("Pause failed: {:?}", e);
            }
        }
    });

    // Handle time update from audio element
    let handle_timeupdate = move |evt: Event<MediaData>| {
        if let Some(target) = evt.data.as_web_event().target() {
            if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                let time = audio.current_time();
                if !time.is_nan() {
                    current_time.set(time);
                    // Only update global state if this card is currently playing
                    if voice_messages_store::is_playing(&event_id) {
                        voice_messages_store::set_current_time(time);
                    }
                }
            }
        }
    };

    // Handle metadata loaded (get duration)
    let handle_loadedmetadata = move |evt: Event<MediaData>| {
        if let Some(target) = evt.data.as_web_event().target() {
            if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                let dur = audio.duration();
                if !dur.is_nan() {
                    duration.set(dur);
                    // Only update global state if this card is currently playing
                    if voice_messages_store::is_playing(&event_id) {
                        voice_messages_store::set_duration(dur);
                    }
                }
                is_loading.set(false);
            }
        }
    };

    // Handle audio ended
    let handle_ended = move |_| {
        voice_messages_store::pause_voice_message();
        current_time.set(0.0);
    };

    // Clone values for interaction handlers
    let event_id_for_like = event_id_str.clone();
    let author_for_like = author_pubkey.clone();
    let event_id_for_repost = event_id_str.clone();
    let author_for_repost = author_pubkey.clone();

    // Like handler
    let handle_like = move |_| {
        let event_id_copy = event_id_for_like.clone();
        let event_author_copy = author_for_like.clone();
        is_liking.set(true);
        spawn(async move {
            match nostr_client::publish_reaction(event_id_copy, event_author_copy, "+".to_string()).await {
                Ok(_) => {
                    log::info!("Like published successfully");
                    is_liked.set(true);
                    let current_count = *like_count.read();
                    like_count.set(current_count + 1);
                }
                Err(e) => log::error!("Failed to publish like: {}", e),
            }
            is_liking.set(false);
        });
    };

    // Repost handler
    let handle_repost = move |_| {
        let event_id_copy = event_id_for_repost.clone();
        let event_author_copy = author_for_repost.clone();
        is_reposting.set(true);
        spawn(async move {
            match nostr_client::publish_repost(event_id_copy, event_author_copy, None).await {
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

    // Format time display
    let current_time_str = voice_messages_store::format_time(*current_time.read());

    // Use the same duration value for both display and calculation
    let duration_val = imeta_duration.unwrap_or(*duration.read());
    let duration_str = voice_messages_store::format_time(duration_val);

    // Calculate progress percentage using the same duration value
    let progress_percent = if duration_val > 0.0 {
        *current_time.read() / duration_val * 100.0
    } else {
        0.0
    };

    // Get author display info
    let author_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or_else(|| m.name.clone()))
        .unwrap_or_else(|| format!("{}...{}", &author_pubkey[..8], &author_pubkey[author_pubkey.len()-8..]));

    let author_username = author_metadata.read().as_ref()
        .and_then(|m| m.name.clone())
        .unwrap_or_default();

    let author_avatar = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone())
        .unwrap_or_default();

    // Time ago formatting
    let time_ago = {
        let now = js_sys::Date::now() / 1000.0;
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

    rsx! {
        div {
            class: "p-4 hover:bg-accent/50 transition cursor-pointer border-b border-border",

            // Author info header
            div {
                class: "flex items-start gap-3 mb-3",

                // Avatar
                Link {
                    to: Route::Profile { pubkey: author_pubkey.clone() },
                    class: "flex-shrink-0",
                    if !author_avatar.is_empty() {
                        img {
                            src: "{author_avatar}",
                            alt: "Avatar",
                            class: "w-12 h-12 rounded-full object-cover bg-muted"
                        }
                    } else {
                        div {
                            class: "w-12 h-12 rounded-full bg-gradient-to-br from-primary to-secondary flex items-center justify-center text-primary-foreground font-bold text-lg",
                            {author_name.chars().next().unwrap_or('?').to_string().to_uppercase()}
                        }
                    }
                }

                // Name and username
                div {
                    class: "flex-1 min-w-0",
                    Link {
                        to: Route::Profile { pubkey: author_pubkey.clone() },
                        class: "hover:underline",
                        div {
                            class: "flex items-center gap-2",
                            span { class: "font-semibold text-foreground truncate", "{author_name}" }
                            if !author_username.is_empty() && author_username != author_name {
                                span { class: "text-muted-foreground text-sm truncate", "@{author_username}" }
                            }
                            span { class: "text-muted-foreground text-sm flex-shrink-0", "· {time_ago}" }
                        }
                    }
                }
            }

            // Audio player
            div {
                class: "mb-3",

                // Hidden audio element
                audio {
                    id: "{audio_id}",
                    src: "{audio_url}",
                    preload: "metadata",
                    style: "display: none;",
                    ontimeupdate: handle_timeupdate,
                    onloadedmetadata: handle_loadedmetadata,
                    onended: handle_ended,
                }

                // Player controls
                div {
                    class: "flex items-center gap-4 bg-muted/30 rounded-lg p-3",

                    // Play/Pause button
                    button {
                        class: "flex-shrink-0 w-10 h-10 rounded-full bg-primary text-primary-foreground hover:bg-primary/90 transition flex items-center justify-center",
                        onclick: toggle_play,
                        if voice_messages_store::VOICE_PLAYBACK.read().currently_playing == Some(event_id) {
                            // Pause icon
                            svg {
                                class: "w-5 h-5",
                                view_box: "0 0 24 24",
                                fill: "currentColor",
                                rect { x: "6", y: "4", width: "4", height: "16" }
                                rect { x: "14", y: "4", width: "4", height: "16" }
                            }
                        } else {
                            // Play icon
                            svg {
                                class: "w-5 h-5 ml-0.5",
                                view_box: "0 0 24 24",
                                fill: "currentColor",
                                polygon { points: "8,5 19,12 8,19" }
                            }
                        }
                    }

                    // Progress bar and time
                    div {
                        class: "flex-1",

                        // Progress bar
                        div {
                            class: "w-full h-1 bg-muted rounded-full overflow-hidden mb-1",
                            div {
                                class: "h-full bg-primary transition-all",
                                style: "width: {progress_percent}%"
                            }
                        }

                        // Time display
                        div {
                            class: "flex justify-between text-xs text-muted-foreground",
                            span { "{current_time_str}" }
                            span { "{duration_str}" }
                        }
                    }
                }
            }

            // Interaction buttons
            div {
                class: "flex items-center justify-between text-muted-foreground",

                // Reply button
                button {
                    class: "flex items-center gap-1 hover:text-blue-500 transition group",
                    onclick: move |_| show_reply_modal.set(true),
                    MessageCircleIcon { class: "w-4 h-4 group-hover:scale-110 transition" }
                    if *reply_count.read() > 0 {
                        span { class: "text-sm", "{reply_count.read()}" }
                    }
                }

                // Repost button
                button {
                    class: if *is_reposted.read() {
                        "flex items-center gap-1 text-green-500 transition group"
                    } else {
                        "flex items-center gap-1 hover:text-green-500 transition group"
                    },
                    onclick: handle_repost,
                    disabled: *is_reposting.read() || *is_reposted.read(),
                    Repeat2Icon { class: "w-4 h-4 group-hover:scale-110 transition" }
                    if *repost_count.read() > 0 {
                        span { class: "text-sm", "{repost_count.read()}" }
                    }
                }

                // Like button
                button {
                    class: if *is_liked.read() {
                        "flex items-center gap-1 text-red-500 transition group"
                    } else {
                        "flex items-center gap-1 hover:text-red-500 transition group"
                    },
                    onclick: handle_like,
                    disabled: *is_liking.read() || *is_liked.read(),
                    HeartIcon {
                        class: "w-4 h-4 group-hover:scale-110 transition",
                        filled: *is_liked.read()
                    }
                    if *like_count.read() > 0 {
                        span { class: "text-sm", "{like_count.read()}" }
                    }
                }

                // Zap button
                button {
                    class: if *is_zapped.read() {
                        "flex items-center gap-1 text-yellow-500 transition group"
                    } else {
                        "flex items-center gap-1 hover:text-yellow-500 transition group"
                    },
                    onclick: move |_| show_zap_modal.set(true),
                    ZapIcon { class: "w-4 h-4 group-hover:scale-110 transition" }
                    if *zap_amount_sats.read() > 0 {
                        span { class: "text-sm", "{zap_amount_sats.read()}" }
                    }
                }
            }

            // Voice reply modal
            if *show_reply_modal.read() {
                VoiceReplyComposer {
                    reply_to: event_clone.clone(),
                    on_close: move |_| show_reply_modal.set(false),
                    on_success: move |_| {
                        show_reply_modal.set(false);
                    }
                }
            }

            // Zap modal
            if *show_zap_modal.read() {
                ZapModal {
                    recipient_pubkey: author_pubkey.clone(),
                    recipient_name: author_name.clone(),
                    lud16: author_metadata.read().as_ref().and_then(|m| m.lud16.clone()),
                    lud06: author_metadata.read().as_ref().and_then(|m| m.lud06.clone()),
                    event_id: Some(event_id_str.clone()),
                    on_close: move |_| show_zap_modal.set(false)
                }
            }
        }
    }
}
