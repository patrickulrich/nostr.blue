use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::rooms::room_hero::RoomHero;

#[component]
pub fn ClosetRoom(blobbi: BlobbiCompanion) -> Element {
    rsx! {
        div { class: "flex flex-col",
            RoomHero { blobbi: blobbi.clone() }
            div { class: "flex flex-col items-center justify-center py-12 px-4",
                div { class: "text-4xl mb-3", "👗" }
                span { class: "text-sm text-muted-foreground text-center",
                    "Wardrobe coming soon! Dress up your Blobbi with accessories and outfits."
                }
            }
        }
    }
}
