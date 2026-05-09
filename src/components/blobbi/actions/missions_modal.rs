use dioxus::prelude::*;

use crate::components::blobbi::actions::missions_pool::{
    get_mission_by_id, missions_for_stages, select_daily_missions,
};
use crate::components::blobbi::core::content_json::{
    safe_parse_content, state_fingerprint, update_daily_missions_content, PersistedDailyMission,
    PersistedDailyMissions,
};
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::stores::blobbi_store;

static PUBLISH_DEBOUNCE_MS: u64 = 2000;

fn today_string() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn get_persisted_missions() -> Option<PersistedDailyMissions> {
    crate::stores::blobbi_profile_store::get_profile()
        .as_ref()
        .and_then(|p| safe_parse_content(&p.content_json).daily_missions)
}

fn save_full_missions_local(missions: &PersistedDailyMissions) {
    let profile = match crate::stores::blobbi_profile_store::get_profile() {
        Some(p) => p,
        None => return,
    };
    let updated = update_daily_missions_content(&profile.content_json, missions);
    let mut p = profile;
    p.content_json = updated;
    crate::stores::blobbi_profile_store::set_profile(p);
}

async fn publish_profile_async() {
    let profile = match crate::stores::blobbi_profile_store::get_profile() {
        Some(p) => p,
        None => return,
    };
    if let Err(e) = crate::components::blobbi::core::builders::publish_profile(&profile).await {
        log::error!("Failed to publish profile: {}", e);
    }
}

static LAST_FINGERPRINT: GlobalSignal<Option<String>> = Signal::global(|| None);

async fn persist_missions_debounced(missions: PersistedDailyMissions, mut last_publish: Signal<u64>) {
    let content = crate::stores::blobbi_profile_store::get_profile()
        .map(|p| safe_parse_content(&p.content_json))
        .unwrap_or_default();
    let fp = state_fingerprint(&content);
    if Some(fp.clone()) == *LAST_FINGERPRINT.read() {
        return;
    }
    *LAST_FINGERPRINT.write() = Some(fp.clone());

    save_full_missions_local(&missions);
    let now = crate::platform::timestamp::now_millis();
    last_publish.set(now);
    spawn(async move {
        crate::platform::timer::sleep_ms((PUBLISH_DEBOUNCE_MS + 100) as u32).await;
        let current = last_publish();
        if current == now {
            publish_profile_async().await;
        }
    });
}

fn needs_daily_reset(persisted_date: &Option<String>) -> bool {
    match persisted_date {
        Some(d) if !d.is_empty() => d != &today_string(),
        _ => true,
    }
}

fn active_mission_from_persisted(pm: &PersistedDailyMission) -> ActiveMission {
    let icon = crate::components::blobbi::actions::action_types::BlobbiActionType::from_str(&pm.action)
        .map(|a| a.icon().to_string())
        .unwrap_or_else(|| "🎯".to_string());
    ActiveMission {
        id: pm.id.clone(),
        title: pm.title.clone(),
        description: pm.description.clone(),
        icon,
        reward_xp: pm.reward as u64,
        reward_coins: if pm.reward_coins > 0 { pm.reward_coins as u64 } else { pm.reward as u64 },
    }
}

