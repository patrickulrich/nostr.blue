use crate::components::blobbi::core::types::*;
use crate::utils::nip_bb::*;
use nostr_sdk::{Event, Tags};

fn tag_value(tags: &Tags, name: &str) -> Option<String> {
    for tag in tags.iter() {
        if tag.kind().to_string() == name {
            if let Some(content) = tag.content() {
                return Some(content.to_string());
            }
        }
    }
    None
}

fn tag_values(tags: &Tags, name: &str) -> Vec<String> {
    let mut result = Vec::new();
    for tag in tags.iter() {
        if tag.kind().to_string() == name {
            if let Some(content) = tag.content() {
                result.push(content.to_string());
            }
        }
    }
    result
}

fn tag_f64(tags: &Tags, name: &str) -> Option<f64> {
    tag_value(tags, name).and_then(|v| v.parse().ok())
}

fn tag_u64(tags: &Tags, name: &str) -> Option<u64> {
    tag_value(tags, name).and_then(|v| v.parse().ok())
}

fn tag_u32(tags: &Tags, name: &str) -> Option<u32> {
    tag_value(tags, name).and_then(|v| v.parse().ok())
}

fn tag_bool(tags: &Tags, name: &str) -> bool {
    tag_value(tags, name).as_deref() == Some("true")
}

fn clamp_stat(value: f64) -> f64 {
    value.round().clamp(STAT_MIN, STAT_MAX)
}

