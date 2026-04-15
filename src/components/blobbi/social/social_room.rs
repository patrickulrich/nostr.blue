use dioxus::prelude::*;

use crate::components::blobbi::actions::action_types::BlobbiActionType;
use crate::components::blobbi::actions::care_actions::execute_blobbi_action;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::rooms::room_hero::RoomHero;
use crate::components::blobbi::social::breeding_modal::BreedingModal;
use crate::components::blobbi::social::photo_modal::PhotoModal;
use crate::components::blobbi::social::records_modal::RecordsModal;

#[component]
pub fn SocialRoom(blobbi: BlobbiCompanion) -> Element {
    let mut show_breeding = use_signal(|| false);
    let mut show_records = use_signal(|| false);
    let mut show_photo = use_signal(|| false);

    rsx! {
        div { class: "flex flex-col",
            RoomHero { blobbi: blobbi.clone() }

            div { class: "px-4 mt-2 space-y-3",
                div { class: "text-xs text-muted-foreground mb-2",
                    "Social"
                }

                div { class: "grid grid-cols-2 gap-2",
                    if blobbi.is_adult() {
                        button {
                            class: "flex flex-col items-center gap-1 p-3 rounded-xl bg-card border border-border hover:bg-accent transition",
                            onclick: move |_| show_breeding.set(true),
                            span { class: "text-2xl", "🧬" }
                            span { class: "text-xs font-medium", "Breed" }
                            span { class: "text-[10px] text-muted-foreground",
                                "Find a mate"
                            }
                        }
                    }

                    button {
                        class: "flex flex-col items-center gap-1 p-3 rounded-xl bg-card border border-border hover:bg-accent transition",
                        onclick: move |_| show_records.set(true),
                        span { class: "text-2xl", "📜" }
                        span { class: "text-xs font-medium", "Records" }
                        span { class: "text-[10px] text-muted-foreground",
                            "Pet history"
                        }
                    }

                    button {
                        class: "flex flex-col items-center gap-1 p-3 rounded-xl bg-card border border-border hover:bg-accent transition",
                        onclick: move |_| show_photo.set(true),
                        span { class: "text-2xl", "📸" }
                        span { class: "text-xs font-medium", "Photo" }
                        span { class: "text-[10px] text-muted-foreground",
                            "Capture moment"
                        }
                    }

                    button {
                        class: "flex flex-col items-center gap-1 p-3 rounded-xl bg-card border border-border hover:bg-accent transition",
                        onclick: {
                            let blobbi = blobbi.clone();
                            move |_| {
                                let b = blobbi.clone();
                                spawn(async move {
                                    let _ = execute_blobbi_action(&b, BlobbiActionType::Sing).await;
                                });
                            }
                        },
                        span { class: "text-2xl", "🎤" }
                        span { class: "text-xs font-medium", "Sing" }
                        span { class: "text-[10px] text-muted-foreground",
                            "Happiness +15"
                        }
                    }
                }

                if blobbi.is_adult() {
                    div { class: "mt-3 p-3 rounded-xl bg-card border border-border",
                        div { class: "text-xs font-medium mb-1", "Breeding Status" }
                        div { class: "text-xs text-muted-foreground",
                            if blobbi.breeding_ready {
                                "Ready to breed!"
                            } else {
                                "Not ready — keep caring for your pet"
                            }
                        }
                        div { class: "text-xs text-muted-foreground mt-1",
                            "Generation: {blobbi.generation}"
                        }
                    }
                }
            }
        }

        if show_breeding() {
            BreedingModal {
                blobbi: blobbi.clone(),
                on_close: move |_| show_breeding.set(false),
            }
        }

        if show_records() {
            RecordsModal {
                blobbi: blobbi.clone(),
                on_close: move |_| show_records.set(false),
            }
        }

        if show_photo() {
            PhotoModal {
                blobbi: blobbi.clone(),
                on_close: move |_| show_photo.set(false),
            }
        }
    }
}
