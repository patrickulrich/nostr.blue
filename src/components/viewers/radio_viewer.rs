use crate::components::icons::{self, MoreHorizontalIcon};
use crate::components::{ConfirmModal, ContentShareModal, ContentType};
use crate::routes::Route;
use crate::stores::music_player::{self, MusicPlayerStateStoreExt, MusicTrack, MUSIC_PLAYER};
use crate::stores::{auth_store, nostr_client};
use crate::utils::radio::{
    delete_radio_station, fetch_station_by_naddr, get_ranked_stream_urls,
    RadioStation as RadioStationData,
};
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::{EventId, Filter};
use std::collections::HashSet;
use std::time::Duration;
#[component]
pub fn RadioViewer(naddr: String) -> Element {
    let mut station = use_signal(|| None::<RadioStationData>);
    let mut is_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut selected_stream_idx = use_signal(|| 0usize);
    let mut is_menu_open = use_signal(|| false);
    let mut is_broadcasting = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);
    let mut is_deleting = use_signal(|| false);
    let mut show_share_modal = use_signal(|| false);
    let toast = consume_toast();
    let nav = navigator();
    let naddr_clone = naddr.clone();
    use_effect(use_reactive(&naddr_clone, move |naddr_val| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        // Fire-and-forget favorites load (idempotent, signer-gated) so the
        // heart reflects the user's list.
        spawn(async move {
            crate::stores::audio::radio_favorites::load().await;
        });
        spawn(async move {
            is_loading.set(true);
            error.set(None);
            match fetch_station_by_naddr(&naddr_val).await {
                Ok(fetched_station) => {
                    station.set(Some(fetched_station));
                }
                Err(e) => {
                    log::error!("Failed to fetch radio station: {}", e);
                    error.set(Some(e));
                }
            }
            is_loading.set(false);
        });
    }));
    let station_id = station
        .read()
        .as_ref()
        .map(|s| s.coordinate.clone())
        .unwrap_or_default();
    let station_id_for_memo = station_id.clone();
    let is_playing = use_memo(move || {
        let store = MUSIC_PLAYER.resolve();
        let current = store.current_track().cloned();
        if let Some(ref cur) = current {
            cur.id == station_id_for_memo && store.is_playing().cloned()
        } else {
            false
        }
    });
    rsx! {
        // Root onclick closes the overflow menu on any outside click. Clicks on
        // child controls (play, stream selector) intentionally call
        // `stop_propagation()` where needed so they don't dismiss the menu
        // mid-interaction; other clicks bubble up and close it.
        div {
            class: "min-h-screen",
            onclick: move |_| {
                if *is_menu_open.read() {
                    is_menu_open.set(false);
                }
            },
            div { class: "sticky top-0 z-30 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::RadioHome {},
                            class: "p-2 hover:bg-muted rounded-lg transition",
                            dangerous_inner_html: icons::ARROW_LEFT,
                        }
                        h1 { class: "text-lg font-bold", "Radio Station" }
                    }
                    if station.read().is_some() {
                        div { class: "relative",
                            button {
                                class: "p-2 rounded-full hover:bg-accent transition-colors text-muted-foreground hover:text-foreground",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    is_menu_open.set(!is_menu_open());
                                },
                                MoreHorizontalIcon { class: "h-5 w-5".to_string(), filled: false }
                            }
                            if *is_menu_open.read() {
                                div {
                                    class: "absolute right-0 mt-2 w-52 bg-background border border-border rounded-lg shadow-lg z-50 py-1",
                                    onclick: move |e: MouseEvent| {
                                        e.stop_propagation();
                                    },
                                    if let Some(s) = station.read().as_ref() {
                                        {
                                            let is_own = auth_store::get_pubkey()
                                                .map(|pk| pk == s.pubkey)
                                                .unwrap_or(false);
                                            let naddr_val =
                                                crate::utils::audio::radio::station_share_naddr(
                                                    s, &naddr,
                                                );
                                            let event_id_hex = s.event_id.clone();
                                            let toast_api = toast;
                                            rsx! {
                                                if is_own {
                                                    Link {
                                                        to: Route::RadioStationNew { edit_naddr: Some(naddr_val.clone()) },
                                                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-sm",
                                                        onclick: move |_| is_menu_open.set(false),
                                                        "Edit Station"
                                                    }
                                                }
                                                button {
                                                    class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-sm",
                                                    disabled: *is_broadcasting.read(),
                                                    onclick: move |e: MouseEvent| {
                                                        e.stop_propagation();
                                                        if *is_broadcasting.read() { return; }
                                                        let eid = event_id_hex.clone();
                                                        let toast_api = toast_api;
                                                        is_broadcasting.set(true);
                                                        is_menu_open.set(false);
                                                        spawn(async move {
                                                            let filter = Filter::new().id(
                                                                EventId::from_hex(&eid).unwrap_or_else(|_| EventId::all_zeros())
                                                            ).limit(1);
                                                            match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                                                                Ok(events) => {
                                                                    if let Some(raw_event) = events.into_iter().next() {
                                                                        let mut relay_urls = crate::stores::relay::get_write_relays();
                                                                        relay_urls.extend(crate::stores::relay::BROADCAST_RELAYS.read().clone());
                                                                        relay_urls.retain(|url| !crate::stores::relay::is_relay_blocked(url));
                                                                        let mut seen = HashSet::new();
                                                                        relay_urls.retain(|url| seen.insert(url.trim_end_matches('/').to_string()));
                                                                        if relay_urls.is_empty() {
                                                                            toast_api.warning(
                                                                                "No relays configured".to_string(),
                                                                                ToastOptions::new()
                                                                                    .description("Add write relays or broadcast relays in Settings")
                                                                                    .duration(Duration::from_secs(3))
                                                                                    .permanent(false),
                                                                            );
                                                                        } else {
                                                                            match nostr_client::broadcast_presigned_event(raw_event, relay_urls).await {
                                                                                Ok(result) => {
                                                                                    if result.is_success() {
                                                                                        toast_api.success(
                                                                                            "Broadcast queued".to_string(),
                                                                                            ToastOptions::new().duration(Duration::from_secs(3)).permanent(false),
                                                                                        );
                                                                                    } else {
                                                                                        toast_api.error(
                                                                                            "Broadcast failed".to_string(),
                                                                                            ToastOptions::new().duration(Duration::from_secs(3)).permanent(false),
                                                                                        );
                                                                                    }
                                                                                }
                                                                                Err(e) => {
                                                                                    toast_api.error(
                                                                                        "Broadcast failed".to_string(),
                                                                                        ToastOptions::new().description(e).duration(Duration::from_secs(3)).permanent(false),
                                                                                    );
                                                                                }
                                                                            }
                                                                        }
                                                                    } else {
                                                                        toast_api.error(
                                                                            "Event not found on relays".to_string(),
                                                                            ToastOptions::new().duration(Duration::from_secs(3)).permanent(false),
                                                                        );
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    toast_api.error(
                                                                        "Failed to fetch event".to_string(),
                                                                        ToastOptions::new().description(e).duration(Duration::from_secs(3)).permanent(false),
                                                                    );
                                                                }
                                                            }
                                                            is_broadcasting.set(false);
                                                        });
                                                    },
                                                    if *is_broadcasting.read() { "Broadcasting..." } else { "Broadcast" }
                                                }
                                                button {
                                                    class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-sm",
                                                    onclick: move |e: MouseEvent| {
                                                        e.stop_propagation();
                                                        is_menu_open.set(false);
                                                        show_share_modal.set(true);
                                                    },
                                                    "Share Station"
                                                }
                                                button {
                                                    class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-sm",
                                                    onclick: move |e: MouseEvent| {
                                                        e.stop_propagation();
                                                        let naddr_str = naddr_val.clone();
                                                        let toast_api = toast_api;
                                                        spawn(async move {
                                                            let text = format!("nostr:{}", naddr_str);
                                                            match crate::platform::clipboard::copy_to_clipboard(&text).await {
                                                                Ok(()) => {
                                                                    toast_api.success(
                                                                        "Copied to clipboard".to_string(),
                                                                        ToastOptions::new().duration(Duration::from_secs(2)).permanent(false),
                                                                    );
                                                                }
                                                                Err(e) => {
                                                                    log::error!("Failed to copy: {:?}", e);
                                                                    toast_api.error(
                                                                        "Failed to copy".to_string(),
                                                                        ToastOptions::new().description("Could not access clipboard".to_string()).duration(Duration::from_secs(2)).permanent(false),
                                                                    );
                                                                }
                                                            }
                                                        });
                                                        is_menu_open.set(false);
                                                    },
                                                    "Copy Station ID"
                                                }
                                                if is_own {
                                                    button {
                                                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-sm text-red-500",
                                                        disabled: *is_deleting.read(),
                                                        onclick: move |e: MouseEvent| {
                                                            e.stop_propagation();
                                                            show_delete_confirm.set(true);
                                                        },
                                                        if *is_deleting.read() { "Deleting..." } else { "Delete" }
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
            }
            div { class: "p-4 max-w-2xl mx-auto",
                if *is_loading.read() {
                    div { class: "animate-pulse space-y-6",
                        div { class: "aspect-square max-w-sm mx-auto bg-muted rounded-xl" }
                        div { class: "space-y-3",
                            div { class: "h-8 bg-muted rounded w-3/4 mx-auto" }
                            div { class: "h-4 bg-muted rounded w-1/2 mx-auto" }
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        div { class: "w-16 h-16 rounded-full bg-destructive/10 flex items-center justify-center mb-4",
                            span { class: "text-destructive text-2xl", "!" }
                        }
                        p { class: "text-lg font-medium text-destructive", "Failed to load station" }
                        p { class: "text-sm text-muted-foreground mt-1", "{err}" }
                        Link {
                            to: Route::RadioHome {},
                            class: "mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Stations"
                        }
                    }
                } else if let Some(s) = station.read().as_ref() {
                    div { class: "space-y-6",
                        div { class: "relative aspect-square max-w-sm mx-auto",
                            img {
                                src: s.thumbnail
                                    .clone()
                                    .unwrap_or_else(|| {
                                        "https://api.dicebear.com/7.x/shapes/svg?seed=radio".to_string()
                                    }),
                                alt: "{s.name}",
                                class: "w-full h-full object-cover rounded-xl shadow-lg",
                            }
                            if *is_playing.read() {
                                div { class: "absolute top-4 left-4 inline-flex items-center gap-1.5 px-2 py-1 rounded-full bg-red-500 text-white text-sm font-bold",
                                    span { class: "w-2 h-2 rounded-full bg-white animate-pulse" }
                                    "LIVE"
                                }
                            }
                            button {
                                class: "absolute inset-0 flex items-center justify-center bg-black/30 hover:bg-black/50 transition rounded-xl group",
                                onclick: {
                                    let play_station = s.clone();
                                    let stream_idx = *selected_stream_idx.read();
                                    move |_| {
                                        let player_state = MUSIC_PLAYER.read();
                                        if let Some(ref current) = player_state.current_track {
                                            if current.id == play_station.coordinate && player_state.is_playing {
                                                drop(player_state);
                                                music_player::toggle_play();
                                                return;
                                            }
                                        }
                                        drop(player_state);
                                        let ranked_streams = get_ranked_stream_urls(&play_station.streams);
                                        if ranked_streams.is_empty() {
                                            log::warn!(
                                                "Station has no available streams: {}",
                                                play_station.name
                                            );
                                            return;
                                        }
                                        music_player::set_available_streams(ranked_streams);
                                        let mut music_track: MusicTrack = play_station.clone().into();
                                        if let Some(stream) = play_station.streams.get(stream_idx) {
                                            music_track.media_url = stream.url.clone();
                                        }
                                        music_player::play_track(music_track, None, None);
                                    }
                                },
                                div {
                                    class: "w-20 h-20 rounded-full bg-primary flex items-center justify-center text-primary-foreground shadow-lg group-hover:scale-110 transition",
                                    dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                                }
                            }
                        }
                        div { class: "text-center space-y-2",
                            h1 { class: "text-2xl font-bold", "{s.name}" }
                            if let Some(location) = s.location.as_ref().or(s.country_code.as_ref()) {
                                p { class: "text-muted-foreground", "{location}" }
                            }
                            if !s.genres.is_empty() {
                                div { class: "flex flex-wrap justify-center gap-2 mt-3",
                                    for genre in s.genres.iter() {
                                        span { class: "px-3 py-1 bg-muted rounded-full text-sm",
                                            "{genre}"
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(desc) = s.description.as_ref() {
                            div { class: "bg-muted/50 rounded-lg p-4",
                                p { class: "text-sm text-muted-foreground", "{desc}" }
                            }
                        }
                        if s.streams.len() > 1 {
                            div { class: "space-y-2",
                                h3 { class: "text-sm font-medium text-muted-foreground",
                                    "Stream Quality"
                                }
                                div { class: "grid gap-2",
                                    for (idx , stream) in s.streams.iter().enumerate() {
                                        button {
                                            key: "{idx}",
                                            class: if *selected_stream_idx.read() == idx { "flex items-center justify-between p-3 rounded-lg border-2 border-primary bg-primary/10" } else { "flex items-center justify-between p-3 rounded-lg border border-border hover:border-primary/50 transition" },
                                            onclick: move |_| selected_stream_idx.set(idx),
                                            div { class: "flex items-center gap-3",
                                                span { class: "font-medium",
                                                    "{stream.format.display_name()}"
                                                }
                                                if let Some(bitrate) = stream.bitrate() {
                                                    span { class: "text-sm text-muted-foreground",
                                                        "{bitrate} kbps"
                                                    }
                                                }
                                            }
                                            if stream.is_primary {
                                                span { class: "text-xs px-2 py-0.5 bg-primary/20 text-primary rounded-full",
                                                    "Primary"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(website) = s.website.as_ref().filter(|w| is_valid_http_url(w)) {
                            a {
                                href: "{website}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "flex items-center justify-center gap-2 w-full p-3 bg-muted rounded-lg hover:bg-muted/80 transition",
                                span {
                                    class: "w-4 h-4",
                                    dangerous_inner_html: icons::EXTERNAL_LINK,
                                }
                                "Visit Website"
                            }
                        }
                        div { class: "pt-4 flex gap-2",
                            {
                                let has_signer = *nostr_client::HAS_SIGNER.read();
                                let is_fav = crate::stores::audio::radio_favorites::is_favorite(
                                    &s.coordinate,
                                );
                                rsx! {
                                    button {
                                        class: if is_fav {
                                            "flex items-center justify-center gap-2 flex-1 p-3 bg-red-500/10 text-red-500 rounded-lg hover:bg-red-500/20 transition font-medium disabled:opacity-50 disabled:cursor-not-allowed"
                                        } else {
                                            "flex items-center justify-center gap-2 flex-1 p-3 bg-muted rounded-lg hover:bg-muted/80 transition font-medium disabled:opacity-50 disabled:cursor-not-allowed"
                                        },
                                        disabled: !has_signer,
                                        title: if has_signer { "Toggle favorite" } else { "Sign in to favorite stations" },
                                        onclick: {
                                            let fav_station = s.clone();
                                            let toast_api = toast;
                                            move |_| {
                                                let fav_station = fav_station.clone();
                                                spawn(async move {
                                                    match crate::stores::audio::radio_favorites::toggle_favorite(&fav_station).await {
                                                        Ok(true) => {
                                                            toast_api.success(
                                                                "Added to favorites".to_string(),
                                                                ToastOptions::new()
                                                                    .description(format!("{} saved to your favorite stations", fav_station.name))
                                                                    .duration(Duration::from_secs(3))
                                                                    .permanent(false),
                                                            );
                                                        }
                                                        Ok(false) => {
                                                            toast_api.success(
                                                                "Removed from favorites".to_string(),
                                                                ToastOptions::new()
                                                                    .duration(Duration::from_secs(3))
                                                                    .permanent(false),
                                                            );
                                                        }
                                                        Err(e) => {
                                                            toast_api.error(
                                                                "Error".to_string(),
                                                                ToastOptions::new()
                                                                    .description(format!("Failed to toggle favorite: {e}"))
                                                                    .duration(Duration::from_secs(3))
                                                                    .permanent(false),
                                                            );
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        crate::components::icons::HeartIcon {
                                            class: "w-5 h-5".to_string(),
                                            filled: is_fav,
                                        }
                                        if is_fav { "Favorited" } else { "Favorite" }
                                    }
                                }
                            }
                            button {
                                class: "flex items-center justify-center gap-2 flex-1 p-3 bg-amber-500/10 text-amber-500 rounded-lg hover:bg-amber-500/20 transition font-medium",
                                onclick: {
                                    let zap_station = s.clone();
                                    move |_| {
                                        let music_track: MusicTrack = zap_station.clone().into();
                                        music_player::show_zap_dialog_for_track(Some(music_track));
                                    }
                                },
                                span {
                                    class: "w-5 h-5",
                                    dangerous_inner_html: icons::ZAP,
                                }
                                "Zap"
                            }
                        }
                    }
                }
            }
            if *show_share_modal.read() {
                if let Some(s) = station.read().as_ref() {
                    {
                        let naddr_val = crate::utils::audio::radio::station_share_naddr(s, &naddr);
                        rsx! {
                            ContentShareModal {
                                title: s.name.clone(),
                                url: format!("https://nostr.blue/radio/{}", naddr_val),
                                content_type: ContentType::RadioStation,
                                image_url: s.thumbnail.clone(),
                                on_close: move |_| show_share_modal.set(false),
                            }
                        }
                    }
                }
            }
            if *show_delete_confirm.read() {
                ConfirmModal {
                    title: "Delete Station?".to_string(),
                    message: "This will publish a deletion request to your relays. There is no guarantee that all relays will honor this request or that the station will be permanently removed.".to_string(),
                    confirm_text: Some("Delete".to_string()),
                    cancel_text: Some("Cancel".to_string()),
                    on_confirm: move |_| {
                        show_delete_confirm.set(false);
                        is_deleting.set(true);
                        let coord = station.read().as_ref().map(|s| s.coordinate.clone()).unwrap_or_default();
                        let toast_api = toast;
                        let nav_clone = nav;
                        spawn(async move {
                            match delete_radio_station(&coord).await {
                                Ok(_) => {
                                    toast_api.success(
                                        "Deletion requested".to_string(),
                                        ToastOptions::new()
                                            .description("A deletion request has been sent to your relays")
                                            .duration(Duration::from_secs(3))
                                            .permanent(false),
                                    );
                                    nav_clone.push(Route::RadioHome {});
                                }
                                Err(e) => {
                                    toast_api.error(
                                        "Error".to_string(),
                                        ToastOptions::new()
                                            .description(format!("Failed to delete: {e}"))
                                            .duration(Duration::from_secs(3))
                                            .permanent(false),
                                    );
                                }
                            }
                            is_deleting.set(false);
                        });
                    },
                    on_cancel: move |_| {
                        show_delete_confirm.set(false);
                    },
                }
            }
        }
    }
}