fn recover_missing_stats(tags: &Tags) -> BlobbiStats {
    let now = crate::platform::timestamp::now_secs();

    let most_recent = [
        tag_u64(tags, TAG_LAST_MEAL),
        tag_u64(tags, TAG_LAST_CLEAN),
        tag_u64(tags, TAG_LAST_WARM),
        tag_u64(tags, TAG_LAST_TALK),
        tag_u64(tags, TAG_LAST_CHECK),
        tag_u64(tags, TAG_LAST_SING),
        tag_u64(tags, TAG_LAST_MEDICINE),
        tag_u64(tags, TAG_LAST_INTERACTION),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(now);

    let elapsed_secs = now.saturating_sub(most_recent);
    let hours_passed = (elapsed_secs as f64 / 3600.0).min(24.0);

    let base = STAT_DEFAULT;
    let hunger = clamp_stat(base - 5.0 * hours_passed);
    let happiness = clamp_stat(base - 3.0 * hours_passed);
    let hygiene = clamp_stat(base - 4.0 * hours_passed);
    let energy = clamp_stat(base - 5.0 * hours_passed);
    let mut health = clamp_stat(base - 1.0 * hours_passed);

    if hunger < 30.0 {
        health = (health - 2.0).max(STAT_MIN);
    }
    if hygiene < 20.0 {
        health = (health - 1.0).max(STAT_MIN);
    }
    if energy < 20.0 {
        health = (health - 1.0).max(STAT_MIN);
    }
    if happiness < 30.0 {
        health = (health - 1.0).max(STAT_MIN);
    }

    BlobbiStats {
        hunger,
        happiness,
        health: clamp_stat(health),
        hygiene,
        energy,
    }
}

fn parse_tasks_from_tags(tags: &Tags) -> Vec<BlobbiTaskProgress> {
    let mut tasks = Vec::new();

    let confirmed: Vec<String> = tags
        .iter()
        .filter_map(|t| {
            let name = t.kind().to_string();
            if name.ends_with("_confirmed") {
                t.content().map(|_| name.replace("_confirmed", ""))
            } else {
                None
            }
        })
        .collect();

    let progress_tags: Vec<(String, u32)> = tags
        .iter()
        .filter_map(|t| {
            let name = t.kind().to_string();
            if name.ends_with("_progress") {
                let id = name.replace("_progress", "");
                let val: u32 = t.content().and_then(|c| c.parse().ok())?;
                Some((id, val))
            } else {
                None
            }
        })
        .collect();

    let all_task_ids: &[&str] = &[
        TASK_FIRST_POST,
        TASK_POST_BLOBBI_PHOTO,
        TASK_INTERACT_7,
        TASK_SHARE_YOUR_EGG,
        QUEST_PUBLISH_5_POSTS,
        QUEST_SHARE_SONG,
        QUEST_USE_BLOBBI_HASHTAGS,
        QUEST_MENTION_USER,
        QUEST_REPLY_TO_POST,
        QUEST_FOLLOW_5_USERS,
        QUEST_REACT_TO_5_POSTS,
        QUEST_REPOST_3_POSTS,
        QUEST_REACT_OR_REPOST_BLOBBI,
        QUEST_MAINTAIN_STATS,
        QUEST_EDIT_PROFILE,
    ];

    let mut seen = std::collections::HashSet::new();
    for id in confirmed
        .iter()
        .chain(progress_tags.iter().map(|(id, _)| id))
    {
        if seen.insert(id.clone()) {
            let is_completed = confirmed.contains(id);
            let progress = progress_tags
                .iter()
                .find(|(pid, _)| pid == id)
                .map(|(_, v)| *v)
                .unwrap_or(0);
            let target = match id {
                t if t == TASK_INTERACT_7 => 7,
                t if t == QUEST_REPOST_3_POSTS => 3,
                t if t == QUEST_PUBLISH_5_POSTS || t == QUEST_REACT_TO_5_POSTS || t == QUEST_FOLLOW_5_USERS => 5,
                _ => 1,
            };
            tasks.push(BlobbiTaskProgress {
                id: id.clone(),
                completed: is_completed,
                progress,
                target,
            });
        }
    }

    for id in all_task_ids {
        let id = id.to_string();
        if !seen.contains(&id) {
            tasks.push(BlobbiTaskProgress {
                id,
                completed: false,
                progress: 0,
                target: 0,
            });
        }
    }

    tasks
}

pub fn parse_blobbi_from_event(event: &Event) -> BlobbiCompanion {
    let tags = &event.tags;
    let stage = tag_value(tags, TAG_STAGE)
        .map(|s| BlobbiStage::from_str(&s))
        .unwrap_or_default();

    let state_str = tag_value(tags, TAG_STATE).unwrap_or_default();
    let legacy_is_sleeping = tag_bool(tags, TAG_IS_SLEEPING);

    let state = BlobbiState::from_str(&state_str);

    let is_sleeping = legacy_is_sleeping || state == BlobbiState::Sleeping;

    let seed = tag_value(tags, TAG_SEED);

    let seed_traits = seed.as_ref().map(|s| crate::components::blobbi::core::seed::derive_visual_traits_from_seed(s));

    let has_all_stats = tag_f64(tags, TAG_HUNGER).is_some()
        && tag_f64(tags, TAG_HAPPINESS).is_some()
        && tag_f64(tags, TAG_HEALTH).is_some()
        && tag_f64(tags, TAG_HYGIENE).is_some()
        && tag_f64(tags, TAG_ENERGY).is_some();

    let stats = if has_all_stats {
        BlobbiStats {
            hunger: clamp_stat(tag_f64(tags, TAG_HUNGER).unwrap_or(STAT_DEFAULT)),
            happiness: clamp_stat(tag_f64(tags, TAG_HAPPINESS).unwrap_or(STAT_DEFAULT)),
            health: clamp_stat(tag_f64(tags, TAG_HEALTH).unwrap_or(STAT_DEFAULT)),
            hygiene: clamp_stat(tag_f64(tags, TAG_HYGIENE).unwrap_or(STAT_DEFAULT)),
            energy: clamp_stat(tag_f64(tags, TAG_ENERGY).unwrap_or(STAT_DEFAULT)),
        }
    } else {
        recover_missing_stats(tags)
    };

    let base_color = tag_value(tags, TAG_BASE_COLOR)
        .filter(|v| !v.is_empty())
        .or_else(|| seed_traits.as_ref().map(|t| t.base_color.clone()))
        .unwrap_or_else(|| DEFAULT_BASE_COLORS[0].to_string());

    let secondary_color = tag_value(tags, TAG_SECONDARY_COLOR)
        .filter(|v| !v.is_empty())
        .or_else(|| seed_traits.as_ref().and_then(|t| t.secondary_color.clone()));

    let eye_color = tag_value(tags, TAG_EYE_COLOR)
        .filter(|v| !v.is_empty())
        .or_else(|| seed_traits.as_ref().map(|t| t.eye_color.clone()))
        .unwrap_or_else(|| DEFAULT_EYE_COLORS[0].to_string());

    let pattern = tag_value(tags, TAG_PATTERN)
        .filter(|v| !v.is_empty())
        .or_else(|| seed_traits.as_ref().map(|t| t.pattern.clone()))
        .unwrap_or_else(|| DEFAULT_PATTERNS[0].to_string());

    let special_mark = tag_value(tags, TAG_SPECIAL_MARK)
        .filter(|v| !v.is_empty())
        .or_else(|| seed_traits.as_ref().map(|t| t.special_mark.clone()))
        .unwrap_or_else(|| DEFAULT_SPECIAL_MARKS[0].to_string());

    let size = tag_value(tags, TAG_SIZE)
        .filter(|v| !v.is_empty())
        .or_else(|| seed_traits.as_ref().map(|t| t.size.clone()))
        .unwrap_or_else(|| DEFAULT_SIZES[0].to_string());

    let visual_traits = BlobbiVisualTraits {
        base_color,
        secondary_color,
        eye_color,
        pattern,
        special_mark,
        size,
    };

    let personality = BlobbiPersonality {
        traits: tag_values(tags, TAG_TRAIT),
        mood: tag_value(tags, TAG_MOOD).unwrap_or_default(),
        favorite_food: tag_value(tags, TAG_FAVORITE_FOOD),
        voice_type: tag_value(tags, TAG_VOICE_TYPE),
        title: tag_value(tags, TAG_TITLE),
        skills: tag_values(tags, TAG_SKILL),
    };

    let start_incubation = if matches!(state, BlobbiState::Incubating) {
        tag_u64(tags, TAG_STATE_STARTED_AT)
    } else {
        tag_u64(tags, TAG_START_INCUBATION)
    };

    let start_evolution = if matches!(state, BlobbiState::Evolving) {
        tag_u64(tags, TAG_STATE_STARTED_AT)
    } else {
        tag_u64(tags, TAG_START_EVOLUTION)
    };

    let state_started_at = if matches!(state, BlobbiState::Incubating | BlobbiState::Evolving) {
        tag_u64(tags, TAG_STATE_STARTED_AT)
    } else {
        None
    };

    let tasks_completed = tag_values(tags, TAG_TASK_COMPLETED);
    let tasks = parse_tasks_from_tags(tags);

    BlobbiCompanion {
        event_id: Some(event.id.to_hex()),
        d: tag_value(tags, TAG_D).unwrap_or_default(),
        name: tag_value(tags, TAG_NAME).unwrap_or_default(),
        stage,
        state,
        stats,
        visual_traits,
        personality,
        generation: tag_u32(tags, TAG_GENERATION).unwrap_or(1),
        breeding_ready: tag_bool(tags, TAG_BREEDING_READY),
        experience: tag_u64(tags, TAG_EXPERIENCE).unwrap_or(0),
        care_streak: tag_u32(tags, TAG_CARE_STREAK).unwrap_or(0),
        is_sleeping,
        sleep_started_at: tag_u64(tags, TAG_SLEEP_STARTED_AT),
        last_sleep_update: tag_u64(tags, TAG_LAST_SLEEP_UPDATE),
        source: tag_value(tags, TAG_SOURCE),
        last_interaction: tag_u64(tags, TAG_LAST_INTERACTION),
        last_decay_at: tag_u64(tags, TAG_LAST_DECAY_AT),
        last_meal: tag_u64(tags, TAG_LAST_MEAL),
        last_clean: tag_u64(tags, TAG_LAST_CLEAN),
        last_warm: tag_u64(tags, TAG_LAST_WARM),
        last_talk: tag_u64(tags, TAG_LAST_TALK),
        last_sing: tag_u64(tags, TAG_LAST_SING),
        last_medicine: tag_u64(tags, TAG_LAST_MEDICINE),
        last_check: tag_u64(tags, TAG_LAST_CHECK),
        seed,
        adult_type: tag_value(tags, TAG_ADULT_TYPE),
        evolution_time: tag_u64(tags, TAG_EVOLUTION_TIME),
        is_dirty: tag_bool(tags, TAG_IS_DIRTY),
        has_buff: tag_value(tags, TAG_HAS_BUFF),
        has_debuff: tag_value(tags, TAG_HAS_DEBUFF),
        incubation_time: tag_u64(tags, TAG_INCUBATION_TIME),
        incubation_progress: tag_f64(tags, TAG_INCUBATION_PROGRESS),
        egg_temperature: tag_f64(tags, TAG_EGG_TEMPERATURE),
        egg_status: tag_value(tags, TAG_EGG_STATUS),
        shell_integrity: tag_f64(tags, TAG_SHELL_INTEGRITY),
        start_incubation,
        start_evolution,
        state_started_at,
        tasks_completed,
        care_streak_last_at: tag_u64(tags, TAG_CARE_STREAK_LAST_AT),
        care_streak_last_day: tag_value(tags, TAG_CARE_STREAK_LAST_DAY),
        theme: tag_value(tags, TAG_THEME),
        crossover_app: tag_value(tags, TAG_CROSSOVER_APP),
        manifestation: tag_value(tags, TAG_MANIFESTATION),
        blessing: tag_value(tags, TAG_BLESSING),
        visual_effect: tag_value(tags, TAG_VISUAL_EFFECT),
        adopted_by: tag_value(tags, TAG_ADOPTED_BY),
        adopted_from: tag_value(tags, TAG_ADOPTED_FROM),
        visible_to_others: tag_value(tags, TAG_VISIBLE_TO_OTHERS).map(|v| v == "true"),
        tasks,
        content: event.content.clone(),
        raw_event: Some(event.clone()),
    }
}

pub fn parse_profile_from_event(event: &Event) -> BlobbonautProfile {
    let tags = &event.tags;

    let has = tag_values(tags, TAG_HAS);
    let storage: Vec<StorageItem> = tag_values(tags, TAG_STORAGE)
        .iter()
        .filter_map(|s| StorageItem::parse(s))
        .collect();

    let achievements = tag_values(tags, TAG_ACHIEVEMENTS);

    let lifetime_blobbis = tag_u32(tags, TAG_LIFETIME_BLOBBIS)
        .or_else(|| tag_u32(tags, "lifetimeBlobbis"))
        .unwrap_or(0);

    let starter_blobbi =
        tag_value(tags, TAG_STARTER_BLOBBI).or_else(|| tag_value(tags, "starterBlobbi"));

    let favorite_blobbi =
        tag_value(tags, TAG_FAVORITE_BLOBBI).or_else(|| tag_value(tags, "favoriteBlobbi"));

    let petting_level = tag_u32(tags, TAG_PETTING_LEVEL)
        .or_else(|| tag_u32(tags, "pettingLevel"))
        .unwrap_or(0);

    BlobbonautProfile {
        d: tag_value(tags, TAG_D).unwrap_or_default(),
        name: tag_value(tags, TAG_NAME).unwrap_or_default(),
        coins: tag_u64(tags, TAG_COINS).unwrap_or(0),
        petting_level,
        level: tag_u32(tags, TAG_LEVEL).unwrap_or(1),
        current_companion: tag_value(tags, TAG_CURRENT_COMPANION),
        onboarding_done: tag_bool(tags, TAG_ONBOARDING_DONE),
        has,
        storage,
        achievements,
        lifetime_blobbis,
        starter_blobbi,
        favorite_blobbi,
        style: tag_value(tags, TAG_STYLE),
        background: tag_value(tags, TAG_BACKGROUND),
        title: tag_value(tags, TAG_TITLE),
        content_json: event.content.clone(),
        raw_event: Some(event.clone()),
    }
}
