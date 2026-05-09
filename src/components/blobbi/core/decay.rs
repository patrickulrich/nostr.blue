use crate::components::blobbi::core::types::{
    BlobbiCompanion, BlobbiStage, BlobbiState, BlobbiStats,
};
use crate::utils::nip_bb::constants::*;

fn clamp_stat(v: f64) -> f64 {
    v.round().clamp(STAT_MIN, STAT_MAX)
}

fn trunc_toward_zero(delta: f64) -> f64 {
    if delta > 0.0 {
        delta.floor()
    } else if delta < 0.0 {
        delta.ceil()
    } else {
        0.0
    }
}

pub fn apply_decay(pet: &BlobbiCompanion, now_secs: u64) -> BlobbiCompanion {
    let last = pet
        .last_decay_at
        .unwrap_or(pet.last_interaction.unwrap_or(now_secs));
    let elapsed_secs = now_secs.saturating_sub(last) as f64;
    if elapsed_secs < 360.0 {
        return pet.clone();
    }
    let hours = elapsed_secs / 3600.0;
    let capped_hours = hours.min(24.0);

    let mut result = pet.clone();

    if matches!(pet.state, BlobbiState::Incubating | BlobbiState::Evolving) {
        return result;
    }

    match pet.stage {
        BlobbiStage::Egg => apply_egg_decay(&mut result, capped_hours),
        BlobbiStage::Baby | BlobbiStage::Adult => apply_post_hatch_decay(&mut result, capped_hours),
    }

    result.last_decay_at = Some(now_secs);
    result
}

fn apply_egg_decay(pet: &mut BlobbiCompanion, hours: f64) {
    pet.stats.hunger = STAT_MAX;
    pet.stats.energy = STAT_MAX;

    let cur_temp = pet.egg_temperature.unwrap_or(STAT_DEFAULT);
    let new_temp = clamp_stat(cur_temp + trunc_toward_zero(-0.5 * hours));
    pet.egg_temperature = Some(new_temp);

    let cur_shell = pet.shell_integrity.unwrap_or(STAT_MAX);
    let shell_rate = if new_temp < 20.0 {
        -2.0
    } else if new_temp < 40.0 {
        -1.0
    } else if new_temp >= 90.0 && pet.stats.hygiene >= 80.0 {
        0.5
    } else {
        0.0
    };
    let new_shell = clamp_stat(cur_shell + (shell_rate * hours));
    pet.shell_integrity = Some(new_shell);
}

fn apply_post_hatch_decay(pet: &mut BlobbiCompanion, hours: f64) {
    if pet.state == BlobbiState::Hibernating {
        return;
    }

    let is_sleeping = pet.is_sleeping();

    let (hunger_rate, happiness_rate, energy_rate, hygiene_rate, health_base) = match pet.stage {
        BlobbiStage::Baby if is_sleeping => (-7.0, -4.0, 6.0, -5.0, -0.75),
        BlobbiStage::Baby => (-7.0, -4.0, -8.0, -5.0, -0.75),
        BlobbiStage::Adult if is_sleeping => (-4.5, -2.5, 5.0, -3.5, -0.4),
        BlobbiStage::Adult => (-4.5, -2.5, -5.0, -3.5, -0.4),
        _ => return,
    };

    let new_hunger = clamp_stat(pet.stats.hunger + trunc_toward_zero(hunger_rate * hours));
    let new_happiness = clamp_stat(pet.stats.happiness + trunc_toward_zero(happiness_rate * hours));
    let new_hygiene = clamp_stat(pet.stats.hygiene + trunc_toward_zero(hygiene_rate * hours));
    let new_energy = clamp_stat(pet.stats.energy + trunc_toward_zero(energy_rate * hours));

    let mut health_rate = health_base;

    match pet.stage {
        BlobbiStage::Baby => {
            if new_hunger < 70.0 {
                health_rate -= 0.75;
            }
            if new_hunger < 40.0 {
                health_rate -= 1.25;
            }
            if new_hygiene < 70.0 {
                health_rate -= 0.75;
            }
            if new_hygiene < 40.0 {
                health_rate -= 1.25;
            }
            if new_energy < 50.0 {
                health_rate -= 0.5;
            }
            if new_energy < 25.0 {
                health_rate -= 1.0;
            }
            if new_happiness < 50.0 {
                health_rate -= 0.5;
            }
            if new_happiness < 25.0 {
                health_rate -= 1.0;
            }

            if new_hunger >= 80.0
                && new_happiness >= 80.0
                && new_hygiene >= 80.0
                && new_energy >= 80.0
            {
                health_rate += 1.5;
            }
        }
        BlobbiStage::Adult => {
            if new_hunger < 60.0 {
                health_rate -= 0.5;
            }
            if new_hunger < 30.0 {
                health_rate -= 1.0;
            }
            if new_hygiene < 60.0 {
                health_rate -= 0.5;
            }
            if new_hygiene < 30.0 {
                health_rate -= 1.0;
            }
            if new_energy < 40.0 {
                health_rate -= 0.4;
            }
            if new_energy < 20.0 {
                health_rate -= 0.8;
            }
            if new_happiness < 40.0 {
                health_rate -= 0.4;
            }
            if new_happiness < 20.0 {
                health_rate -= 0.8;
            }

            if new_hunger >= 80.0
                && new_happiness >= 80.0
                && new_hygiene >= 80.0
                && new_energy >= 80.0
            {
                health_rate += 1.0;
            }
        }
        _ => {}
    }

    let new_health = clamp_stat(pet.stats.health + trunc_toward_zero(health_rate * hours));

    pet.stats = BlobbiStats {
        hunger: new_hunger,
        happiness: new_happiness,
        health: new_health,
        hygiene: new_hygiene,
        energy: new_energy,
    };
}

