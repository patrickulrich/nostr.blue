use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::rooms::room_hero::RoomHero;

#[component]
pub fn RestRoom(blobbi: BlobbiCompanion) -> Element {
    let sleeping = blobbi.is_sleeping();

    rsx! {
        div { class: "flex flex-col",
            RoomHero { blobbi: blobbi.clone() }
            div { class: "px-4 mt-4 flex flex-col items-center gap-4",
                div { class: "text-sm text-muted-foreground text-center",
                    if sleeping {
                        "Your Blobbi is sleeping peacefully..."
                    } else {
                        "Put your Blobbi to bed to restore energy"
                    }
                }
                if sleeping {
                    div { class: "text-4xl animate-[blobbi-sleep-breathe_3s_ease-in-out_infinite]",
                        "\u{1F4A4}"
                    }
                    span { class: "text-xs text-green-500",
                        "Energy regenerating"
                    }
                }
                span { class: "text-xs text-muted-foreground",
                    "Energy: {blobbi.stats.energy:.0}/100"
                }
            }
        }
    }
}
