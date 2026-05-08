use dioxus::prelude::*;

use super::care_room::CareRoom;
use super::closet_room::ClosetRoom;
use super::hatchery_room::HatcheryRoom;
use super::home_room::HomeRoom;
use super::kitchen_room::KitchenRoom;
use super::rest_room::RestRoom;
use super::room_config::BlobbiRoomId;
use crate::components::blobbi::core::types::BlobbiCompanion;

#[component]
pub fn RoomShell(blobbi: BlobbiCompanion) -> Element {
    let default_room = if blobbi.is_egg() {
        super::room_config::BlobbiRoomId::Hatchery
    } else {
        super::room_config::BlobbiRoomId::Home
    };
    let mut active_room = use_signal(|| default_room);
    let is_sleeping = blobbi.is_sleeping();
    let mut swipe_start: Signal<Option<(f32, f64)>> = use_signal(|| None);

    rsx! {
        div {
            class: "flex flex-col h-full relative",
            ontouchstart: move |evt: Event<TouchData>| {
                if let Some(touch) = evt.data().touches().first() {
                    swipe_start.set(Some((touch.client_coordinates().x as f32, crate::platform::timestamp::now_millis() as f64)));
                }
            },
            ontouchend: move |evt: Event<TouchData>| {
                if let Some((start_x, start_t)) = swipe_start() {
                    if let Some(touch) = evt.data().touches().first() {
                        let end_x = touch.client_coordinates().x as f32;
                        let dx = end_x - start_x;
                        let dt = crate::platform::timestamp::now_millis() as f64 - start_t;
                        if dt < 500.0 && dx.abs() > 50.0 {
                            if dx > 0.0 {
                                active_room.set(super::room_config::get_previous_room(active_room()));
                            } else {
                                active_room.set(super::room_config::get_next_room(active_room()));
                            }
                        }
                    }
                    swipe_start.set(None);
                }
            },
            div { class: "relative flex items-center justify-center py-2 z-10",
                button {
                    class: "absolute left-2 p-2 rounded-full hover:bg-accent transition",
                    onclick: move |_| active_room.set(super::room_config::get_previous_room(active_room())),
                    svg { class: "size-5 text-muted-foreground", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
                        path { d: "M15 18l-6-6 6-6" }
                    }
                }
                div { class: "flex items-center gap-2",
                    span { class: "text-lg", "{active_room().icon()}" }
                    span { class: "text-sm font-medium text-muted-foreground", "{active_room().label()}" }
                }
                button {
                    class: "absolute right-2 p-2 rounded-full hover:bg-accent transition",
                    onclick: move |_| active_room.set(super::room_config::get_next_room(active_room())),
                    svg { class: "size-5 text-muted-foreground", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
                        path { d: "M9 18l6-6-6-6" }
                    }
                }
            }

            div { class: "flex items-center justify-center gap-1.5 pb-1",
                for room in super::room_config::DEFAULT_ROOM_ORDER.iter() {
                    div {
                        key: "{room:?}",
                        class: if *room == active_room() {
                            "w-4 h-1 rounded-full bg-primary transition-all duration-300"
                        } else {
                            "w-1 h-1 rounded-full bg-muted-foreground/20 transition-all duration-300"
                        },
                    }
                }
            }

            div { class: "flex-1 overflow-y-auto",
                match active_room() {
                    BlobbiRoomId::Home => {
                        let mut ar = active_room;
                        rsx! { HomeRoom {
                            blobbi: blobbi.clone(),
                            on_navigate_to_room: move |room: super::room_config::BlobbiRoomId| ar.set(room),
                        } }
                    }
                    BlobbiRoomId::Kitchen => rsx! { KitchenRoom { blobbi: blobbi.clone() } },
                    BlobbiRoomId::Closet => rsx! { ClosetRoom { blobbi: blobbi.clone() } },
                    BlobbiRoomId::Care => rsx! { CareRoom { blobbi: blobbi.clone() } },
                    BlobbiRoomId::Hatchery => rsx! { HatcheryRoom { blobbi: blobbi.clone() } },
                    BlobbiRoomId::Rest => rsx! { RestRoom { blobbi: blobbi.clone() } },
                }
            }

            if is_sleeping {
                div {
                    class: "absolute inset-0 z-20 pointer-events-none transition-opacity duration-700",
                    style: "background: radial-gradient(ellipse at 50% 40%, rgba(0,0,0,0.2) 0%, rgba(0,0,0,0.4) 100%);",
                }
            }
        }
    }
}
