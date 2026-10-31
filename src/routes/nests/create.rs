use crate::components::icons;
use crate::components::ConfirmModal;
use crate::routes::Route;
use crate::stores::auth_store;
use crate::stores::nostr_client::{self, CLIENT_INITIALIZED};
use crate::utils::nip19::parse_naddr;
use crate::utils::nips::nip53::{
    add_meeting_space_optional_tags, build_meeting_space_tags, parse_meeting_space,
    parse_nests_servers, rebuild_meeting_space_tags, MeetingSpace, RoomStatus,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

#[component]
pub fn NestCreate(naddr: Option<String>) -> Element {
    let nav = navigator();
    let is_logged_in = auth_store::get_pubkey().is_some();
    let mut room_name = use_signal(String::new);
    let mut summary = use_signal(String::new);
    let mut image_url = use_signal(String::new);
    let mut is_live_now = use_signal(|| true);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut edit_d_tag = use_signal(|| None::<String>);
    let mut is_edit_mode = use_signal(|| false);
    let mut loading_edit = use_signal(|| false);
    let mut edit_meeting_space = use_signal(|| None::<MeetingSpace>);
    let mut show_close_confirm = use_signal(|| false);
    let mut show_seed_dialog = use_signal(|| false);
    let mut scheduled_start = use_signal(String::new);

    use_effect(use_reactive((&naddr,), move |(naddr,)| {
        if let Some(ref naddr_str) = naddr {
            let naddr_str = naddr_str.clone();
            is_edit_mode.set(true);
            loading_edit.set(true);
            spawn(async move {
                let parsed = match parse_naddr(&naddr_str) {
                    Ok(p) => p,
                    Err(e) => {
                        error.set(Some(format!("Invalid room address: {}", e)));
                        loading_edit.set(false);
                        return;
                    }
                };
                match nostr_client::fetch_event_by_coordinate_with_relays(
                    parsed.kind,
                    parsed.pubkey.clone(),
                    parsed.identifier,
                    parsed.relay_hints,
                )
                .await
                {
                    Ok(Some(event)) => match parse_meeting_space(&event) {
                        Ok(ms) => {
                            let my_pk = auth_store::get_pubkey();
                            if my_pk.as_ref() != Some(&ms.pubkey) {
                                error.set(Some(
                                    "You can only edit your own rooms".to_string(),
                                ));
                                loading_edit.set(false);
                                return;
                            }
                            room_name.set(ms.room_name.clone());
                            summary.set(ms.summary.clone().unwrap_or_default());
                            image_url.set(ms.image.clone().unwrap_or_default());
                            // Seed the toggle from live-ness only: Open and
                            // Private rooms are live (Private must stay
                            // private on save); Planned and Closed rooms are
                            // not. `!matches!(Planned)` would flip Closed
                            // rooms live and silently drop Private access
                            // restrictions on save.
                            is_live_now.set(matches!(
                                ms.status,
                                RoomStatus::Open | RoomStatus::Private
                            ));
                            if let Some(starts) = ms.starts {
                                scheduled_start.set(unix_to_datetime_local(starts));
                            }
                            edit_d_tag.set(Some(ms.d_tag.clone()));
                            edit_meeting_space.set(Some(ms));
                            loading_edit.set(false);
                        }
                        Err(e) => {
                            error.set(Some(format!("Failed to parse room: {}", e)));
                            loading_edit.set(false);
                        }
                    },
                    Ok(None) => {
                        error.set(Some("Room not found".to_string()));
                        loading_edit.set(false);
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to load room: {}", e)));
                        loading_edit.set(false);
                    }
                }
            });
        }
    }));

    // Phase 3.4: First-run server seed check. When creating (not editing) and
    // the user has no kind 10112 server list, offer to seed with the public
    // nostrnests.com defaults (the public audio-server defaults).
    use_effect(use_reactive(&*CLIENT_INITIALIZED.read(), move |ready| {
        if !ready || *is_edit_mode.read() {
            return;
        }
        let Some(pk_hex) = auth_store::get_pubkey() else { return };
        let Ok(author) = PublicKey::from_hex(&pk_hex) else { return };
        spawn(async move {
            let filter = nostr::filter::Filter::new()
                .kind(Kind::Custom(10112))
                .author(author)
                .limit(1);
            let has_servers = match nostr_client::fetch_events_aggregated(
                filter,
                std::time::Duration::from_secs(3),
            )
            .await
            {
                Ok(events) => events
                    .first()
                    .map(|e| !parse_nests_servers(e).is_empty())
                    .unwrap_or(false),
                Err(_) => false,
            };
            if !has_servers {
                show_seed_dialog.set(true);
            }
        });
    }));

    let handle_seed_confirm = move |_| {
        show_seed_dialog.set(false);
        spawn(async move {
            let default_auth = "https://moq-auth.nostrnests.com".to_string();
            let default_relay = "https://moq.nostrnests.com:4443".to_string();
            let tags = crate::utils::nips::nip53::build_nests_servers_tags(&[
                crate::utils::nips::nip53::NestsServer {
                    relay_url: default_relay,
                    auth_url: default_auth,
                },
            ]);
            let builder = EventBuilder::new(Kind::Custom(10112), "").tags(tags);
            if let Ok(event) =
                crate::stores::publish_queue::signing::sign_event_builder(builder).await
            {
                let _ = crate::stores::publish_queue::enqueue(
                    event,
                    crate::stores::publish_queue::types::QueueEventType::Other(
                        "nest-servers".to_string(),
                    ),
                    None,
                    std::collections::HashMap::new(),
                )
                .await;
            }
        });
    };

    if !is_logged_in {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center",
                div { class: "text-center space-y-4",
                    div { class: "w-16 h-16 rounded-full bg-muted flex items-center justify-center mx-auto",
                        span {
                            class: "text-muted-foreground",
                            dangerous_inner_html: icons::LOCK,
                        }
                    }
                    h1 { class: "text-xl font-bold", "Authentication Required" }
                    p { class: "text-muted-foreground", "Please log in to create a nest" }
                    Link {
                        to: Route::NestsHome {},
                        class: "inline-block mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                        "Back to Nests"
                    }
                }
            }
        };
    }

    let is_edit = *is_edit_mode.read();
    let edit_d_tag_val = edit_d_tag.read().clone();

    let mut handle_submit = move |_| {
        if *is_submitting.read() {
            return;
        }
        let name_val = room_name.read().clone();
        if name_val.trim().is_empty() {
            error.set(Some("Room name is required".to_string()));
            return;
        }
        is_submitting.set(true);
        error.set(None);
        let summary_val = summary.read().clone();
        let image_val = image_url.read().clone();
        let live_now = *is_live_now.read();
        let scheduled_start_val = scheduled_start.read().clone();
        let edit_d = edit_d_tag_val.clone();
        let existing_ms = edit_meeting_space.read().clone();
        spawn(async move {
            let pubkey = match auth_store::get_pubkey() {
                Some(pk) => pk,
                None => {
                    error.set(Some("No active account".to_string()));
                    is_submitting.set(false);
                    return;
                }
            };
            let d_tag = edit_d.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            // EGG-08: scheduled rooms carry `status=planned` + a required
            // `starts` unix timestamp. A planned room without `starts` is
            // un-renderable per spec (and Amethyst sorts it to the bottom of
            // the SCHEDULED bucket with no time shown).
            // Status preservation on edit: a Private room must not silently
            // become a public live room because the host fixed a typo, and a
            // Closed room must stay closed unless the host explicitly goes
            // live (or re-schedules) again.
            let prev_status = existing_ms.as_ref().map(|ms| ms.status);
            let (status, starts_secs) = if live_now {
                match prev_status {
                    Some(RoomStatus::Private) => (RoomStatus::Private, None),
                    _ => (RoomStatus::Open, None),
                }
            } else if prev_status == Some(RoomStatus::Closed) {
                // Metadata-only edit of a closed room: keep it closed; no
                // `starts` is required to fix a title or summary.
                (RoomStatus::Closed, None)
            } else {
                match datetime_local_to_unix(&scheduled_start_val) {
                    Some(ts) => {
                        // EGG-08: the future check guards new/edited
                        // schedules only — a host editing unrelated metadata
                        // of a planned room whose start already passed must
                        // still be able to save. Compare at minute
                        // resolution: the datetime-local round-trip
                        // (unix → HH:MM → unix) truncates seconds.
                        let starts_unchanged = existing_ms
                            .as_ref()
                            .and_then(|ms| ms.starts)
                            .map(|prev| prev / 60 == ts / 60)
                            .unwrap_or(false);
                        if ts <= crate::platform::timestamp::now_secs() && !starts_unchanged {
                            error.set(Some(
                                "Scheduled start time must be in the future".to_string(),
                            ));
                            is_submitting.set(false);
                            return;
                        }
                        (RoomStatus::Planned, Some(ts))
                    }
                    None => {
                        error.set(Some(
                            "Please pick a valid start date and time".to_string(),
                        ));
                        is_submitting.set(false);
                        return;
                    }
                }
            };
            let tags = if let Some(ref ms) = existing_ms {
                let mut rebuilt = rebuild_meeting_space_tags(ms, status);
                let title_idx = rebuilt.iter().position(|t| {
                    t.as_slice().first().map(|s| s.as_str()) == Some("title")
                });
                if let Some(idx) = title_idx {
                    rebuilt[idx] = Tag::custom("title",
                        [name_val.trim()],
                    );
                } else {
                    rebuilt.push(Tag::custom("title",
                        [name_val.trim()],
                    ));
                }
                let summary_idx = rebuilt.iter().position(|t| {
                    t.as_slice().first().map(|s| s.as_str()) == Some("summary")
                });
                match (summary_idx, summary_val.trim().is_empty()) {
                    (Some(idx), true) => { rebuilt.remove(idx); }
                    (Some(idx), false) => {
                        rebuilt[idx] = Tag::custom("summary",
                            [summary_val.trim()],
                        );
                    }
                    (None, false) => {
                        rebuilt.push(Tag::custom("summary",
                            [summary_val.trim()],
                        ));
                    }
                    (None, true) => {}
                }
                let image_idx = rebuilt.iter().position(|t| {
                    t.as_slice().first().map(|s| s.as_str()) == Some("image")
                });
                match (image_idx, image_val.trim().is_empty()) {
                    (Some(idx), true) => { rebuilt.remove(idx); }
                    (Some(idx), false) => {
                        rebuilt[idx] = Tag::custom("image",
                            [image_val.trim()],
                        );
                    }
                    (None, false) => {
                        rebuilt.push(Tag::custom("image",
                            [image_val.trim()],
                        ));
                    }
                    (None, true) => {}
                }
                // Keep the `starts` tag in sync with the schedule selection.
                let starts_idx = rebuilt.iter().position(|t| {
                    t.as_slice().first().map(|s| s.as_str()) == Some("starts")
                });
                match (starts_idx, starts_secs) {
                    (Some(idx), Some(ts)) => {
                        rebuilt[idx] = Tag::custom("starts", [ts.to_string()]);
                    }
                    (Some(idx), None) => { rebuilt.remove(idx); }
                    (None, Some(ts)) => {
                        rebuilt.push(Tag::custom("starts", [ts.to_string()]));
                    }
                    (None, None) => {}
                }
                rebuilt
            } else {
                let (auth_url, streaming_url) = {
                    let default_auth = "https://moq-auth.nostrnests.com".to_string();
                    let default_streaming = "https://moq.nostrnests.com:4443".to_string();
                    if let Some(pk) = auth_store::get_pubkey() {
                        if let Ok(author) = PublicKey::from_hex(&pk) {
                            let filter = nostr::filter::Filter::new()
                                .kind(Kind::Custom(10112))
                                .author(author)
                                .limit(1);
                            if let Ok(events) = nostr_client::fetch_events_aggregated(
                                filter,
                                std::time::Duration::from_secs(5),
                            )
                            .await
                            {
                                if let Some(event) = events.first() {
                                    let servers = parse_nests_servers(event);
                                    if let Some(server) = servers.first() {
                                        (
                                            server.auth_url.clone(),
                                            Some(server.relay_url.clone()),
                                        )
                                    } else {
                                        (default_auth, Some(default_streaming))
                                    }
                                } else {
                                    (default_auth, Some(default_streaming))
                                }
                            } else {
                                (default_auth, Some(default_streaming))
                            }
                        } else {
                            (default_auth, Some(default_streaming))
                        }
                    } else {
                        (default_auth, Some(default_streaming))
                    }
                };
                // EGG-01 rule 10: publish the room to the `relays` tag ∪ the
                // NIP-65 outbox, and emit the tag itself so other clients can
                // target their room-update / presence / chat subscriptions at
                // it (Amethyst unions the room's `relays` tag into its
                // in-room subscription set). Seed from the user's NIP-65
                // write relays, falling back to the default pool.
                let room_relays: Vec<String> = {
                    let metadata = crate::stores::relay::nip65::USER_RELAY_METADATA.peek();
                    let mut urls: Vec<String> = metadata
                        .as_ref()
                        .map(|m| {
                            m.relays
                                .iter()
                                .filter(|r| r.write && r.url.starts_with("wss://"))
                                .map(|r| r.url.trim_end_matches('/').to_string())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if urls.is_empty() {
                        urls = crate::stores::relay::DEFAULT_RELAYS
                            .iter()
                            .map(|s| s.to_string())
                            .collect();
                    }
                    let mut seen = std::collections::HashSet::new();
                    urls.retain(|u| seen.insert(u.clone()));
                    urls.truncate(5);
                    urls
                };
                let mut tags = build_meeting_space_tags(
                    &d_tag,
                    &name_val,
                    status,
                    &auth_url,
                    &pubkey,
                );
                add_meeting_space_optional_tags(
                    &mut tags,
                    streaming_url.as_deref(),
                    if summary_val.trim().is_empty() {
                        None
                    } else {
                        Some(summary_val.trim())
                    },
                    if image_val.trim().is_empty() {
                        None
                    } else {
                        Some(image_val.trim())
                    },
                    starts_secs,
                    None,
                    &room_relays,
                );
                tags
            };
            let builder = EventBuilder::new(Kind::Custom(30312), "").tags(tags);
            match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
                Ok(event) => {
                    crate::stores::publish_queue::enqueue(
                        event.clone(),
                        crate::stores::publish_queue::types::QueueEventType::Other(
                            "nest".to_string(),
                        ),
                        None,
                        std::collections::HashMap::new(),
                    )
                    .await;
                    let pk = event.pubkey.to_hex();
                    let coord = Coordinate::new(Kind::Custom(30312), event.pubkey)
                        .identifier(&d_tag);
                    let naddr_str = Nip19Coordinate::new(coord, vec![])
                        .to_bech32()
                        .unwrap_or_else(|_| format!("30312:{}:{}", pk, d_tag));
                    nav.push(Route::AddressViewer { address: naddr_str });
                }
                Err(e) => {
                    log::error!("Failed to sign nest event: {}", e);
                    error.set(Some(format!("Failed to {} room: {}", if is_edit { "update" } else { "create" }, e)));
                    is_submitting.set(false);
                }
            }
        });
    };

    let handle_close_room = {
        let existing_ms = edit_meeting_space.read().clone();
        let nav = navigator();
        move |_| {
            show_close_confirm.set(false);
            let ms = match existing_ms {
                Some(ref ms) => ms.clone(),
                None => return,
            };
            spawn(async move {
                let tags = rebuild_meeting_space_tags(&ms, RoomStatus::Closed);
                let builder = EventBuilder::new(Kind::Custom(30312), "").tags(tags);
                match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
                    Ok(event) => {
                        crate::stores::publish_queue::enqueue(
                            event,
                            crate::stores::publish_queue::types::QueueEventType::Other(
                                "nest".to_string(),
                            ),
                            None,
                            std::collections::HashMap::new(),
                        )
                        .await;
                        nav.push(Route::NestsHome {});
                    }
                    Err(e) => {
                        log::error!("Failed to close room: {}", e);
                    }
                }
            });
        }
    };

    let page_title = if is_edit { "Edit Nest" } else { "Create Nest" };
    let can_close = is_edit
        && edit_meeting_space
            .read()
            .as_ref()
            .map(|ms| ms.status != RoomStatus::Closed)
            .unwrap_or(false);

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-30 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-3",
                    Link {
                        to: if is_edit {
                            if let Some(ref n) = naddr {
                                Route::AddressViewer { address: n.clone() }
                            } else {
                                Route::NestsHome {}
                            }
                        } else {
                            Route::NestsHome {}
                        },
                        class: "p-2 hover:bg-muted rounded-lg transition",
                        span {
                            dangerous_inner_html: icons::ARROW_LEFT,
                        }
                    }
                    h1 { class: "text-lg font-bold", "{page_title}" }
                }
            }
            div { class: "p-4 max-w-xl mx-auto",
                if *loading_edit.read() {
                    div { class: "flex items-center justify-center py-12",
                        div { class: "animate-pulse text-muted-foreground", "Loading room..." }
                    }
                } else {
                    if let Some(err) = error.read().as_ref() {
                        div { class: "mb-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                            "{err}"
                        }
                    }
                    form {
                        class: "space-y-6",
                        onsubmit: move |e| {
                            e.prevent_default();
                            handle_submit(());
                        },
                        div { class: "space-y-2",
                            label { class: "block text-sm font-medium", r#for: "room-name", "Room Name *" }
                            input {
                                id: "room-name",
                                r#type: "text",
                                class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                placeholder: "e.g., Friday Hangout",
                                value: "{room_name}",
                                oninput: move |e| room_name.set(e.value()),
                            }
                        }
                        div { class: "space-y-2",
                            label { class: "block text-sm font-medium", r#for: "summary", "Summary" }
                            textarea {
                                id: "summary",
                                class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary min-h-[80px]",
                                placeholder: "What's this room about?",
                                value: "{summary}",
                                oninput: move |e| summary.set(e.value()),
                            }
                        }
                        div { class: "space-y-2",
                            label { class: "block text-sm font-medium", r#for: "image-url", "Cover Image URL" }
                            input {
                                id: "image-url",
                                r#type: "url",
                                class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                placeholder: "https://example.com/cover.jpg",
                                value: "{image_url}",
                                oninput: move |e| image_url.set(e.value()),
                            }
                        }
                        div { class: "space-y-3",
                            label { class: "block text-sm font-medium", "Room Mode" }
                            div { class: "flex gap-3",
                                button {
                                    r#type: "button",
                                    class: if *is_live_now.read() {
                                        "flex-1 py-2.5 px-4 rounded-lg font-medium text-sm bg-blue-500 text-white"
                                    } else {
                                        "flex-1 py-2.5 px-4 rounded-lg font-medium text-sm bg-muted text-muted-foreground hover:bg-accent transition"
                                    },
                                    onclick: move |_| is_live_now.set(true),
                                    "Live Now"
                                }
                                button {
                                    r#type: "button",
                                    class: if !*is_live_now.read() {
                                        "flex-1 py-2.5 px-4 rounded-lg font-medium text-sm bg-blue-500 text-white"
                                    } else {
                                        "flex-1 py-2.5 px-4 rounded-lg font-medium text-sm bg-muted text-muted-foreground hover:bg-accent transition"
                                    },
                                    onclick: move |_| is_live_now.set(false),
                                    "Scheduled"
                                }
                            }
                        }
                        if !*is_live_now.read() {
                            div { class: "space-y-2",
                                label { class: "block text-sm font-medium", r#for: "scheduled-start", "Start Date & Time *" }
                                input {
                                    id: "scheduled-start",
                                    r#type: "datetime-local",
                                    class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    value: "{scheduled_start}",
                                    oninput: move |e| scheduled_start.set(e.value()),
                                }
                                p { class: "text-xs text-muted-foreground",
                                    "Scheduled rooms are listed under Scheduled and become joinable when you flip the room to live"
                                }
                            }
                        }
                        div { class: "pt-2",
                            button {
                                r#type: "submit",
                                class: "w-full py-3 bg-blue-500 hover:bg-blue-600 text-white font-bold rounded-xl transition disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *is_submitting.read()
                                    || room_name.read().trim().is_empty()
                                    || (!*is_live_now.read()
                                        && scheduled_start.read().trim().is_empty()),
                                if *is_submitting.read() {
                                    if is_edit { "Saving..." } else { "Creating..." }
                                } else if is_edit {
                                    "Save Changes"
                                } else if *is_live_now.read() {
                                    "Go Live"
                                } else {
                                    "Schedule Room"
                                }
                            }
                        }
                        if can_close {
                            div { class: "pt-4 border-t border-border",
                                button {
                                    r#type: "button",
                                    class: "w-full py-2.5 text-destructive hover:bg-destructive/10 rounded-xl transition text-sm font-medium",
                                    disabled: *is_submitting.read(),
                                    onclick: move |_| show_close_confirm.set(true),
                                    "Close Room"
                                }
                                p { class: "text-xs text-muted-foreground text-center mt-1",
                                    "This will end the room for all participants"
                                }
                            }
                        }
                    }
                }
            }
            if *show_close_confirm.read() {
                ConfirmModal {
                    title: "Close Room?".to_string(),
                    message: "All attendees will be disconnected. The room will show as CLOSED in the feed.".to_string(),
                    confirm_text: Some("Close Room".to_string()),
                    cancel_text: Some("Cancel".to_string()),
                    on_confirm: handle_close_room,
                    on_cancel: move |_| show_close_confirm.set(false),
                }
            }

            if *show_seed_dialog.read() {
                ConfirmModal {
                    title: "Set Up Audio Servers?".to_string(),
                    message: "You don't have audio servers configured yet. Use the public nostrnests.com defaults? You can change these later in Settings.".to_string(),
                    confirm_text: Some("Use Defaults".to_string()),
                    cancel_text: Some("Skip".to_string()),
                    on_confirm: handle_seed_confirm,
                    on_cancel: move |_| show_seed_dialog.set(false),
                }
            }
        }
    }
}

/// Parse an HTML `datetime-local` value ("YYYY-MM-DDTHH:MM") into unix
/// seconds. The input's semantics are local time, so convert via the
/// platform's local clock.
#[cfg(feature = "web")]
fn datetime_local_to_unix(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // `new Date("YYYY-MM-DDTHH:MM[:SS]")` without a timezone suffix is
    // parsed as LOCAL time per the ES spec — matching the input type.
    // `datetime-local` values are "YYYY-MM-DDTHH:MM", but inputs with
    // second-granularity steps submit "…THH:MM:SS" — only append the
    // seconds slot when it's missing, else "…:SS:00" parses to NaN.
    let padded = if s.matches(':').count() == 1 {
        format!("{s}:00")
    } else {
        s.to_string()
    };
    let js_date = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(&padded));
    let ms = js_date.get_time();
    if ms.is_nan() {
        return None;
    }
    Some((ms / 1000.0) as u64)
}

/// Format a unix-seconds timestamp as an HTML `datetime-local` value
/// ("YYYY-MM-DDTHH:MM") in local time.
#[cfg(feature = "web")]
fn unix_to_datetime_local(ts: u64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ts as f64 * 1000.0));
    // get_month() is 0-based.
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}",
        d.get_full_year(),
        d.get_month() + 1,
        d.get_date(),
        d.get_hours(),
        d.get_minutes()
    )
}

#[cfg(not(feature = "web"))]
fn datetime_local_to_unix(s: &str) -> Option<u64> {
    let s = s.trim();
    // Accept both minute- and second-granularity `datetime-local` values.
    let dt = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .ok()?;
    use chrono::TimeZone;
    chrono::Local
        .from_local_datetime(&dt)
        .single()
        .map(|dt| dt.timestamp().max(0) as u64)
}

#[cfg(not(feature = "web"))]
fn unix_to_datetime_local(ts: u64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_opt(ts as i64, 0).single() {
        Some(dt) => dt.format("%Y-%m-%dT%H:%M").to_string(),
        None => String::new(),
    }
}
