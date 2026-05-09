use dioxus::prelude::*;

use crate::components::blobbi::actions::missions_modal::MissionsModal;
use crate::components::blobbi::actions::stage_transition;
use crate::components::blobbi::actions::tasks_panel::TasksPanel;
use crate::components::blobbi::core::builders::publish_blobbi_state;
use crate::components::blobbi::core::types::{BlobbiCompanion, BlobbiState};
use crate::components::blobbi::onboarding::hatching_ceremony::HatchingCeremony;
use crate::components::blobbi::rooms::blobbi_selector::BlobbiSelector;
use crate::components::blobbi::rooms::room_action_button::RoomActionButton;
use crate::components::blobbi::rooms::room_hero::RoomHero;
use crate::components::blobbi::social::breeding_modal::BreedingModal;
use crate::components::blobbi::social::records_modal::RecordsModal;
use crate::stores::blobbi::blobbi_store;

#[component]
pub fn HatcheryRoom(blobbi: BlobbiCompanion) -> Element {
    let mut show_missions = use_signal(|| false);
    let mut show_selector = use_signal(|| false);
    let mut show_ceremony = use_signal(|| false);
    let mut show_breeding = use_signal(|| false);
    let mut show_records = use_signal(|| false);
    let mut busy = use_signal(|| false);

    let is_incubating = blobbi.state == BlobbiState::Incubating;
    let is_evolving = blobbi.state == BlobbiState::Evolving;
    let has_active_process = is_incubating || is_evolving;

    let can_transition = stage_transition::can_transition(&blobbi);
    let completed_count = blobbi.tasks.iter().filter(|t| t.completed).count();
    let total_tasks = crate::components::blobbi::actions::hatch_tasks::tasks_for_stage(blobbi.stage).len();
    let all_done = completed_count >= total_tasks && total_tasks > 0;

    let ready_to_transition = has_active_process && all_done && can_transition;

    if show_ceremony() {
        return rsx! {
            HatchingCeremony {
                blobbi: Some(blobbi.clone()),
                egg_only: false,
                on_complete: move |_name| {
                    show_ceremony.set(false);
                },
            }
        };
    }

    let reaction = crate::components::blobbi::rooms::reaction_state::reaction_string();

    rsx! {
        div { class: "flex flex-col min-h-full",
            RoomHero { blobbi: blobbi.clone(), reaction: reaction.clone() }

            div { class: "px-4 mt-2",
                TasksPanel { blobbi: blobbi.clone() }
            }

            div { class: "flex-1" }

            div { class: "px-4 pb-4 pt-3 border-t border-border/50",
                div { class: "flex items-center justify-between gap-2",

                    RoomActionButton {
                        icon: rsx! { span { class: "text-2xl sm:text-3xl", "🥚" } },
                        label: "Blobbis".to_string(),
                        color: "bg-pink-500/10".to_string(),
                        glow_hex: "#ec4899".to_string(),
                        onclick: move |_| {
                            show_selector.set(true);
                        },
                        disabled: Some(false),
                    }

                    div { class: "flex-1 min-w-0 flex flex-col items-center gap-2",
                        if ready_to_transition {
                            button {
                                class: if busy() {
                                    "w-full py-3 rounded-xl font-medium text-sm text-white bg-green-600 cursor-not-allowed"
                                } else {
                                    "w-full py-3 rounded-xl font-medium text-sm text-white bg-green-500 hover:bg-green-600 transition active:scale-95"
                                },
                                disabled: busy(),
                                onclick: {
                                    move |_| {
                                        show_ceremony.set(true);
                                    }
                                },
                                if busy() {
                                    "..."
                                } else if blobbi.is_egg() {
                                    "🥚 Hatch Now!"
                                } else {
                                    "✨ Evolve Now!"
                                }
                            }
                        } else if has_active_process {
                            div { class: "w-full",
                                div { class: "flex items-center justify-between text-[10px] text-muted-foreground mb-1",
                                    span {
                                        if blobbi.is_egg() { "Incubating" } else { "Evolving" }
                                    }
                                    span { "{completed_count}/{total_tasks}" }
                                }
                                div { class: "w-full h-2 bg-muted rounded-full overflow-hidden",
                                    div {
                                        class: "h-full bg-purple-500 rounded-full transition-all duration-500",
                                        style: "width: {((completed_count as f64 / total_tasks.max(1) as f64) * 100.0).min(100.0):.0}%",
                                    }
                                }
                                button {
                                    class: "mt-2 w-full py-2 rounded-xl text-xs text-muted-foreground hover:text-foreground transition",
                                    onclick: {
                                        let blobbi = blobbi.clone();
                                        move |_| {
                                            let updated = if is_incubating {
                                                stage_transition::stop_incubation(&blobbi)
                                            } else {
                                                stage_transition::stop_evolution(&blobbi)
                                            };
                                            blobbi_store::update_blobbi_in_collection(&updated);
                                            spawn(async move {
                                                let _ = publish_blobbi_state(&updated).await;
                                            });
                                        }
                                    },
                                    if is_incubating { "Stop incubation" } else { "Stop evolution" }
                                }
                            }
                        } else {
                            button {
                                class: "w-full py-3 rounded-xl text-sm font-medium bg-purple-500/10 border border-purple-500/20 text-purple-500 hover:bg-purple-500/20 transition active:scale-95",
                                disabled: busy(),
                                onclick: {
                                    let blobbi = blobbi.clone();
                                    move |_| {
                                        if busy() { return; }
                                        busy.set(true);
                                        let updated = if blobbi.is_egg() {
                                            stage_transition::start_incubation(&blobbi)
                                        } else {
                                            stage_transition::start_evolution(&blobbi)
                                        };
                                        blobbi_store::update_blobbi_in_collection(&updated);
                                        let updated_clone = updated.clone();
                                        spawn(async move {
                                            let _ = publish_blobbi_state(&updated_clone).await;
                                            busy.set(false);
                                        });
                                    }
                                },
                                if blobbi.is_egg() {
                                    "🥚 Begin Hatching"
                                } else {
                                    "✨ Begin Evolution"
                                }
                            }
                        }
                    }

                    RoomActionButton {
                        icon: rsx! { span { class: "text-2xl sm:text-3xl", "📋" } },
                        label: "Quests".to_string(),
                        color: "bg-yellow-500/10".to_string(),
                        glow_hex: "#eab308".to_string(),
                        onclick: move |_| show_missions.set(true),
                        disabled: Some(false),
                    }
                }
            }
        }

        if show_missions() {
            {
                let mut show_missions = show_missions;
                rsx! {
                    MissionsModal {
                        blobbi: blobbi.clone(),
                        on_close: move |_| show_missions.set(false),
                    }
                }
            }
        }

        if show_breeding() {
            {
                let mut show_breeding = show_breeding;
                rsx! {
                    BreedingModal {
                        blobbi: blobbi.clone(),
                        on_close: move |_| show_breeding.set(false),
                    }
                }
            }
        }

        if show_records() {
            {
                let mut show_records = show_records;
                rsx! {
                    RecordsModal {
                        blobbi: blobbi.clone(),
                        on_close: move |_| show_records.set(false),
                    }
                }
            }
        }

        BlobbiSelector {
            show: show_selector(),
            on_close: move |open| show_selector.set(open),
        }

        if blobbi.is_adult() {
            div { class: "px-4 pb-2",
                div { class: "grid grid-cols-2 gap-2",
                    button {
                        class: "flex items-center justify-center gap-1.5 py-2 rounded-xl bg-pink-500/10 border border-pink-500/20 text-pink-500 text-xs font-medium hover:bg-pink-500/20 transition",
                        onclick: move |_| show_breeding.set(true),
                        span { "🧬" }
                        span { "Breed" }
                    }
                    button {
                        class: "flex items-center justify-center gap-1.5 py-2 rounded-xl bg-blue-500/10 border border-blue-500/20 text-blue-500 text-xs font-medium hover:bg-blue-500/20 transition",
                        onclick: move |_| show_records.set(true),
                        span { "📜" }
                        span { "Records" }
                    }
                }
            }
        }
    }
}
