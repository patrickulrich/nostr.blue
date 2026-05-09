use dioxus::prelude::*;

use crate::components::blobbi::actions::missions_modal::{
    build_active_missions, get_mission_progress, is_mission_claimed,
};
use crate::components::blobbi::actions::stage_transition;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::actions::hatch_tasks;

#[component]
pub fn MissionSurfaceCard(blobbi: BlobbiCompanion) -> Element {
    let mut index = use_signal(|| 0usize);
    let mut hide = use_signal(|| false);

    if hide() {
        return rsx! { div {} };
    }

    let hatch_defs = hatch_tasks::tasks_for_stage(blobbi.stage);
    let incomplete_hatch: Vec<_> = hatch_defs
        .iter()
        .filter(|def| !hatch_tasks::is_task_completed(&blobbi, def.id))
        .collect();

    let can_evolve = stage_transition::can_transition(&blobbi);

    let daily_missions = build_active_missions(&blobbi);
    let incomplete_daily: Vec<_> = daily_missions
        .iter()
        .enumerate()
        .filter(|(_, m)| !is_mission_claimed(&blobbi, &m.id))
        .collect();

    let mut cards: Vec<MissionCard> = Vec::new();

    if !incomplete_hatch.is_empty() && !can_evolve {
        for def in &incomplete_hatch {
            let task = blobbi.tasks.iter().find(|t| t.id == def.id);
            let current = task.map(|t| t.progress).unwrap_or(0);
            cards.push(MissionCard {
                badge: if blobbi.is_egg() { "Hatch" } else { "Evolve" }.to_string(),
                badge_color: "bg-amber-500/20 text-amber-400".to_string(),
                title: def.name.to_string(),
                description: def.description.to_string(),
                icon: def.icon.to_string(),
                current,
                target: def.target,
                reward_xp: 0,
                reward_coins: 0,
            });
        }
    }

    for (_, m) in &incomplete_daily {
        let (current, target) = get_mission_progress(&blobbi, &m.id);
        cards.push(MissionCard {
            badge: "Daily".to_string(),
            badge_color: "bg-blue-500/20 text-blue-400".to_string(),
            title: m.title.clone(),
            description: m.description.clone(),
            icon: m.icon.clone(),
            current,
            target,
            reward_xp: m.reward_xp,
            reward_coins: m.reward_coins,
        });
    }

    if cards.is_empty() {
        return rsx! { div {} };
    }

    if index() >= cards.len() {
        index.set(0);
    }

    let card = cards[index()].clone();
    let card_count = cards.len();
    let progress_pct = if card.target > 0 {
        ((card.current as f64 / card.target as f64) * 100.0).min(100.0)
    } else {
        100.0
    };
    let progress_str = format!("{:.0}", progress_pct);

    rsx! {
        div { class: "px-4 mt-3",
            div {
                class: "relative p-3 rounded-lg bg-card border border-border",

                onclick: move |_| {
                    let next = index() + 1;
                    index.set(if next >= card_count { 0 } else { next });
                },

                div { class: "flex items-center justify-between mb-1",
                    div { class: "flex items-center gap-2",
                        span { class: "text-sm", "{card.icon}" }
                        span { class: "text-[10px] px-1.5 py-0.5 rounded {card.badge_color}",
                            "{card.badge}"
                        }
                        span { class: "text-xs font-medium", "{card.title}" }
                    }
                    button {
                        class: "text-[10px] text-muted-foreground hover:text-foreground transition p-0.5",
                        onclick: move |e| {
                            e.stop_propagation();
                            hide.set(true);
                        },
                        "✕"
                    }
                }

                p { class: "text-[10px] text-muted-foreground mb-1.5", "{card.description}" }

                div { class: "w-full h-1.5 bg-muted rounded-full overflow-hidden",
                    div {
                        class: "h-full bg-blue-500 rounded-full transition-all duration-300",
                        style: "width: {progress_str}%",
                    }
                }

                div { class: "flex items-center justify-between mt-1",
                    span { class: "text-[9px] text-muted-foreground",
                        "{card.current}/{card.target}"
                    }
                    if card.reward_coins > 0 {
                        span { class: "text-[9px] text-yellow-500",
                            "+{card.reward_coins} coins"
                        }
                    }
                }

                if card_count > 1 {
                    div { class: "flex justify-center gap-1 mt-1.5",
                        for i in 0..card_count {
                            div {
                                class: if i == index() {
                                    "w-1.5 h-1.5 rounded-full bg-blue-500"
                                } else {
                                    "w-1.5 h-1.5 rounded-full bg-muted"
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
struct MissionCard {
    badge: String,
    badge_color: String,
    title: String,
    description: String,
    icon: String,
    current: u32,
    target: u32,
    reward_xp: u64,
    reward_coins: u64,
}
