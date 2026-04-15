use dioxus::prelude::*;

use super::care_room::CareRoom;
use super::closet_room::ClosetRoom;
use super::hatchery_room::HatcheryRoom;
use super::home_room::HomeRoom;
use super::kitchen_room::KitchenRoom;
use super::rest_room::RestRoom;
use super::room_hero::*;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::social::social_room::SocialRoom;

#[component]
pub fn RoomShell(blobbi: BlobbiCompanion) -> Element {
    let mut active_room = use_signal(BlobbiRoom::default);
    let rooms = BlobbiRoom::all();

    rsx! {
        div { class: "flex flex-col h-full",
            div { class: "flex-1 overflow-y-auto",
                match active_room() {
                    BlobbiRoom::Home => rsx! { HomeRoom { blobbi: blobbi.clone() } },
                    BlobbiRoom::Kitchen => rsx! { KitchenRoom { blobbi: blobbi.clone() } },
                    BlobbiRoom::Care => rsx! { CareRoom { blobbi: blobbi.clone() } },
                    BlobbiRoom::Hatchery => rsx! { HatcheryRoom { blobbi: blobbi.clone() } },
                    BlobbiRoom::Rest => rsx! { RestRoom { blobbi: blobbi.clone() } },
                    BlobbiRoom::Closet => rsx! { ClosetRoom { blobbi: blobbi.clone() } },
                    BlobbiRoom::Social => rsx! { SocialRoom { blobbi: blobbi.clone() } },
                }
            }

            div { class: "sticky bottom-0 bg-background border-t border-border px-2 py-1.5 flex justify-around",
                for room in rooms {
                    button {
                        class: if *active_room.read() == *room {
                            "flex flex-col items-center gap-0.5 px-3 py-1 rounded-lg bg-accent transition"
                        } else {
                            "flex flex-col items-center gap-0.5 px-3 py-1 rounded-lg hover:bg-accent transition"
                        },
                        onclick: {
                            let room = *room;
                            move |_| active_room.set(room)
                        },
                        span { class: "text-lg", "{room.label()}" }
                        span { class: "text-[10px] text-muted-foreground", "{room.name()}" }
                    }
                }
            }
        }
    }
}
