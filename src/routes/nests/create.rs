use crate::components::icons;
use crate::components::ConfirmModal;
use crate::routes::Route;
use crate::stores::auth_store;
use crate::stores::nostr_client;
use crate::utils::nip19::parse_naddr;
use crate::utils::nips::nip53::{
    add_meeting_space_optional_tags, build_meeting_space_tags, parse_meeting_space,
    rebuild_meeting_space_tags, MeetingSpace, RoomStatus,
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
                            is_live_now.set(ms.status == RoomStatus::Open);
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
            let status = if live_now {
                RoomStatus::Open
            } else {
                RoomStatus::Private
            };
            let tags = if let Some(ref ms) = existing_ms {
                let mut rebuilt = rebuild_meeting_space_tags(ms, status);
                let title_idx = rebuilt.iter().position(|t| {
                    t.as_slice().first().map(|s| s.as_str()) == Some("title")
                });
                if let Some(idx) = title_idx {
                    rebuilt[idx] = Tag::custom(TagKind::custom("title"), [&*name_val]);
                }
                let summary_idx = rebuilt.iter().position(|t| {
                    t.as_slice().first().map(|s| s.as_str()) == Some("summary")
                });
                match (summary_idx, summary_val.trim().is_empty()) {
                    (Some(idx), true) => { rebuilt.remove(idx); }
                    (Some(idx), false) => {
                        rebuilt[idx] = Tag::custom(
                            TagKind::custom("summary"),
                            [summary_val.trim()],
                        );
                    }
                    (None, false) => {
                        rebuilt.push(Tag::custom(
                            TagKind::custom("summary"),
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
                        rebuilt[idx] = Tag::custom(
                            TagKind::custom("image"),
                            [image_val.trim()],
                        );
                    }
                    (None, false) => {
                        rebuilt.push(Tag::custom(
                            TagKind::custom("image"),
                            [image_val.trim()],
                        ));
                    }
                    (None, true) => {}
                }
                rebuilt
            } else {
                let auth_url = "https://moq-auth.nostrnests.com";
                let streaming_url = "https://moq.nostrnests.com:4443";
                let mut tags = build_meeting_space_tags(
                    &d_tag,
                    &name_val,
                    status,
                    auth_url,
                    &pubkey,
                );
                add_meeting_space_optional_tags(
                    &mut tags,
                    Some(streaming_url),
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
                    None,
                    None,
                    &[],
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
                    nav.push(Route::NestDetail { naddr: naddr_str });
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
                                Route::NestDetail { naddr: n.clone() }
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
                        div { class: "pt-2",
                            button {
                                r#type: "submit",
                                class: "w-full py-3 bg-blue-500 hover:bg-blue-600 text-white font-bold rounded-xl transition disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *is_submitting.read()
                                    || room_name.read().trim().is_empty(),
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
        }
    }
}
