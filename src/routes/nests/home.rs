use crate::components::icons;
use crate::components::nests::NestCard;
use crate::routes::Route;
use crate::stores::nostr_client::{self, CLIENT_INITIALIZED};
use crate::utils::nips::nip53::{
    nest_effective_status, parse_meeting_space, LiveStatus, MeetingSpace,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashMap;

#[component]
pub fn NestsHome() -> Element {
    let mut spaces = use_signal(Vec::<MeetingSpace>::new);
    let mut loading = use_signal(|| true);
    let mut presence_map = use_signal(HashMap::<String, u64>::new);

    use_effect(move || {
        let client_initialized = *CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            let filter = nostr_sdk::Filter::new()
                .kind(nostr_sdk::Kind::Custom(30312))
                .limit(100);
            match nostr_client::fetch_events_aggregated(
                filter,
                std::time::Duration::from_secs(15),
            )
            .await
            {
                Ok(events) => {
                    let mut parsed = Vec::new();
                    for event in events {
                        match parse_meeting_space(&event) {
                            Ok(space) => parsed.push(space),
                            Err(e) => {
                                log::warn!("Failed to parse meeting space: {}", e);
                            }
                        }
                    }
                    parsed.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                    spaces.set(parsed);
                }
                Err(e) => {
                    log::error!("Failed to fetch meeting spaces: {}", e);
                }
            }
            loading.set(false);
        });
    });

    {
        let mut sub_handle: Signal<Option<crate::stores::notification_dispatcher::DispatcherHandle>> =
            use_signal(|| None);
        let mut sub_fallback_id: Signal<Option<nostr_sdk::SubscriptionId>> =
            use_signal(|| None);

        use_effect(move || {
            let current_spaces = spaces.read();
            let coordinates: Vec<String> = current_spaces
                .iter()
                .map(|s| s.coordinate.clone())
                .collect();
            drop(current_spaces);
            let filter = if coordinates.is_empty() {
                return;
            } else {
                nostr_sdk::Filter::new()
                    .kind(nostr_sdk::Kind::Custom(10312))
                    .custom_tags(
                        SingleLetterTag::lowercase(Alphabet::A),
                        coordinates,
                    )
                    .limit(0)
            };
            spawn(async move {
                if let Some(handle) = sub_handle.write().take() {
                    handle.unregister().await;
                }
                if let Some(sid) = sub_fallback_id.write().take() {
                    if let Some(client) = crate::stores::nostr_client::get_client() {
                        let _ = client.unsubscribe(&sid).await;
                    }
                }
                let client = match crate::stores::nostr_client::get_client() {
                    Some(c) => c,
                    None => return,
                };
                match client.subscribe(filter, None).await {
                    Ok(output) => {
                        let sub_id = output.val;
                        if let Some((handle, mut rx)) =
                            crate::stores::notification_dispatcher::DispatcherHandle::create(
                                sub_id.clone(),
                            )
                        {
                            sub_handle.set(Some(handle));
                            spawn(async move {
                                let mut buffer = Vec::new();
                                while let Some(event) = rx.recv().await {
                                    buffer.push(event);
                                    while let Ok(event) = rx.try_recv() {
                                        buffer.push(event);
                                    }
                                    for event in &buffer {
                                        if event.kind.as_u16() == 10312 {
                                            let coordinate = event
                                                .tags
                                                .iter()
                                                .find(|t| {
                                                    t.as_slice()
                                                        .first()
                                                        .map(|s| s.as_str())
                                                        == Some("a")
                                                })
                                                .and_then(|t| t.as_slice().get(1).cloned());
                                            if let Some(coord) = coordinate {
                                                let ts = event.created_at.as_secs();
                                                let mut map = presence_map.write();
                                                let entry = map.entry(coord).or_insert(0);
                                                if ts > *entry {
                                                    *entry = ts;
                                                }
                                            }
                                        }
                                    }
                                    buffer.clear();
                                }
                            });
                        } else {
                            sub_fallback_id.set(Some(sub_id));
                        }
                    }
                    Err(e) => {
                        log::error!("presence subscription failed: {}", e);
                    }
                }
            });
        });
    }

    let mut live_rooms: Vec<(MeetingSpace, LiveStatus, Option<u64>)> = Vec::new();
    let mut scheduled_rooms: Vec<(MeetingSpace, LiveStatus, Option<u64>)> = Vec::new();
    let mut ended_rooms: Vec<(MeetingSpace, LiveStatus, Option<u64>)> = Vec::new();
    {
        let current_spaces = spaces.read();
        let current_presence = presence_map.read();
        for space in current_spaces.iter() {
            let last_presence = current_presence.get(&space.coordinate).copied();
            let status = nest_effective_status(
                space.status,
                last_presence,
                space.created_at,
            );
            match status {
                LiveStatus::Live => live_rooms.push((space.clone(), status, last_presence)),
                LiveStatus::Planned => {
                    scheduled_rooms.push((space.clone(), status, last_presence))
                }
                LiveStatus::Ended => ended_rooms.push((space.clone(), status, last_presence)),
            }
        }
    }

    let has_live = !live_rooms.is_empty();
    let has_scheduled = !scheduled_rooms.is_empty();
    let has_ended = !ended_rooms.is_empty();
    let is_empty = !has_live && !has_scheduled && !has_ended;

    rsx! {
        div { class: "min-h-screen pb-20",
            div { class: "sticky top-0 z-30 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3",
                    h1 { class: "text-xl font-bold", "Nests" }
                    p { class: "text-sm text-muted-foreground", "Live audio rooms" }
                }
            }

            if *loading.read() {
                div { class: "p-4 space-y-4",
                    for _ in 0..6 {
                        div { class: "bg-card border border-border rounded-xl overflow-hidden animate-pulse",
                            div { class: "aspect-video bg-muted" }
                            div { class: "p-3 space-y-2",
                                div { class: "h-4 bg-muted rounded w-3/4" }
                                div { class: "h-3 bg-muted rounded w-1/2" }
                            }
                        }
                    }
                }
            } else {
                div { class: "divide-y divide-border",
                    {if has_live {
                        rsx! {
                            div {
                                div { class: "sticky top-[60px] z-20 bg-background/90 backdrop-blur-sm px-4 py-2 border-b border-border",
                                    h2 { class: "text-sm font-bold text-red-500 uppercase tracking-wider",
                                        "Live Now"
                                    }
                                }
                                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4 p-4",
                                    for (space, status, _last_presence) in live_rooms {
                                        NestCard {
                                            key: "{space.coordinate}",
                                            space: space.clone(),
                                            display_status: status,
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }}

                    {if has_scheduled {
                        rsx! {
                            div {
                                div { class: "sticky top-[60px] z-20 bg-background/90 backdrop-blur-sm px-4 py-2 border-b border-border",
                                    h2 { class: "text-sm font-bold text-blue-500 uppercase tracking-wider",
                                        "Scheduled"
                                    }
                                }
                                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4 p-4",
                                    for (space, status, _) in scheduled_rooms {
                                        NestCard {
                                            key: "{space.coordinate}",
                                            space: space.clone(),
                                            display_status: status,
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }}

                    {if has_ended {
                        rsx! {
                            div {
                                div { class: "sticky top-[60px] z-20 bg-background/90 backdrop-blur-sm px-4 py-2 border-b border-border",
                                    h2 { class: "text-sm font-bold text-muted-foreground uppercase tracking-wider",
                                        "Recently Ended"
                                    }
                                }
                                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4 p-4",
                                    for (space, status, _) in ended_rooms {
                                        NestCard {
                                            key: "{space.coordinate}",
                                            space: space.clone(),
                                            display_status: status,
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }}

                    if is_empty {
                        div { class: "flex flex-col items-center justify-center py-20 text-muted-foreground",
                            icons::RadioIcon {
                                class: "w-16 h-16 mb-4 opacity-30".to_string(),
                            }
                            h3 { class: "text-lg font-medium", "No nests found" }
                            p { class: "text-sm mt-1", "Create one to get started!" }
                        }
                    }
                }
            }

            div { class: "fixed bottom-20 right-4 lg:bottom-6 lg:right-6 z-40 flex flex-col gap-3",
                Link {
                    to: Route::NestServers {},
                    class: "w-12 h-12 bg-muted hover:bg-accent text-muted-foreground rounded-full shadow-lg flex items-center justify-center transition",
                    span {
                        dangerous_inner_html: icons::SETTINGS,
                    }
                }
                Link {
                    to: Route::NestCreate { naddr: None },
                    class: "w-14 h-14 bg-blue-500 hover:bg-blue-600 text-white rounded-full shadow-lg flex items-center justify-center transition",
                    span {
                        dangerous_inner_html: icons::PLUS,
                    }
                }
            }
        }
    }
}
