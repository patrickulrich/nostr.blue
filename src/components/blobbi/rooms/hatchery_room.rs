use dioxus::prelude::*;

use crate::components::blobbi::actions::missions_modal::MissionsModal;
use crate::components::blobbi::actions::tasks_panel::TasksPanel;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::rooms::room_hero::RoomHero;

#[component]
pub fn HatcheryRoom(blobbi: BlobbiCompanion) -> Element {
    let mut show_missions = use_signal(|| false);

    rsx! {
        div { class: "flex flex-col",
            RoomHero { blobbi: blobbi.clone() }
            div { class: "px-4 mt-2",
                if blobbi.is_egg() {
                    div { class: "space-y-3",
                        div { class: "text-sm font-medium",
                            "Incubation"
                        }
                        span { class: "text-xs text-muted-foreground",
                            "Care for your egg to help it hatch"
                        }
                    }
                }

                TasksPanel { blobbi: blobbi.clone() }

                button {
                    class: "w-full mt-3 flex items-center justify-center gap-2 py-2.5 rounded-xl bg-purple-500/10 border border-purple-500/20 text-purple-500 text-sm font-medium hover:bg-purple-500/20 transition",
                    onclick: move |_| show_missions.set(true),
                    span { "\u{1F4CB}" }
                    span { "Daily Missions" }
                }
            }
        }

        if show_missions() {
            MissionsModal {
                blobbi: blobbi.clone(),
                on_close: move |_| show_missions.set(false),
            }
        }
    }
}
