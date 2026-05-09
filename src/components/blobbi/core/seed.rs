use sha2::{Digest, Sha256};

use crate::components::blobbi::core::types::BlobbiVisualTraits;
use crate::utils::nip_bb::constants::*;

pub fn derive_seed(pubkey: &str, d_tag: &str, created_at: u64) -> String {
    let input = format!("blobbi:v1|{}:{}:{}", pubkey, d_tag, created_at);
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn seed_byte_at(seed: &str, offset: usize) -> u8 {
    let hex_chars: Vec<char> = seed.chars().collect();
    if hex_chars.len() < offset + 2 {
        return 0;
    }
    let byte_str = format!("{}{}", hex_chars[offset], hex_chars[offset + 1]);
    u8::from_str_radix(&byte_str, 16).unwrap_or(0)
}

pub fn derive_visual_traits_from_seed(seed: &str) -> BlobbiVisualTraits {
    let base_color = DEFAULT_BASE_COLORS
        [(seed_byte_at(seed, 0) as usize) % DEFAULT_BASE_COLORS.len()]
    .to_string();
    let secondary_color = Some(
        DEFAULT_SECONDARY_COLORS[(seed_byte_at(seed, 8) as usize) % DEFAULT_SECONDARY_COLORS.len()]
            .to_string(),
    );
    let eye_color = DEFAULT_EYE_COLORS
        [(seed_byte_at(seed, 12) as usize) % DEFAULT_EYE_COLORS.len()]
    .to_string();
    let pattern =
        DEFAULT_PATTERNS[(seed_byte_at(seed, 16) as usize) % DEFAULT_PATTERNS.len()].to_string();
    let special_mark = DEFAULT_SPECIAL_MARKS
        [(seed_byte_at(seed, 24) as usize) % DEFAULT_SPECIAL_MARKS.len()]
    .to_string();
    let size = DEFAULT_SIZES[(seed_byte_at(seed, 32) as usize) % DEFAULT_SIZES.len()].to_string();

    BlobbiVisualTraits {
        base_color,
        secondary_color,
        eye_color,
        pattern,
        special_mark,
        size,
    }
}

pub fn djb2_hash(input: &str) -> u32 {
    let mut hash: u32 = 5381;
    for byte in input.bytes() {
        hash = ((hash << 5).wrapping_add(hash)).wrapping_add(byte as u32);
    }
    hash
}

pub fn derive_adult_type(
    pubkey: &str,
    name: &str,
    birth_time: u64,
    care_days: u32,
    stats_hash: &str,
) -> String {
    let factors = format!(
        "{}|{}|{}|{}|{}",
        pubkey, name, birth_time, care_days, stats_hash
    );
    let hash = djb2_hash(&factors);
    let idx = (hash as usize) % ADULT_TYPES.len();
    ADULT_TYPES[idx].to_string()
}

pub fn derive_adult_type_from_seed(seed: &str) -> String {
    let idx = (seed_byte_at(seed, 40) as usize) % ADULT_TYPES.len();
    ADULT_TYPES[idx].to_string()
}

pub fn generate_eye_color_at_hatching(roll: f64) -> &'static str {
    let common: &[&str] = &["#2D3748", "#4A5568", "#1A202C"];
    let uncommon: &[&str] = &["#3182CE", "#38A169", "#D69E2E"];
    let rare: &[&str] = &["#9F7AEA", "#ED64A6", "#F56565"];
    let legendary: &[&str] = &["#00F5FF", "#FFD700", "#FF1493"];

    let (tier, sub_idx) = if roll < 0.50 {
        (common, (roll / 0.50 * 3.0).floor() as usize)
    } else if roll < 0.80 {
        (uncommon, ((roll - 0.50) / 0.30 * 3.0).floor() as usize)
    } else if roll < 0.95 {
        (rare, ((roll - 0.80) / 0.15 * 3.0).floor() as usize)
    } else {
        (legendary, ((roll - 0.95) / 0.05 * 3.0).floor() as usize)
    };

    tier[sub_idx.min(tier.len() - 1)]
}

pub fn generate_random_personality(roll: f64) -> &'static str {
    let personalities = ["brave", "shy", "curious", "gentle", "playful", "calm"];
    let idx = (roll * personalities.len() as f64).floor() as usize;
    personalities[idx.min(personalities.len() - 1)]
}

pub fn generate_random_trait(roll: f64) -> &'static str {
    let traits = [
        "night_owl",
        "early_bird",
        "social",
        "independent",
        "adventurous",
        "cautious",
    ];
    let idx = (roll * traits.len() as f64).floor() as usize;
    traits[idx.min(traits.len() - 1)]
}

pub fn generate_random_voice(roll: f64) -> &'static str {
    let voices = ["squeaky", "melodic", "chirpy", "soft", "bubbly"];
    let idx = (roll * voices.len() as f64).floor() as usize;
    voices[idx.min(voices.len() - 1)]
}

pub fn generate_random_food(roll: f64) -> &'static str {
    let foods = [
        "glowberries",
        "moonfruits",
        "starseeds",
        "dewdrops",
        "crystalnuts",
    ];
    let idx = (roll * foods.len() as f64).floor() as usize;
    foods[idx.min(foods.len() - 1)]
}

pub fn generate_random_title(roll: f64) -> Option<&'static str> {
    if roll > 0.10 {
        return None;
    }
    let sub_roll = roll / 0.10;
    let idx = (sub_roll * TITLES.len() as f64).floor() as usize;
    Some(TITLES[idx.min(TITLES.len() - 1)])
}

pub fn generate_random_memory(roll: f64) -> Option<(&'static str, &'static str)> {
    if roll > 0.15 {
        return None;
    }
    let titles = [
        "Woke with a Yawn",
        "Blinking into Light",
        "First Wiggle",
        "Broke the Shell",
        "First Gaze",
        "Whispered by the Wind",
    ];
    let descriptions = [
        "Opened eyes to the world for the first time",
        "The shell cracked under the full moon's light",
        "Greeted by soft glowing moss",
        "Heard distant humming during birth",
        "Felt warmth and safety while hatching",
        "Emerged during a gentle rainstorm",
    ];
    let sub = (roll / 0.15 * titles.len() as f64).floor() as usize;
    let idx = sub.min(titles.len() - 1);
    Some((titles[idx], descriptions[idx]))
}

pub fn generate_random_blessing(roll: f64) -> Option<&'static str> {
    if roll > 0.10 {
        return None;
    }
    let blessings = [
        "telepathic",
        "keen_sense",
        "light_heal",
        "night_vision",
        "inner_peace",
        "sun_gifted",
        "eternal_grace",
        "blessing_of_light",
        "soul_touch",
    ];
    let sub = (roll / 0.10 * blessings.len() as f64).floor() as usize;
    Some(blessings[sub.min(blessings.len() - 1)])
}
