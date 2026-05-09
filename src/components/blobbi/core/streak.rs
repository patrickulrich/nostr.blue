use crate::components::blobbi::actions::hatch_tasks;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::utils::nip_bb::constants::*;
use crate::utils::nip_bb::BlobbiStage;

pub fn today_day_string() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn days_between(day_a: &str, day_b: &str) -> Option<u32> {
    let a = chrono::NaiveDate::parse_from_str(day_a, "%Y-%m-%d").ok()?;
    let b = chrono::NaiveDate::parse_from_str(day_b, "%Y-%m-%d").ok()?;
    Some(b.signed_duration_since(a).num_days() as u32)
}

pub fn compute_streak(
    current_streak: u32,
    last_care_day_str: Option<&str>,
    today_str: &str,
) -> (u32, String, bool) {
    let last = last_care_day_str.unwrap_or("");

    if last.is_empty() || current_streak == 0 {
        return (1, today_str.to_string(), true);
    }

    if last == today_str {
        return (current_streak, today_str.to_string(), false);
    }

    match days_between(last, today_str) {
        Some(1) => (current_streak.saturating_add(1), today_str.to_string(), true),
        Some(_) => (1, today_str.to_string(), true),
        None => (1, today_str.to_string(), true),
    }
}

pub fn record_care_action(blobbi: &mut BlobbiCompanion, action: &str) {
    if !is_care_action(action) {
        return;
    }

    let today = today_day_string();
    let last_day = blobbi.care_streak_last_day.as_deref();

    let (new_streak, new_day, _was_updated) = compute_streak(blobbi.care_streak, last_day, &today);

    blobbi.care_streak = new_streak;
    blobbi.care_streak_last_day = Some(new_day);
    blobbi.care_streak_last_at = Some(nostr_sdk::Timestamp::now().as_secs());
    blobbi.last_interaction = Some(nostr_sdk::Timestamp::now().as_secs());
}

pub fn streak_bonus(streak: u32) -> f64 {
    if streak >= 30 {
        0.3
    } else if streak >= 14 {
        0.2
    } else if streak >= 7 {
        0.15
    } else if streak >= 3 {
        0.1
    } else {
        0.0
    }
}

pub fn streak_xp_bonus(streak: u32) -> u64 {
    if streak >= 30 {
        5
    } else if streak >= 14 {
        3
    } else if streak >= 7 {
        2
    } else if streak >= 3 {
        1
    } else {
        0
    }
}

pub fn streak_label(streak: u32) -> &'static str {
    if streak >= 30 {
        "Legendary"
    } else if streak >= 14 {
        "Dedicated"
    } else if streak >= 7 {
        "Consistent"
    } else if streak >= 3 {
        "Growing"
    } else {
        "New"
    }
}

pub fn streak_decay_rate_bonus(streak: u32, stage: BlobbiStage) -> f64 {
    let base = match stage {
        BlobbiStage::Baby => 0.1,
        BlobbiStage::Adult => 0.05,
        _ => 0.0,
    };
    base * streak_bonus(streak)
}

pub fn check_hatch_readiness(blobbi: &BlobbiCompanion) -> HatchReadiness {
    if !blobbi.is_egg() {
        return HatchReadiness::default();
    }

    let now = nostr_sdk::Timestamp::now().as_secs();
    let birth = blobbi.last_interaction.unwrap_or(now);
    let days_passed = (now.saturating_sub(birth)) / 86400;
    let xp = blobbi.experience;
    let health = blobbi.stats.health;
    let care_days = blobbi.care_streak;

    let days_ok = days_passed >= HATCH_MIN_DAYS;
    let xp_ok = xp >= HATCH_MIN_EXPERIENCE;
    let health_ok = health >= HATCH_MIN_HEALTH;
    let care_ok = care_days >= HATCH_MIN_CARE_DAYS as u32;
    let tasks_ok = hatch_tasks::all_tasks_completed(blobbi);

    let ready = days_ok && xp_ok && health_ok && care_ok && tasks_ok;

    HatchReadiness {
        ready,
        days_passed,
        days_required: HATCH_MIN_DAYS,
        xp,
        xp_required: HATCH_MIN_EXPERIENCE,
        health,
        health_required: HATCH_MIN_HEALTH,
        care_days,
        care_days_required: HATCH_MIN_CARE_DAYS,
        tasks_completed: tasks_ok,
    }
}

pub fn check_evolve_readiness(blobbi: &BlobbiCompanion) -> EvolveReadiness {
    if !blobbi.is_baby() {
        return EvolveReadiness::default();
    }

    let now = nostr_sdk::Timestamp::now().as_secs();
    let birth = blobbi.last_interaction.unwrap_or(now);
    let days_passed = (now.saturating_sub(birth)) / 86400;
    let xp = blobbi.experience;
    let happiness = blobbi.stats.happiness;
    let health = blobbi.stats.health;

    let (tasks_done, _tasks_total) = hatch_tasks::task_progress_summary(blobbi);

    let days_ok = days_passed >= EVOLVE_MIN_DAYS;
    let xp_ok = xp >= EVOLVE_MIN_EXPERIENCE;
    let interactions_ok = tasks_done as u64 >= EVOLVE_MIN_INTERACTIONS;
    let happiness_ok = happiness >= EVOLVE_MIN_HAPPINESS;
    let health_ok = health >= EVOLVE_MIN_HEALTH;
    let quests_ok = hatch_tasks::all_tasks_completed(blobbi);

    let ready = days_ok && xp_ok && interactions_ok && happiness_ok && health_ok && quests_ok;

    EvolveReadiness {
        ready,
        days_passed,
        days_required: EVOLVE_MIN_DAYS,
        xp,
        xp_required: EVOLVE_MIN_EXPERIENCE,
        interactions: tasks_done as u64,
        interactions_required: EVOLVE_MIN_INTERACTIONS,
        happiness,
        happiness_required: EVOLVE_MIN_HAPPINESS,
        health,
        health_required: EVOLVE_MIN_HEALTH,
        quests_completed: quests_ok,
    }
}

#[derive(Clone, Debug, Default)]
pub struct HatchReadiness {
    pub ready: bool,
    pub days_passed: u64,
    pub days_required: u64,
    pub xp: u64,
    pub xp_required: u64,
    pub health: f64,
    pub health_required: f64,
    pub care_days: u32,
    pub care_days_required: u64,
    pub tasks_completed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct EvolveReadiness {
    pub ready: bool,
    pub days_passed: u64,
    pub days_required: u64,
    pub xp: u64,
    pub xp_required: u64,
    pub interactions: u64,
    pub interactions_required: u64,
    pub happiness: f64,
    pub happiness_required: f64,
    pub health: f64,
    pub health_required: f64,
    pub quests_completed: bool,
}