pub fn apply_sleep_recovery(pet: &BlobbiCompanion, now_secs: u64) -> BlobbiCompanion {
    if !pet.is_sleeping() {
        return pet.clone();
    }

    let last_update = pet
        .last_sleep_update
        .unwrap_or(pet.sleep_started_at.unwrap_or(now_secs));
    let elapsed_secs = now_secs.saturating_sub(last_update) as f64;
    if elapsed_secs < SLEEP_BLOCK_SECS as f64 {
        return pet.clone();
    }

    let blocks = (elapsed_secs / SLEEP_BLOCK_SECS as f64).floor() as u64;
    let energy_gain = SLEEP_ENERGY_PER_BLOCK * blocks as f64;
    let new_energy = (pet.stats.energy + energy_gain)
        .round()
        .clamp(STAT_MIN, STAT_MAX);
    let maxed = new_energy >= STAT_MAX;

    let mut result = pet.clone();
    result.stats.energy = new_energy;
    result.last_sleep_update = Some(now_secs);

    if maxed {
        result.is_sleeping = false;
        result.state = BlobbiState::Active;
        result.sleep_started_at = None;
    }

    result
}

pub fn should_emit_shell_penalty(pet: &BlobbiCompanion) -> bool {
    pet.is_egg() && pet.shell_integrity.unwrap_or(STAT_MAX) < 50.0
}

pub fn get_decay_warning(pet: &BlobbiCompanion) -> Option<&'static str> {
    if pet.is_egg() {
        let temp = pet.egg_temperature.unwrap_or(STAT_DEFAULT);
        let shell = pet.shell_integrity.unwrap_or(STAT_MAX);
        if shell < 50.0 {
            return Some("Shell integrity critical!");
        }
        if temp < 40.0 {
            return Some("Temperature critically low!");
        }
        if pet.stats.hygiene < 20.0 {
            return Some("Shell very dirty!");
        }
        if pet.stats.happiness < 40.0 {
            return Some("Egg feeling neglected!");
        }
    } else {
        if pet.stats.hunger < 30.0 {
            return Some("Very hungry!");
        }
        if pet.stats.hygiene < 20.0 {
            return Some("Needs bath!");
        }
        if pet.stats.energy < 20.0 {
            return Some("Exhausted!");
        }
        if pet.stats.happiness < 30.0 {
            return Some("Feeling sad!");
        }
        if pet.stats.health < 30.0 {
            return Some("Feeling sick!");
        }
    }
    None
}

#[allow(dead_code)]
pub fn get_stat_status(stage: BlobbiStage, stat: &str, value: f64) -> &'static str {
    let (warning, critical) = if stage == BlobbiStage::Egg {
        (75.0, 45.0)
    } else if stage == BlobbiStage::Baby {
        (65.0, 35.0)
    } else {
        (60.0, 30.0)
    };

    let _ = stat;
    if value < critical {
        "critical"
    } else if value < warning {
        "warning"
    } else {
        "normal"
    }
}

#[allow(dead_code)]
pub fn get_visible_stats(stage: BlobbiStage) -> Vec<&'static str> {
    if stage == BlobbiStage::Egg {
        vec!["happiness", "hygiene", "health"]
    } else {
        vec!["hunger", "happiness", "health", "hygiene", "energy"]
    }
}