fn persisted_from_active(active: &ActiveMission, progress: u32, target: u32) -> PersistedDailyMission {
    let def = get_mission_by_id(&active.id);
    PersistedDailyMission {
        id: active.id.clone(),
        title: active.title.clone(),
        description: active.description.clone(),
        action: def.map(|d| d.action.as_str().to_string()).unwrap_or_default(),
        required_count: target,
        reward: def.map(|d| d.reward_xp as u32).unwrap_or(active.reward_xp as u32),
        reward_coins: def.map(|d| d.reward_coins as u32).unwrap_or(active.reward_coins as u32),
        weight: def.map(|d| d.weight).unwrap_or(10),
        required_stages: def.map(|d| d.required_stages.iter().map(|s| s.as_str().to_string()).collect()).unwrap_or_default(),
        current_count: progress,
        completed: progress >= target,
        claimed: false,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveMission {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub reward_xp: u64,
    pub reward_coins: u64,
}

pub fn build_active_missions(blobbi: &BlobbiCompanion) -> Vec<ActiveMission> {
    let today = today_string();

    if let Some(persisted) = get_persisted_missions() {
        if persisted.date == today && !persisted.missions.is_empty() {
            return persisted
                .missions
                .iter()
                .map(active_mission_from_persisted)
                .collect();
        }
    }

    let stages = vec![blobbi.stage];
    let pubkey = crate::stores::blobbi_profile_store::get_profile()
        .as_ref()
        .and_then(|p| p.raw_event.as_ref())
        .map(|e| e.pubkey.to_hex())
        .unwrap_or_default();
    let defs = select_daily_missions(3, &today, &pubkey, &stages);
    let missions: Vec<ActiveMission> = defs
        .into_iter()
        .map(|d| ActiveMission {
            id: d.id.to_string(),
            title: d.title.to_string(),
            description: d.description.to_string(),
            icon: d.action.icon().to_string(),
            reward_xp: d.reward_xp,
            reward_coins: d.reward_coins,
        })
        .collect();

    let persisted = PersistedDailyMissions {
        date: today,
        missions: missions
            .iter()
            .map(|m| persisted_from_active(m, 0, get_mission_by_id(&m.id).map(|d| d.required_count).unwrap_or(1)))
            .collect(),
        bonus_claimed: false,
        rerolls_remaining: 3,
        total_xp_earned: 0,
        last_updated_at: crate::platform::timestamp::now_secs(),
    };
    save_full_missions_local(&persisted);

    missions
}

fn do_reroll(
    current: &[ActiveMission],
    idx: usize,
    stage: crate::utils::nip_bb::BlobbiStage,
) -> Vec<ActiveMission> {
    let stages = vec![stage];
    let pool = missions_for_stages(&stages);
    let used_ids: Vec<&str> = current.iter().map(|m| m.id.as_str()).collect();
    let available: Vec<_> = pool.iter().filter(|d| !used_ids.contains(&d.id)).collect();
    if available.is_empty() {
        return current.to_vec();
    }

    let pubkey = crate::stores::auth_store::get_pubkey()
        .unwrap_or_default();
    let seed_base = format!("{}_reroll_{}_{}:{}", today_string(), idx, current[idx].id, pubkey);
    let mut hash: i32 = 0;
    for b in seed_base.bytes() {
        hash = (hash << 5).wrapping_sub(hash).wrapping_add(b as i32);
    }
    let seed = hash.unsigned_abs();

    let mut rng = {
        let mut state = seed;
        move || {
            state = state.wrapping_add(0x6D2B79F5);
            let mut t = state;
            t = (t ^ (t >> 15)).wrapping_mul(t | 1);
            t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
            let raw = t ^ (t >> 14);
            (raw as f64) / 4294967296.0
        }
    };

    let total_weight: f64 = available.iter().map(|m| m.weight as f64).sum();
    let mut pick = rng() * total_weight;
    let mut chosen_idx = 0;
    for (i, m) in available.iter().enumerate() {
        pick -= m.weight as f64;
        if pick <= 0.0 {
            chosen_idx = i;
            break;
        }
    }
    let new_def = available[chosen_idx];
    let mut result = current.to_vec();
    result[idx] = ActiveMission {
        id: new_def.id.to_string(),
        title: new_def.title.to_string(),
        description: new_def.description.to_string(),
        icon: new_def.action.icon().to_string(),
        reward_xp: new_def.reward_xp,
        reward_coins: new_def.reward_coins,
    };
    result
}

pub fn is_mission_claimed(blobbi: &BlobbiCompanion, mission_id: &str) -> bool {
    let tag = format!("{}_claimed", mission_id);
    blobbi.tasks.iter().any(|t| t.id == tag && t.completed)
}

pub fn get_mission_progress(blobbi: &BlobbiCompanion, mission_id: &str) -> (u32, u32) {
    let def = get_mission_by_id(mission_id);
    let target = def.map(|d| d.required_count).unwrap_or(1);
    let action_tag = def.map(|d| d.action.as_str()).unwrap_or(mission_id);
    if let Some(task) = blobbi.tasks.iter().find(|t| t.id == mission_id) {
        return (task.progress.min(target), task.target);
    }
    let action_count = blobbi
        .tasks
        .iter()
        .filter(|t| t.id.starts_with(&format!("daily_{}", action_tag)))
        .map(|t| t.progress)
        .sum::<u32>();
    (action_count.min(target), target)
}

#[derive(Clone, Debug)]
struct MissionCard {
    idx: usize,
    mission: ActiveMission,
    progress: u32,
    target: u32,
    is_completed: bool,
    is_claimed: bool,
    can_reroll: bool,
    pct: u32,
}

fn build_cards(
    missions: &[ActiveMission],
    blobbi: &BlobbiCompanion,
    rerolls_remaining: u32,
) -> Vec<MissionCard> {
    missions
        .iter()
        .enumerate()
        .map(|(idx, mission)| {
            let (progress, target) = get_mission_progress(blobbi, &mission.id);
            let is_completed = progress >= target;
            let is_claimed = is_mission_claimed(blobbi, &mission.id);
            let can_reroll = rerolls_remaining > 0 && !is_claimed && !is_completed;
            let pct = if target > 0 {
                (progress as f64 / target as f64 * 100.0) as u32
            } else {
                0
            };
            MissionCard {
                idx,
                mission: mission.clone(),
                progress,
                target,
                is_completed,
                is_claimed,
                can_reroll,
                pct,
            }
        })
        .collect()
}

fn render_card(card: &MissionCard) -> Element {
    let card_class = if card.is_claimed {
        "flex items-center gap-3 p-3 rounded-xl bg-muted/50 border border-border opacity-60"
    } else if card.is_completed {
        "flex items-center gap-3 p-3 rounded-xl bg-green-500/10 border border-green-500/20"
    } else {
        "flex items-center gap-3 p-3 rounded-xl bg-background border border-border"
    };

    rsx! {
        div { class: "{card_class}",
            span { class: "text-2xl", "{card.mission.icon}" }
            div { class: "flex-1 min-w-0",
                span { class: "text-sm font-medium block", "{card.mission.title}" }
                span { class: "text-[10px] text-muted-foreground block", "{card.mission.description}" }
                div { class: "flex items-center gap-2 mt-1",
                    if !card.is_claimed && !card.is_completed {
                        div { class: "flex-1 h-1.5 bg-muted rounded-full overflow-hidden",
                            div {
                                class: "h-full bg-blue-400 rounded-full transition-all",
                                style: "width: {card.pct}%",
                            }
                        }
                        span { class: "text-[10px] text-muted-foreground", "{card.progress}/{card.target}" }
                    }
                }
                div { class: "flex gap-3 mt-1",
                    span { class: "text-[10px] text-yellow-500",
                        "\u{1FA99} {card.mission.reward_coins}"
                    }
                    span { class: "text-[10px] text-blue-400",
                        "\u{2B50} {card.mission.reward_xp} XP"
                    }
                }
            }
        }
    }
}

#[component]
pub fn MissionsModal(blobbi: BlobbiCompanion, on_close: EventHandler<()>) -> Element {
    let initial = build_active_missions(&blobbi);
    let mut missions: Signal<Vec<ActiveMission>> = use_signal(|| initial);

    let persisted = get_persisted_missions();
    let persisted_date = persisted.as_ref().map(|m| m.date.clone());
    let persisted_remaining = persisted.as_ref().map(|m| m.rerolls_remaining).unwrap_or(3);

    let mut rerolls_remaining: Signal<u32> = use_signal(move || persisted_remaining);
    let last_publish: Signal<u64> = use_signal(|| 0u64);
    let max_rerolls: u32 = 3;

    if needs_daily_reset(&persisted_date) {
        let reset_remaining = 3u32;
        if *rerolls_remaining.read() != reset_remaining {
            rerolls_remaining.set(reset_remaining);
            let lp = last_publish;
            let blobbi_for_build = blobbi.clone();
            spawn(async move {
                let fresh = {
                    let stages = vec![blobbi_for_build.stage];
                    let pubkey = crate::stores::blobbi_profile_store::get_profile()
                        .as_ref()
                        .and_then(|p| p.raw_event.as_ref())
                        .map(|e| e.pubkey.to_hex())
                        .unwrap_or_default();
                    select_daily_missions(3, &today_string(), &pubkey, &stages)
                };
                let new_persisted = PersistedDailyMissions {
                    date: today_string(),
                    missions: fresh.iter().map(|d| PersistedDailyMission {
                        id: d.id.to_string(),
                        title: d.title.to_string(),
                        description: d.description.to_string(),
                        action: d.action.as_str().to_string(),
                        required_count: d.required_count,
                        reward: d.reward_xp as u32,
                        reward_coins: d.reward_coins as u32,
                        weight: d.weight,
                        required_stages: d.required_stages.iter().map(|s| s.as_str().to_string()).collect(),
                        current_count: 0,
                        completed: false,
                        claimed: false,
                    }).collect(),
                    bonus_claimed: false,
                    rerolls_remaining: reset_remaining,
                    total_xp_earned: 0,
                    last_updated_at: crate::platform::timestamp::now_secs(),
                };
                persist_missions_debounced(new_persisted, lp).await;
            });
        }
    }

    let cards = build_cards(&missions.read(), &blobbi, *rerolls_remaining.read());
    let all_completed = cards.iter().all(|c| c.is_completed) && !cards.is_empty();
    let _all_claimed = cards.iter().all(|c| c.is_claimed) && !cards.is_empty();
    let rerolls_used = max_rerolls.saturating_sub(*rerolls_remaining.read());

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
                    div { class: "flex items-center gap-2",
                        span { class: "text-xs text-muted-foreground",
                            "Rerolls: {rerolls_used}/{max_rerolls}"
                        }                        button {
                            class: "p-1.5 hover:bg-accent rounded-lg transition",
                            onclick: move |_| on_close.call(()),
                            "\u{2715}"
                        }
                    }
                }

                div { class: "flex-1 overflow-y-auto p-4 space-y-3",
                    for card in &cards {
                        {rsx! {
                            div { key: "{card.mission.id}_{card.idx}",
                                {render_card(card)}

                                if card.is_claimed {
                                    span { class: "text-xs text-muted-foreground", "Done" }
                                } else if card.is_completed {
                                    {
                                        let mid = card.mission.id.clone();
                                        let coins = card.mission.reward_coins;
                                        let xp = card.mission.reward_xp;
                                        rsx! {
                                            button {
                                                class: "px-3 py-1.5 bg-green-500 hover:bg-green-600 text-white text-xs font-medium rounded-lg transition",
                                                onclick: move |_| {
                                                    let mid = mid.clone();
                                                    spawn(async move {
                                                        claim_mission(&mid, coins, xp).await;
                                                    });
                                                },
                                                "Claim"
                                            }
                                        }
                                    }
                                } else if card.can_reroll {
                                    {
                                        let idx = card.idx;
                                        let stage = blobbi.stage;
                                        rsx! {
                                            button {
                                                class: "p-1.5 hover:bg-accent rounded-lg transition text-muted-foreground hover:text-foreground",
                                                title: "Reroll mission",
                                                onclick: move |_| {
                                                    let current = missions.read().clone();
                                                    let next = do_reroll(&current, idx, stage);
                                                    let remaining = rerolls_remaining.read().saturating_sub(1);
                                                    missions.set(next.clone());
                                                    rerolls_remaining.set(remaining);
            let lp = last_publish;
                                                    spawn(async move {
                                                        let existing_xp = get_persisted_missions()
                                                            .map(|p| p.total_xp_earned)
                                                            .unwrap_or(0);
                                                        let persisted = PersistedDailyMissions {
                                                            date: today_string(),
                                                            missions: next.iter().map(|m| persisted_from_active(m, 0, get_mission_by_id(&m.id).map(|d| d.required_count).unwrap_or(1))).collect(),
                                                            bonus_claimed: false,
                                                            rerolls_remaining: remaining,
                                                            total_xp_earned: existing_xp,
                                                            last_updated_at: crate::platform::timestamp::now_secs(),
                                                        };
                                                        persist_missions_debounced(persisted, lp).await;
                                                    });
                                                },
                                                "\u{1F3B2}"
                                            }
                                        }
                                    }
                                }
                            }
                        }}
                    }
                }

                if all_completed {
                    {render_bonus_section(&blobbi)}
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

fn render_bonus_section(blobbi: &BlobbiCompanion) -> Element {
    let bonus_id = "daily_all_bonus";
    let bonus_claimed = is_mission_claimed(blobbi, bonus_id);

    rsx! {
        div { class: "mx-4 mb-2 p-3 rounded-xl bg-yellow-500/10 border border-yellow-500/20",
            div { class: "flex items-center gap-3",
                span { class: "text-2xl", "\u{1F31F}" }
                div { class: "flex-1",
                    span { class: "text-sm font-medium block", "Bonus: All Missions Complete!" }
                    span { class: "text-[10px] text-muted-foreground block", "+50 XP + 25 coins for completing all daily missions" }
                }
                if bonus_claimed {
                    span { class: "text-xs text-muted-foreground", "Done" }
                } else {
                    button {
                        class: "px-3 py-1.5 bg-yellow-500 hover:bg-yellow-600 text-white text-xs font-medium rounded-lg transition",
                        onclick: move |_| {
                            spawn(async move {
                                claim_mission(bonus_id, 25, 50).await;
                            });
                        },
                        "Claim"
                    }
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

        let mut profile = match crate::stores::blobbi_profile_store::get_profile() {
            Some(p) => p,
            None => return,
        };
        let original_coins = profile.coins;

        blobbi.tasks.push(crate::components::blobbi::core::types::BlobbiTaskProgress {
            id: claimed_tag.clone(),
            completed: true,
            progress: 1,
            target: 1,
        });
        blobbi.experience = blobbi.experience.saturating_add(reward_xp);
        profile.coins = profile.coins.saturating_add(reward_coins);

        if let Some(mut persisted) = get_persisted_missions() {
            for m in &mut persisted.missions {
                if m.id == mission_id {
                    m.claimed = true;
                    m.completed = true;
                }
            }
            if mission_id == "daily_all_bonus" {
                persisted.bonus_claimed = true;
            }
            persisted.total_xp_earned = persisted.total_xp_earned.saturating_add(reward_xp);
            profile.content_json = update_daily_missions_content(&profile.content_json, &persisted);
        }

        if let Err(e) = crate::components::blobbi::core::builders::publish_profile(&profile).await {
            log::error!("Failed to publish profile (mission reward): {}", e);
            return;
        }

        if let Err(e) = crate::components::blobbi::core::builders::publish_blobbi_state(&blobbi).await {
            log::error!("Failed to publish blobbi state (mission claim): {}", e);
            profile.coins = original_coins;
            let _ = crate::components::blobbi::core::builders::publish_profile(&profile).await;
            return;
        }

        let toast = dioxus_primitives::toast::consume_toast();
        toast.success(
            format!("Reward claimed! +{} XP +{} coins", reward_xp, reward_coins),
            dioxus_primitives::toast::ToastOptions::new()
                .duration(std::time::Duration::from_secs(3)),
        );

        crate::stores::blobbi_profile_store::set_profile(profile);
        blobbi_store::update_blobbi_in_collection(&blobbi);
    }
}
