use dioxus::prelude::*;

use crate::components::blobbi::core::builders::publish_blobbi_state;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::recipe::EmotionPreset;
use crate::stores::blobbi_store;
use crate::utils::nip_bb::*;

fn is_dev() -> bool {
    #[cfg(debug_assertions)]
    {
        true
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

#[component]
pub fn DevTools(blobbi: BlobbiCompanion) -> Element {
    if !is_dev() {
        return rsx! { div {} };
    }

    let mut show = use_signal(|| false);
    let mut emotion_override = use_signal(|| None::<EmotionPreset>);

    rsx! {
        div { class: "mt-4 px-4",
            button {
                class: "text-[10px] text-muted-foreground underline",
                onclick: move |_| show.set(!show()),
                "Dev Tools"
            }

            if show() {
                div { class: "mt-2 p-3 rounded-lg bg-muted/30 border border-dashed border-border text-xs space-y-2",
                    div { class: "font-medium", "State Inspector" }

                    div { class: "grid grid-cols-2 gap-1 text-[10px] text-muted-foreground",
                        span { "d:" }
                        span { "{blobbi.d}" }
                        span { "stage:" }
                        span { "{blobbi.stage.as_str()}" }
                        span { "state:" }
                        span { "{blobbi.state.as_str()}" }
                        span { "xp:" }
                        span { "{blobbi.experience}" }
                        span { "generation:" }
                        span { "{blobbi.generation}" }
                        span { "streak:" }
                        span { "{blobbi.care_streak}" }
                        span { "sleeping:" }
                        span { "{blobbi.is_sleeping()}" }
                        span { "breeding_ready:" }
                        span { "{blobbi.breeding_ready}" }
                    }

                    div { class: "grid grid-cols-5 gap-1 text-[10px]",
                        div { class: "text-center",
                            div { "🍔" }
                            div { "{blobbi.stats.hunger:.0}" }
                        }
                        div { class: "text-center",
                            div { "😊" }
                            div { "{blobbi.stats.happiness:.0}" }
                        }
                        div { class: "text-center",
                            div { "❤️" }
                            div { "{blobbi.stats.health:.0}" }
                        }
                        div { class: "text-center",
                            div { "🧹" }
                            div { "{blobbi.stats.hygiene:.0}" }
                        }
                        div { class: "text-center",
                            div { "⚡" }
                            div { "{blobbi.stats.energy:.0}" }
                        }
                    }

                    div { class: "font-medium mt-2", "Emotion Presets" }
                    div { class: "flex flex-wrap gap-1",
                        for emotion in EmotionPreset::all() {
                            {
                                let is_active = emotion_override() == Some(*emotion);
                                let label = emotion.as_str();
                                rsx! {
                                    button {
                                        class: if is_active {
                                            "px-1.5 py-0.5 rounded bg-purple-500/30 text-purple-300 text-[9px] border border-purple-500/50"
                                        } else {
                                            "px-1.5 py-0.5 rounded bg-muted text-muted-foreground text-[9px] hover:bg-accent transition"
                                        },
                                        onclick: move |_| {
                                            if emotion_override() == Some(*emotion) {
                                                emotion_override.set(None);
                                            } else {
                                                emotion_override.set(Some(*emotion));
                                            }
                                        },
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                    if emotion_override().is_some() {
                        button {
                            class: "px-1.5 py-0.5 rounded bg-red-500/20 text-red-400 text-[9px]",
                            onclick: move |_| emotion_override.set(None),
                            "Clear Emotion"
                        }
                    }

                    div { class: "font-medium mt-2", "Quick Actions" }

                    div { class: "flex flex-wrap gap-1",
                        button {
                            class: "px-2 py-1 rounded bg-green-500/20 text-green-500 text-[10px]",
                            onclick: {
                                let blobbi = blobbi.clone();
                                move |_| {
                                    let mut b = blobbi.clone();
                                    b.stats.hunger = STAT_MAX;
                                    b.stats.happiness = STAT_MAX;
                                    b.stats.health = STAT_MAX;
                                    b.stats.hygiene = STAT_MAX;
                                    b.stats.energy = STAT_MAX;
                                    let b_clone = b.clone();
                                    spawn(async move {
                                        let _ = publish_blobbi_state(&b_clone).await;
                                    });
                                    blobbi_store::update_blobbi_in_collection(&b);
                                }
                            },
                            "Max Stats"
                        }
                        button {
                            class: "px-2 py-1 rounded bg-yellow-500/20 text-yellow-500 text-[10px]",
                            onclick: {
                                let blobbi = blobbi.clone();
                                move |_| {
                                    let mut b = blobbi.clone();
                                    b.experience += 100;
                                    let b_clone = b.clone();
                                    spawn(async move {
                                        let _ = publish_blobbi_state(&b_clone).await;
                                    });
                                    blobbi_store::update_blobbi_in_collection(&b);
                                }
                            },
                            "+100 XP"
                        }
                        button {
                            class: "px-2 py-1 rounded bg-blue-500/20 text-blue-500 text-[10px]",
                            onclick: {
                                let blobbi = blobbi.clone();
                                move |_| {
                                    let mut b = blobbi.clone();
                                    b.breeding_ready = !b.breeding_ready;
                                    let b_clone = b.clone();
                                    spawn(async move {
                                        let _ = publish_blobbi_state(&b_clone).await;
                                    });
                                    blobbi_store::update_blobbi_in_collection(&b);
                                }
                            },
                            "Toggle Breed Ready"
                        }
                        button {
                            class: "px-2 py-1 rounded bg-red-500/20 text-red-500 text-[10px]",
                            onclick: {
                                let blobbi = blobbi.clone();
                                move |_| {
                                    let mut b = blobbi.clone();
                                    b.stage = BlobbiStage::Adult;
                                    b.adult_type = Some("pandi".to_string());
                                    b.stats.hunger = STAT_MAX;
                                    b.stats.happiness = STAT_MAX;
                                    b.stats.health = STAT_MAX;
                                    b.stats.hygiene = STAT_MAX;
                                    b.stats.energy = STAT_MAX;
                                    b.breeding_ready = true;
                                    let b_clone = b.clone();
                                    spawn(async move {
                                        let _ = publish_blobbi_state(&b_clone).await;
                                    });
                                    blobbi_store::update_blobbi_in_collection(&b);
                                }
                            },
                            "Force Adult"
                        }
                    }

                    if !blobbi.tasks.is_empty() {
                        div { class: "font-medium mt-2", "Tasks" }
                        for task in &blobbi.tasks {
                            div { class: "text-[10px] text-muted-foreground",
                                "{task.id}: {task.progress}/{task.target}"
                            }
                        }
                    }
                }
            }
        }
    }
}
