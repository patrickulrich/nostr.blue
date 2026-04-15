use crate::components::blobbi::core::seed::{
    derive_adult_type, derive_seed, generate_eye_color_at_hatching, generate_random_blessing,
    generate_random_food, generate_random_memory, generate_random_personality,
    generate_random_title, generate_random_visual_effect, generate_random_voice,
};
use crate::components::blobbi::core::streak::{check_evolve_readiness, check_hatch_readiness};
use crate::components::blobbi::core::types::{
    BlobbiCompanion, BlobbiStage, BlobbiState, BlobbiStats,
};
use crate::utils::nip_bb::constants::*;

pub fn can_hatch(blobbi: &BlobbiCompanion) -> bool {
    check_hatch_readiness(blobbi).ready
}

pub fn can_evolve(blobbi: &BlobbiCompanion) -> bool {
    check_evolve_readiness(blobbi).ready
}

pub fn hatch_egg(blobbi: &BlobbiCompanion, pubkey: &str) -> BlobbiCompanion {
    if !blobbi.is_egg() {
        return blobbi.clone();
    }

    let now = nostr_sdk::Timestamp::now().as_secs();
    let mut baby = blobbi.clone();

    baby.stage = BlobbiStage::Baby;
    baby.state = BlobbiState::Active;
    baby.is_sleeping = false;
    baby.sleep_started_at = None;
    baby.last_sleep_update = None;

    baby.stats = BlobbiStats {
        hunger: baby.stats.hunger,
        happiness: (baby.stats.happiness + 20.0)
            .round()
            .clamp(STAT_MIN, STAT_MAX),
        health: baby.stats.health,
        hygiene: baby.stats.hygiene,
        energy: (baby.stats.energy + 15.0).round().clamp(STAT_MIN, STAT_MAX),
    };

    let seed = baby
        .seed
        .clone()
        .unwrap_or_else(|| derive_seed(pubkey, &baby.d, baby.last_interaction.unwrap_or(now)));
    baby.seed = Some(seed.clone());

    let mut rng_val = 0.5;
    if let Ok(h) = u64::from_str_radix(&seed[..8.min(seed.len())], 16) {
        rng_val = (h % 1000) as f64 / 1000.0;
    }
    let rng2 = ((rng_val * 1000.0) as u64 % 1000) as f64 / 1000.0;
    let rng3 = ((rng_val * 1000000.0) as u64 % 1000) as f64 / 1000.0;
    let rng4 = ((rng_val * 1000000000.0) as u64 % 1000) as f64 / 1000.0;
    let rng5 = ((rng_val * 1e12) as u64 % 1000) as f64 / 1000.0;
    let rng6 = ((rng_val * 1e15) as u64 % 1000) as f64 / 1000.0;

    if baby.visual_traits.eye_color.is_empty() {
        baby.visual_traits.eye_color = generate_eye_color_at_hatching(rng_val).to_string();
    }

    if baby.personality.traits.is_empty() {
        baby.personality.traits = vec![generate_random_personality(rng2).to_string()];
    }

    if baby.personality.mood.is_empty() {
        baby.personality.mood = "joyful".to_string();
    }

    if baby.personality.favorite_food.is_none() {
        baby.personality.favorite_food = Some(generate_random_food(rng3).to_string());
    }

    if baby.personality.voice_type.is_none() {
        baby.personality.voice_type = Some(generate_random_voice(rng4).to_string());
    }

    if let Some(title) = generate_random_title(rng5) {
        baby.personality.title = Some(title.to_string());
    }

    if let Some(blessing) = generate_random_blessing(rng6) {
        baby.blessing = Some(blessing.to_string());
    }

    if let Some((mem_title, mem_desc)) = generate_random_memory(rng_val) {
        baby.manifestation = Some(format!("{}: {}", mem_title, mem_desc));
    }

    baby.incubation_time = None;
    baby.incubation_progress = None;
    baby.egg_temperature = None;
    baby.egg_status = None;
    baby.shell_integrity = None;
    baby.start_incubation = None;

    baby.tasks.clear();
    super::hatch_tasks::initialize_tasks_for_stage(&mut baby);

    baby.last_interaction = Some(now);
    baby.last_decay_at = Some(now);
    baby.evolution_time = Some(now);
    baby.source = Some("system".to_string());
    baby.experience = baby.experience.saturating_add(100);

    baby
}

pub fn evolve_baby(blobbi: &BlobbiCompanion, pubkey: &str) -> BlobbiCompanion {
    if !blobbi.is_baby() {
        return blobbi.clone();
    }

    let now = nostr_sdk::Timestamp::now().as_secs();
    let mut adult = blobbi.clone();

    adult.stage = BlobbiStage::Adult;
    adult.state = BlobbiState::Active;
    adult.is_sleeping = false;
    adult.sleep_started_at = None;
    adult.last_sleep_update = None;

    adult.stats = BlobbiStats {
        hunger: adult.stats.hunger,
        happiness: (adult.stats.happiness + 20.0)
            .round()
            .clamp(STAT_MIN, STAT_MAX),
        health: adult.stats.health,
        hygiene: adult.stats.hygiene,
        energy: (adult.stats.energy + 15.0)
            .round()
            .clamp(STAT_MIN, STAT_MAX),
    };

    if adult.adult_type.is_none() {
        let stats_hash = format!(
            "{}|{}|{}|{}|{}",
            adult.stats.hunger as u32,
            adult.stats.happiness as u32,
            adult.stats.health as u32,
            adult.stats.hygiene as u32,
            adult.stats.energy as u32
        );
        let adult_type = derive_adult_type(
            pubkey,
            &adult.name,
            adult.last_interaction.unwrap_or(now),
            adult.care_streak,
            &stats_hash,
        );
        adult.adult_type = Some(adult_type);
    }

    adult.breeding_ready = true;

    if let Some(ref seed) = adult.seed {
        if adult.visual_traits.base_color.is_empty() {
            adult.visual_traits =
                crate::components::blobbi::core::seed::derive_visual_traits_from_seed(seed);
        }
    }

    if let Some(eff) = generate_random_visual_effect(0.5) {
        if adult.visual_effect.is_none() {
            adult.visual_effect = Some(eff.to_string());
        }
    }

    adult.start_evolution = None;

    adult.tasks.clear();
    super::hatch_tasks::initialize_tasks_for_stage(&mut adult);

    adult.last_interaction = Some(now);
    adult.last_decay_at = Some(now);
    adult.evolution_time = Some(now);
    adult.source = Some("system".to_string());
    adult.experience = adult.experience.saturating_add(500);

    adult
}

pub fn can_transition(blobbi: &BlobbiCompanion) -> bool {
    match blobbi.stage {
        BlobbiStage::Egg => can_hatch(blobbi),
        BlobbiStage::Baby => can_evolve(blobbi),
        BlobbiStage::Adult => false,
    }
}

pub fn transition_stage(blobbi: &BlobbiCompanion, pubkey: &str) -> BlobbiCompanion {
    match blobbi.stage {
        BlobbiStage::Egg => hatch_egg(blobbi, pubkey),
        BlobbiStage::Baby => evolve_baby(blobbi, pubkey),
        BlobbiStage::Adult => blobbi.clone(),
    }
}
