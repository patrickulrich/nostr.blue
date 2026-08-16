use crate::components::icons::{BookmarkIcon, MessageCircleIcon, PlayIcon, ZapIcon};
use crate::components::{ReactionButton, SensitiveContent, ZapModal};
use crate::hooks::use_reaction;
use crate::routes::Route;
use crate::stores::bookmarks;
use crate::stores::nostr_client::{get_client, HAS_SIGNER};
use crate::stores::signer::SIGNER_INFO;
use crate::utils::duration::format_duration_timecode_padded;
use crate::utils::nip36;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind};
use std::time::Duration;
/// Skeleton loader for VideoCard - prevents layout shift during loading
#[component]
pub fn VideoCardSkeleton() -> Element {
    rsx! {
        div { class: "border-b border-border animate-pulse",
            div { class: "p-4 flex items-center gap-3",
                div { class: "w-12 h-12 rounded-full bg-gray-300 dark:bg-gray-700" }
                div { class: "flex-1 space-y-2",
                    div { class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-1/4" }
                    div { class: "h-3 bg-gray-300 dark:bg-gray-700 rounded w-1/6" }
                }
            }
            div { class: "relative bg-gray-300 dark:bg-gray-700 aspect-video" }
            div { class: "p-4 space-y-2",
                div { class: "h-5 bg-gray-300 dark:bg-gray-700 rounded w-3/4" }
                div { class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-full" }
                div { class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-2/3" }
            }
            div { class: "px-4 pb-4 flex items-center gap-6",
                for _ in 0..4 {
                    div { class: "w-8 h-8 bg-gray-300 dark:bg-gray-700 rounded" }
                }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct VideoMeta {
    pub url: String,
    pub mime_type: Option<String>,
    pub duration: Option<f64>,
    pub dim: Option<(u32, u32)>,
    pub thumbnail: Option<String>,
    pub blurhash: Option<String>,
    pub fallback_urls: Vec<String>,
}
/// Parse imeta tags from NIP-71 video events
pub fn parse_video_imeta_tags(event: &Event) -> Vec<VideoMeta> {
    let mut videos = Vec::new();
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("imeta") {
            let mut video = VideoMeta {
                url: String::new(),
                mime_type: None,
                duration: None,
                dim: None,
                thumbnail: None,
                blurhash: None,
                fallback_urls: Vec::new(),
            };
            for field in tag_vec.iter().skip(1) {
                if let Some((key, value)) = field.split_once(' ') {
                    match key {
                        "url" => video.url = value.to_string(),
                        "m" => video.mime_type = Some(value.to_string()),
                        "duration" => {
                            if let Ok(dur) = value.parse::<f64>() {
                                video.duration = Some(dur);
                            }
                        }
                        "dim" => {
                            if let Some((w, h)) = value.split_once('x') {
                                if let (Ok(width), Ok(height)) = (w.parse(), h.parse()) {
                                    video.dim = Some((width, height));
                                }
                            }
                        }
                        "image" if video.thumbnail.is_none() => {
                            video.thumbnail = Some(value.to_string());
                        }
                        "blurhash" => {
                            video.blurhash = Some(value.to_string());
                        }
                        "fallback" => {
                            video.fallback_urls.push(value.to_string());
                        }
                        _ => {}
                    }
                }
            }
            if !video.url.is_empty() {
                videos.push(video);
            }
        }
    }
    videos
}

fn blurhash_js(hash: &str, width: u32, height: u32) -> String {
    format!(
        r#"
        return (function() {{
            var Base83 = {{
                chars: "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$",
                decode: function(str) {{
                    var v = 0;
                    for (var i = 0; i < str.length; i++) {{
                        var c = this.chars.indexOf(str[i]);
                        if (c === -1) return 0;
                        v = v * 83 + c;
                    }}
                    return v;
                }}
            }};
            function signPow(val, exp) {{ return Math.sign(val) * Math.pow(Math.abs(val), exp); }}
            function sRGBToLinear(value) {{ var v = Math.max(0, Math.min(1, value)); return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }}
            function tosRGB(value) {{ var v = Math.max(0, Math.min(1, value)); return v <= 0.0031308 ? Math.round(v * 12.92 * 255 + 0.5) : Math.round((1.055 * Math.pow(v, 1.0 / 2.4) - 0.055) * 255 + 0.5); }}
            try {{
                var size_flag = Base83.decode("{hash}"[0]);
                var num_y = Math.floor(size_flag / 9) + 1;
                var num_x = (size_flag % 9) + 1;
                var quant_max_value = Base83.decode("{hash}"[1]);
                var max_value = (quant_max_value + 1) / 166.0;
                var colors = [];
                for (var i = 0; i < num_x * num_y; i++) {{
                    if (i === 0) {{
                        var value = Base83.decode("{hash}".substring(2, 6));
                        colors.push([sRGBToLinear((value >> 16) / 255.0), sRGBToLinear(((value >> 8) & 255) / 255.0), sRGBToLinear((value & 255) / 255.0)]);
                    }} else {{
                        var value = Base83.decode("{hash}".substring(4 + i * 2, 6 + i * 2));
                        colors.push([
                            signPow((Math.floor(value / (19 * 19)) - 9.0) / 9.0, 2.0) * max_value,
                            signPow(((Math.floor(value / 19) % 19) - 9.0) / 9.0, 2.0) * max_value,
                            signPow(((value % 19) - 9.0) / 9.0, 2.0) * max_value
                        ]);
                    }}
                }}
                var pixelsPerRow = [];
                for (var y = 0; y < {height}; y++) {{
                    for (var x = 0; x < {width}; x++) {{
                        var r = 0, g = 0, b = 0;
                        for (var j = 0; j < num_y; j++) {{
                            for (var i = 0; i < num_x; i++) {{
                                var basis = Math.cos((Math.PI * x * i) / {width}) * Math.cos((Math.PI * y * j) / {height});
                                r += colors[i + j * num_x][0] * basis;
                                g += colors[i + j * num_x][1] * basis;
                                b += colors[i + j * num_x][2] * basis;
                            }}
                        }}
                        pixelsPerRow.push(tosRGB(r), tosRGB(g), tosRGB(b));
                    }}
                }}
                var c = document.createElement('canvas');
                c.width = {width}; c.height = {height};
                var ctx = c.getContext('2d');
                var id = ctx.createImageData({width}, {height});
                for (var i = 0, j = 0; i < pixelsPerRow.length; i += 3, j += 4) {{
                    id.data[j] = pixelsPerRow[i];
                    id.data[j + 1] = pixelsPerRow[i + 1];
                    id.data[j + 2] = pixelsPerRow[i + 2];
                    id.data[j + 3] = 255;
                }}
                ctx.putImageData(id, 0, 0);
                return c.toDataURL('image/jpeg', 0.7);
            }} catch(e) {{ return null; }}
        }})()
        "#
    )
}

async fn eval_blurhash(hash: &str, width: u32, height: u32) -> Option<String> {
    let js = blurhash_js(hash, width, height);
    document::eval(&js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

fn capture_frame_js(element_id: &str) -> String {
    format!(
        r#"
        return (function() {{
            var v = document.getElementById("{element_id}");
            if (!v || !v.videoWidth) return null;
            try {{
                var c = document.createElement('canvas');
                c.width = v.videoWidth; c.height = v.videoHeight;
                c.getContext('2d').drawImage(v, 0, 0);
                return c.toDataURL('image/jpeg', 0.7);
            }} catch(e) {{ return null; }}
        }})()
        "#
    )
}

async fn eval_capture_frame(element_id: &str) -> Option<String> {
    let js = capture_frame_js(element_id);
    document::eval(&js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

/// Get the title from NIP-71 video events
pub fn get_video_title(event: &Event) -> Option<String> {
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("title") {
            return tag_vec.get(1).map(|s| s.to_string());
        }
    }
    None
}

#[component]
pub fn VideoCard(event: Event) -> Element {
    let videos = parse_video_imeta_tags(&event);
    let title = get_video_title(&event);
    let description = &event.content;
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_fetch = author_pubkey.clone();
    let author_pubkey_for_like = author_pubkey.clone();
    let author_pubkey_display = author_pubkey.clone();
    let created_at = event.created_at;
    let event_id = event.id.to_string();
    let event_id_like = event_id.clone();
    let event_id_bookmark = event_id.clone();
    let event_id_memo = event_id.clone();
    let event_id_counts = event_id.clone();
    let mut is_zapped = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    let is_bookmarked = bookmarks::is_bookmarked(&event_id_memo);
    let has_signer = *HAS_SIGNER.read();
    let reaction = use_reaction(event_id_like.clone(), author_pubkey_for_like.clone(), None);
    let mut reply_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut show_zap_modal = use_signal(|| false);
    let content_warning = nip36::get_content_warning(&event.tags);
    if videos.is_empty() {
        return rsx! {
            div { class: "hidden" }
        };
    }
    let first_video = &videos[0];
    use_effect(use_reactive(&event_id_counts, move |event_id_for_counts| {
        spawn(async move {
            let client = match get_client() {
                Some(c) => c,
                None => return,
            };
            let event_id_parsed = match nostr_sdk::EventId::from_hex(&event_id_for_counts) {
                Ok(id) => id,
                Err(_) => return,
            };
            let interaction_filter = Filter::new()
                .kinds(vec![Kind::TextNote, Kind::Comment])
                .event(event_id_parsed)
                .limit(500);
            let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
            let nip22_filter = Filter::new()
                .kind(Kind::Comment)
                .custom_tag(upper_e_tag, event_id_for_counts.clone())
                .limit(500);
            let mut all_reply_ids = std::collections::HashSet::new();
            if let Ok(events) = client
                .fetch_events(interaction_filter, Duration::from_secs(5))
                .await
            {
                for event in events.iter() {
                    all_reply_ids.insert(event.id);
                }
            }
            if let Ok(events) = client
                .fetch_events(nip22_filter, Duration::from_secs(5))
                .await
            {
                for event in events.iter() {
                    all_reply_ids.insert(event.id);
                }
            }
            reply_count.set(all_reply_ids.len());
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
    }));
    // Author metadata: derived from PROFILE_CACHE, re-evaluated on either a
    // cache version bump or a pubkey change. If the profile is missing,
    // enqueue it for the app-shell batch drain (single REQ for the whole
    // batch instead of N per-card REQs). The version is read into a local
    // *before* `use_memo` so the `ReadRef` is dropped at the end of the
    // `let` line — otherwise it would still be alive when the memo polls
    // the closure synchronously, and the inner `queue_profile_request` ->
    // `bump_cache_version` -> `with_mut` would panic with `AlreadyBorrowed`.
    let author_version = *crate::stores::profiles::PROFILE_CACHE_VERSION.read();
    let _ = use_memo(use_reactive(
        (&author_version, &author_pubkey_for_fetch),
        move |(_v, pk): (u64, String)| {
            if let Some(p) = crate::stores::profiles::get_profile(&pk) {
                author_metadata.set(Some(p));
            } else {
                crate::stores::profiles::queue_profile_request(pk);
            }
        },
    ));
    let handle_bookmark = move |_| {
        if *is_bookmarking.read() || !has_signer {
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
    };
    let author_name = if let Some(ref metadata) = *author_metadata.read() {
        metadata
            .display_name
            .clone()
            .or_else(|| metadata.name.clone())
            .unwrap_or_else(|| truncate_pubkey(&author_pubkey_display))
    } else {
        truncate_pubkey(&author_pubkey_display)
    };
    let author_picture = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone());
    let formatted_duration = first_video
        .duration
        .map(|d| format_duration_timecode_padded(d as u64));
    let video_src = if first_video.thumbnail.is_none() {
        format!("{}#t=0.1", first_video.url)
    } else {
        first_video.url.clone()
    };
    let has_thumbnail = first_video.thumbnail.is_some();
    let video_element_id = format!("vc-{}", &event_id[..8]);
    let blurhash_str = first_video.blurhash.clone();
    let mut captured_poster: Signal<Option<String>> = use_signal(|| None);
    let video_element_id_for_capture = video_element_id.clone();
    let has_no_thumbnail = !has_thumbnail;
    use_effect(move || {
        if let Some(ref bh) = blurhash_str {
            let bh = bh.clone();
            spawn(async move {
                if let Some(data_url) = eval_blurhash(&bh, 32, 32).await {
                    captured_poster.set(Some(data_url));
                }
            });
        }
    });
    use_effect(move || {
        if !has_no_thumbnail || captured_poster.read().is_some() {
            return;
        }
        let vid = video_element_id_for_capture.clone();
        spawn(async move {
            crate::platform::timer::sleep_ms(1500).await;
            if let Some(data_url) = eval_capture_frame(&vid).await {
                captured_poster.set(Some(data_url));
            }
        });
    });
    let effective_poster = first_video
        .thumbnail
        .clone()
        .or_else(|| captured_poster.read().clone());
    let show_play_overlay = !has_thumbnail && captured_poster.read().is_none();
    rsx! {
        div { class: "border-b border-border hover:bg-accent/5 transition",
            div { class: "p-4 flex items-center gap-3",
                Link {
                    to: Route::AddressViewer {
                        address: crate::utils::nip19_urls::profile_route_id(&author_pubkey),
                    },
                    class: "flex items-center gap-3 flex-1",
                    if let Some(pic_url) = author_picture {
                        img {
                            src: "{pic_url}",
                            class: "w-12 h-12 rounded-full object-cover",
                            alt: "Avatar",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-12 h-12 rounded-full bg-blue-600 flex items-center justify-center text-white font-bold",
                            "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                        }
                    }
                    div { class: "flex-1",
                        div { class: "font-semibold", "{author_name}" }
                        div { class: "text-sm text-muted-foreground",
                            "{created_at.to_human_datetime()}"
                        }
                    }
                }
            }
            {
                if let Some(reason) = content_warning.clone() {
                    rsx! {
                        SensitiveContent { reason,
                            div { class: "relative bg-black",
                                if let Some(ref bg) = *captured_poster.read() {
                                    img {
                                        src: "{bg}",
                                        class: "absolute inset-0 w-full h-full object-contain",
                                    }
                                }
                                video {
                                    id: "{video_element_id}",
                                    class: "w-full max-h-[600px] object-contain relative z-10",
                                    controls: true,
                                    preload: "metadata",
                                    poster: effective_poster.as_deref(),
                                    source {
                                        src: "{video_src}",
                                        r#type: first_video.mime_type.as_deref().unwrap_or("video/mp4"),
                                    }
                                    for fallback_url in &first_video.fallback_urls {
                                        source { src: "{fallback_url}" }
                                    }
                                    "Your browser does not support the video tag."
                                }
                                if let Some(dur) = &formatted_duration {
                                    div { class: "absolute bottom-2 right-2 bg-black/75 text-white text-xs px-2 py-1 rounded z-20",
                                        "{dur}"
                                    }
                                }
                                if show_play_overlay {
                                    div { class: "absolute inset-0 flex items-center justify-center pointer-events-none z-20",
                                        div { class: "w-16 h-16 rounded-full bg-white/20 backdrop-blur-sm flex items-center justify-center",
                                            PlayIcon { class: "w-8 h-8 text-white ml-1" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    rsx! {
                        div { class: "relative bg-black",
                            if let Some(ref bg) = *captured_poster.read() {
                                img {
                                    src: "{bg}",
                                    class: "absolute inset-0 w-full h-full object-contain",
                                }
                            }
                            video {
                                id: "{video_element_id}",
                                class: "w-full max-h-[600px] object-contain relative z-10",
                                controls: true,
                                preload: "metadata",
                                poster: effective_poster.as_deref(),
                                source {
                                    src: "{video_src}",
                                    r#type: first_video.mime_type.as_deref().unwrap_or("video/mp4"),
                                }
                                for fallback_url in &first_video.fallback_urls {
                                    source { src: "{fallback_url}" }
                                }
                                "Your browser does not support the video tag."
                            }
                            if let Some(dur) = &formatted_duration {
                                div { class: "absolute bottom-2 right-2 bg-black/75 text-white text-xs px-2 py-1 rounded z-20",
                                    "{dur}"
                                }
                            }
                            if show_play_overlay {
                                div { class: "absolute inset-0 flex items-center justify-center pointer-events-none z-20",
                                    div { class: "w-16 h-16 rounded-full bg-white/20 backdrop-blur-sm flex items-center justify-center",
                                        PlayIcon { class: "w-8 h-8 text-white ml-1" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            {
                if let Some(reason) = content_warning.clone() {
                    rsx! {
                        SensitiveContent { reason,
                            div { class: "p-4",
                                if let Some(title_text) = &title {
                                    h3 { class: "font-bold text-lg mb-2", "{title_text}" }
                                }
                                if !description.is_empty() {
                                    p { class: "text-sm whitespace-pre-wrap", "{description}" }
                                }
                            }
                        }
                    }
                } else {
                    rsx! {
                        div { class: "p-4",
                            if let Some(title_text) = &title {
                                h3 { class: "font-bold text-lg mb-2", "{title_text}" }
                            }
                            if !description.is_empty() {
                                p { class: "text-sm whitespace-pre-wrap", "{description}" }
                            }
                        }
                    }
                }
            }
            div { class: "px-4 pb-4 flex items-center gap-6 text-muted-foreground",
                Link {
                    to: Route::AddressViewer {
                        address: crate::utils::nip19_urls::note_route_id_with_kind(&event_id, Some(&author_pubkey), Some(event.kind)),
                    },
                    class: "flex items-center gap-2 hover:text-blue-500 transition",
                    MessageCircleIcon { class: "w-5 h-5" }
                    if *reply_count.read() > 0 {
                        span { class: "text-sm", "{reply_count.read()}" }
                    }
                }
                ReactionButton {
                    reaction: reaction.clone(),
                    has_signer,
                    icon_class: "w-5 h-5".to_string(),
                    count_class: "text-sm".to_string(),
                }
                button {
                    class: if *is_zapped.read() { "flex items-center gap-2 text-yellow-500 transition" } else { "flex items-center gap-2 hover:text-yellow-500 transition" },
                    disabled: !has_signer,
                    onclick: move |_| show_zap_modal.set(true),
                    ZapIcon {
                        class: "w-5 h-5".to_string(),
                        filled: *is_zapped.read(),
                    }
                    if *zap_amount_sats.read() > 0 {
                        span { class: "text-sm", "{zap_amount_sats.read()}" }
                    }
                }
                button {
                    class: if is_bookmarked { "flex items-center gap-2 text-blue-500 hover:text-blue-600 transition" } else { "flex items-center gap-2 hover:text-blue-500 transition" },
                    disabled: *is_bookmarking.read() || !has_signer,
                    onclick: handle_bookmark,
                    BookmarkIcon { class: "w-5 h-5", filled: is_bookmarked }
                }
            }
        }
        if *show_zap_modal.read() {
            ZapModal {
                recipient_pubkey: author_pubkey.clone(),
                recipient_name: author_name.clone(),
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
