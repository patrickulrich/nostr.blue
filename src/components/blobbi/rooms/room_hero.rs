use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::blobbi_visual::BlobbiVisual;
use crate::components::blobbi::visual::stat_display::StatDisplay;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlobbiRoom {
    #[default]
    Home,
    Kitchen,
    Care,
    Hatchery,
    Rest,
    Closet,
    Social,
}

impl BlobbiRoom {
    pub fn label(&self) -> &'static str {
        match self {
            BlobbiRoom::Home => "🏠",
            BlobbiRoom::Kitchen => "🍳",
            BlobbiRoom::Care => "🩹",
            BlobbiRoom::Hatchery => "🥚",
            BlobbiRoom::Rest => "🌙",
            BlobbiRoom::Closet => "👗",
            BlobbiRoom::Social => "🌐",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            BlobbiRoom::Home => "Home",
            BlobbiRoom::Kitchen => "Kitchen",
            BlobbiRoom::Care => "Care",
            BlobbiRoom::Hatchery => "Hatchery",
            BlobbiRoom::Rest => "Bedroom",
            BlobbiRoom::Closet => "Closet",
            BlobbiRoom::Social => "Social",
        }
    }

    pub fn all() -> &'static [BlobbiRoom] {
        &[
            BlobbiRoom::Home,
            BlobbiRoom::Kitchen,
            BlobbiRoom::Care,
            BlobbiRoom::Hatchery,
            BlobbiRoom::Rest,
            BlobbiRoom::Closet,
            BlobbiRoom::Social,
        ]
    }
}

#[component]
pub fn RoomHero(blobbi: BlobbiCompanion) -> Element {
    rsx! {
        div { class: "flex flex-col items-center py-4",
            BlobbiVisual { blobbi: blobbi.clone(), size: Some("180".to_string()) }
            h2 { class: "text-lg font-bold text-foreground mt-2",
                "{blobbi.display_name()}"
            }
            span { class: "text-xs text-muted-foreground capitalize",
                "{blobbi.stage.label()}"
            }
            if !blobbi.personality.mood.is_empty() {
                span { class: "text-xs text-muted-foreground ml-1",
                    " · {blobbi.personality.mood}"
                }
            }
            div { class: "mt-3",
                StatDisplay { stats: blobbi.stats, compact: false }
            }
        }
    }
}
