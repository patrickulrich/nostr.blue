use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::stores::blobbi_store;

#[component]
pub fn MissionsModal(blobbi: BlobbiCompanion, on_close: EventHandler<()>) -> Element {
    let missions = generate_daily_missions();

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-card border border-border rounded-2xl w-full max-w-md mx-4 shadow-2xl max-h-[85vh] flex flex-col",
                onclick: move |e| e.stop_propagation(),

                div { class: "flex items-center justify-between p-4 border-b border-border",
                    div { class: "flex items-center gap-2",
                        span { class: "text-xl", "\u{1F4CB}" }
                        h3 { class: "text-lg font-bold", "Daily Missions" }
                    }
                    button {
                        class: "p-1.5 hover:bg-accent rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "\u{2715}"
                    }
                }

                div { class: "flex-1 overflow-y-auto p-4 space-y-3",
                    for mission in &missions {
                        {render_mission(mission, &blobbi)}
                    }
                }

                div { class: "p-4 border-t border-border text-center",
                    span { class: "text-xs text-muted-foreground",
                        "Missions reset daily at midnight"
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Mission {
    id: String,
    label: String,
    icon: String,
    description: String,
    reward_coins: u64,
    reward_xp: u64,
}

fn generate_daily_missions() -> Vec<Mission> {
    vec![
        Mission {
            id: "feed_3".to_string(),
            label: "Feed 3 Times".to_string(),
            icon: "\u{1F354}".to_string(),
            description: "Feed your Blobbi 3 times today".to_string(),
            reward_coins: 15,
            reward_xp: 20,
        },
        Mission {
            id: "clean_2".to_string(),
            label: "Clean Twice".to_string(),
            icon: "\u{1F9F9}".to_string(),
            description: "Clean your Blobbi 2 times today".to_string(),
            reward_coins: 10,
            reward_xp: 15,
        },
        Mission {
            id: "play_2".to_string(),
            label: "Play Session".to_string(),
            icon: "\u{1F3AE}".to_string(),
            description: "Play with your Blobbi 2 times today".to_string(),
            reward_coins: 20,
            reward_xp: 25,
        },
    ]
}

fn is_mission_claimed(blobbi: &BlobbiCompanion, mission_id: &str) -> bool {
    let tag = format!("{}_claimed", mission_id);
    blobbi.tasks.iter().any(|t| t.id == tag && t.completed)
}

fn is_mission_completed(blobbi: &BlobbiCompanion, mission_id: &str) -> bool {
    blobbi.tasks.iter().any(|t| t.id == mission_id && t.completed)
}

fn render_mission(mission: &Mission, blobbi: &BlobbiCompanion) -> Element {
    let is_completed = is_mission_completed(blobbi, &mission.id);
    let is_claimed = is_mission_claimed(blobbi, &mission.id);

    rsx! {
        div {
            class: if is_claimed {
                "flex items-center gap-3 p-3 rounded-xl bg-muted/50 border border-border opacity-60"
            } else if is_completed {
                "flex items-center gap-3 p-3 rounded-xl bg-green-500/10 border border-green-500/20"
            } else {
                "flex items-center gap-3 p-3 rounded-xl bg-background border border-border"
            },
            span { class: "text-2xl", "{mission.icon}" }
            div { class: "flex-1 min-w-0",
                span { class: "text-sm font-medium block", "{mission.label}" }
                span { class: "text-[10px] text-muted-foreground block", "{mission.description}" }
                div { class: "flex gap-3 mt-1",
                    span { class: "text-[10px] text-yellow-500",
                        "\u{1FA99} {mission.reward_coins}"
                    }
                    span { class: "text-[10px] text-blue-400",
                        "\u{2B50} {mission.reward_xp} XP"
                    }
                }
            }
            if is_claimed {
                span { class: "text-xs text-muted-foreground", "Done" }
            } else if is_completed {
                button {
                    class: "px-3 py-1.5 bg-green-500 hover:bg-green-600 text-white text-xs font-medium rounded-lg transition",
                    onclick: {
                        let mission_id = mission.id.clone();
                        let reward_coins = mission.reward_coins;
                        let reward_xp = mission.reward_xp;
                        move |_| {
                            let mission_id = mission_id.clone();
                            spawn(async move {
                                claim_mission(&mission_id, reward_coins, reward_xp).await;
                            });
                        }
                    },
                    "Claim"
                }
            }
        }
    }
}

async fn claim_mission(mission_id: &str, reward_coins: u64, reward_xp: u64) {
    let claimed_tag = format!("{}_claimed", mission_id);
    if let Some(mut blobbi) = blobbi_store::get_selected_blobbi() {
        if is_mission_claimed(&blobbi, mission_id) {
            return;
        }
        blobbi.tasks.push(crate::components::blobbi::core::types::BlobbiTaskProgress {
            id: claimed_tag.clone(),
            completed: true,
            progress: 1,
            target: 1,
        });
        blobbi.experience = blobbi.experience.saturating_add(reward_xp);

        if let Some(mut profile) = crate::stores::blobbi_profile_store::get_profile() {
            profile.coins = profile.coins.saturating_add(reward_coins);
            let _ = crate::components::blobbi::core::builders::publish_profile(&profile).await;
            crate::stores::blobbi_profile_store::set_profile(profile);
        }

        let _ = crate::components::blobbi::core::builders::publish_blobbi_state(&blobbi).await;
        blobbi_store::update_blobbi_in_collection(&blobbi);
    }
}
