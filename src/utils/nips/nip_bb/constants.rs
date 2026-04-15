#[allow(dead_code)]
use nostr_sdk::Kind;

pub const KIND_BLOBBI_STATE: u16 = 31124;
pub const KIND_BLOBBONAUT_PROFILE: u16 = 31125;
pub const KIND_BLOBBI_INTERACTION: u16 = 14919;
#[allow(dead_code)]
pub const KIND_BLOBBI_BREEDING: u16 = 14920;
#[allow(dead_code)]
pub const KIND_BLOBBI_RECORD: u16 = 14921;

pub const BLOBBI_ECOSYSTEM_TAG: &str = "blobbi:ecosystem:v1";
pub const BLOBBI_TOPIC_TAG: &str = "blobbi";
pub const CLIENT_TAG: &str = "nostr.blue";

pub const STAT_MIN: f64 = 0.0;
pub const STAT_MAX: f64 = 100.0;
pub const STAT_DEFAULT: f64 = 80.0;

pub const INITIAL_BLOBBONAUT_COINS: u64 = 500;
pub const ADOPTION_FEE: u64 = 100;

pub const DIVINE_EGG_CHANCE: f64 = 0.40;
pub const DIVINE_PRIMARY_GREEN: &str = "#55C4A2";

pub const MIN_ACTIONS_PER_DAY: u32 = 3;
pub const CARE_STREAK_GRACE_HOURS: u64 = 36;

pub const HATCH_MIN_DAYS: u64 = 7;
pub const HATCH_MIN_EXPERIENCE: u64 = 40;
pub const HATCH_MIN_HEALTH: f64 = 50.0;
pub const HATCH_MIN_CARE_DAYS: u64 = 4;

pub const EVOLVE_MIN_DAYS: u64 = 10;
pub const EVOLVE_MIN_EXPERIENCE: u64 = 150;
pub const EVOLVE_MIN_INTERACTIONS: u64 = 50;
pub const EVOLVE_MIN_HAPPINESS: f64 = 70.0;
pub const EVOLVE_MIN_HEALTH: f64 = 80.0;

pub const SLEEP_ENERGY_PER_BLOCK: f64 = 10.0;
pub const SLEEP_BLOCK_SECS: u64 = 30 * 60;

pub const INTERACTION_COOLDOWN_SECS: u64 = 3;

pub const STAGE_EGG: &str = "egg";
pub const STAGE_BABY: &str = "baby";
pub const STAGE_ADULT: &str = "adult";
pub const STATE_ACTIVE: &str = "active";
pub const STATE_SLEEPING: &str = "sleeping";
pub const STATE_HIBERNATING: &str = "hibernating";
pub const TAG_ORIGIN: &str = "origin";

pub fn blobbi_state_kind() -> Kind {
    Kind::Custom(KIND_BLOBBI_STATE)
}

pub fn blobbonaut_profile_kind() -> Kind {
    Kind::Custom(KIND_BLOBBONAUT_PROFILE)
}

pub fn blobbi_interaction_kind() -> Kind {
    Kind::Custom(KIND_BLOBBI_INTERACTION)
}

pub fn blobbi_breeding_kind() -> Kind {
    Kind::Custom(KIND_BLOBBI_BREEDING)
}

pub fn blobbi_record_kind() -> Kind {
    Kind::Custom(KIND_BLOBBI_RECORD)
}

pub fn profile_d_tag(pubkey_hex: &str) -> String {
    let prefix = &pubkey_hex[..12.min(pubkey_hex.len())];
    format!("blobbonaut-{}", prefix)
}

pub fn blobbi_d_tag(pubkey_hex: &str, pet_id: &str) -> String {
    let prefix = &pubkey_hex[..12.min(pubkey_hex.len())];
    let pet_prefix = &pet_id[..10.min(pet_id.len())];
    format!("blobbi-{}-{}", prefix, pet_prefix)
}

pub const TAG_D: &str = "d";
pub const TAG_B: &str = "b";
pub const TAG_T: &str = "t";
pub const TAG_CLIENT: &str = "client";
pub const TAG_SOURCE: &str = "source";
pub const TAG_NAME: &str = "name";
pub const TAG_SEED: &str = "seed";
pub const TAG_STAGE: &str = "stage";
pub const TAG_STATE: &str = "state";
pub const TAG_BREEDING_READY: &str = "breeding_ready";
pub const TAG_GENERATION: &str = "generation";
pub const TAG_EXPERIENCE: &str = "experience";
pub const TAG_CARE_STREAK: &str = "care_streak";
pub const TAG_LAST_INTERACTION: &str = "last_interaction";
pub const TAG_LAST_DECAY_AT: &str = "last_decay_at";

pub const TAG_HUNGER: &str = "hunger";
pub const TAG_HAPPINESS: &str = "happiness";
pub const TAG_HEALTH: &str = "health";
pub const TAG_HYGIENE: &str = "hygiene";
pub const TAG_ENERGY: &str = "energy";

pub const TAG_IS_SLEEPING: &str = "is_sleeping";
pub const TAG_SLEEP_STARTED_AT: &str = "sleep_started_at";
pub const TAG_LAST_SLEEP_UPDATE: &str = "last_sleep_update";
pub const TAG_IS_DIRTY: &str = "is_dirty";
pub const TAG_HAS_BUFF: &str = "has_buff";
pub const TAG_HAS_DEBUFF: &str = "has_debuff";

pub const TAG_INCUBATION_TIME: &str = "incubation_time";
pub const TAG_INCUBATION_PROGRESS: &str = "incubation_progress";
pub const TAG_EGG_TEMPERATURE: &str = "egg_temperature";
pub const TAG_EGG_STATUS: &str = "egg_status";
pub const TAG_SHELL_INTEGRITY: &str = "shell_integrity";
pub const TAG_START_INCUBATION: &str = "start_incubation";
pub const TAG_START_EVOLUTION: &str = "start_evolution";

pub const TAG_BASE_COLOR: &str = "base_color";
pub const TAG_SECONDARY_COLOR: &str = "secondary_color";
pub const TAG_EYE_COLOR: &str = "eye_color";
pub const TAG_PATTERN: &str = "pattern";
pub const TAG_SPECIAL_MARK: &str = "special_mark";
pub const TAG_SIZE: &str = "size";
pub const TAG_ADULT_TYPE: &str = "adult_type";
pub const TAG_EVOLUTION_TIME: &str = "evolution_time";
pub const TAG_THEME: &str = "theme";
pub const TAG_CROSSOVER_APP: &str = "crossover_app";
pub const TAG_MANIFESTATION: &str = "manifestation";
pub const TAG_BLESSING: &str = "blessing";
pub const TAG_VISUAL_EFFECT: &str = "visual_effect";

pub const TAG_PERSONALITY: &str = "personality";
pub const TAG_TRAIT: &str = "trait";
pub const TAG_MOOD: &str = "mood";
pub const TAG_FAVORITE_FOOD: &str = "favorite_food";
pub const TAG_VOICE_TYPE: &str = "voice_type";
pub const TAG_TITLE: &str = "title";
pub const TAG_SKILL: &str = "skill";

pub const TAG_LAST_MEAL: &str = "last_meal";
pub const TAG_LAST_CLEAN: &str = "last_clean";
pub const TAG_LAST_WARM: &str = "last_warm";
pub const TAG_LAST_TALK: &str = "last_talk";
pub const TAG_LAST_CHECK: &str = "last_check";
pub const TAG_LAST_SING: &str = "last_sing";
pub const TAG_LAST_MEDICINE: &str = "last_medicine";

pub const TAG_ADOPTED_BY: &str = "adopted_by";
pub const TAG_ADOPTED_FROM: &str = "adopted_from";
pub const TAG_CURRENT_LOCATION: &str = "current_location";
pub const TAG_IN_PARTY: &str = "in_party";
pub const TAG_VISIBLE_TO_OTHERS: &str = "visible_to_others";

pub const TAG_BLOBBI_ID: &str = "blobbi_id";
pub const TAG_ACTION: &str = "action";
pub const TAG_ACTION_CATEGORY: &str = "action_category";
pub const TAG_STAT_CHANGE: &str = "stat_change";
pub const TAG_ITEM_USED: &str = "item_used";
pub const TAG_EXPERIENCE_GAINED: &str = "experience_gained";
pub const TAG_CARE_POINTS: &str = "care_points";

pub const TAG_PARENT_A: &str = "parent_a";
pub const TAG_PARENT_B: &str = "parent_b";
pub const TAG_OWNER_A: &str = "owner_a";
pub const TAG_OWNER_B: &str = "owner_b";
pub const TAG_BREED_TIME: &str = "breed_time";
pub const TAG_SUCCESS: &str = "success";
pub const TAG_OFFSPRING_ID: &str = "offspring_id";

pub const TAG_RECORD_TYPE: &str = "record_type";
pub const TAG_BLOBBI_MOOD_BEFORE: &str = "blobbi_mood_before";
pub const TAG_BLOBBI_MOOD_AFTER: &str = "blobbi_mood_after";

pub const TAG_COINS: &str = "coins";
pub const TAG_PETTING_LEVEL: &str = "petting_level";
pub const TAG_LIFETIME_BLOBBIS: &str = "lifetime_blobbis";
pub const TAG_FAVORITE_BLOBBI: &str = "favorite_blobbi";
pub const TAG_STARTER_BLOBBI: &str = "starter_blobbi";
pub const TAG_CURRENT_COMPANION: &str = "current_companion";
pub const TAG_ONBOARDING_DONE: &str = "onboarding_done";
pub const TAG_HAS: &str = "has";
pub const TAG_STORAGE: &str = "storage";
pub const TAG_ACHIEVEMENTS: &str = "achievements";
pub const TAG_STYLE: &str = "style";
pub const TAG_BACKGROUND: &str = "background";

pub const TASK_FIRST_POST: &str = "first_post";
pub const TASK_POST_BLOBBI_PHOTO: &str = "post_blobbi_photo";
pub const TASK_INTERACT_6: &str = "interact_6";
pub const TASK_SHELL_INTEGRITY_ABOVE_50: &str = "shell_integrity_above_50";

pub const QUEST_PUBLISH_5_POSTS: &str = "publish_5_posts";
pub const QUEST_SHARE_SONG: &str = "share_song";
pub const QUEST_USE_BLOBBI_HASHTAGS: &str = "use_blobbi_hashtags";
pub const QUEST_MENTION_USER: &str = "mention_user";
pub const QUEST_REPLY_TO_POST: &str = "reply_to_post";
pub const QUEST_FOLLOW_5_USERS: &str = "follow_5_users";
pub const QUEST_REACT_TO_5_POSTS: &str = "react_to_5_posts";
pub const QUEST_REPOST_3_POSTS: &str = "repost_3_posts";
pub const QUEST_REACT_OR_REPOST_BLOBBI: &str = "react_or_repost_blobbi";

pub const DEFAULT_BASE_COLORS: &[&str] = &[
    "#ffffff", "#f2f2f2", "#e6e6ff", "#99ccff", "#ccffcc", "#ffffcc", "#cc99ff", "#ffb3cc",
    "#66ffcc", "#6633cc", "#ff3399", "#00ffff",
];

pub const DEFAULT_SECONDARY_COLORS: &[&str] = &[
    "#cccccc", "#f0f0f0", "#aabbcc", "#99ccff", "#ccffcc", "#ffcc99", "#ff99ff", "#9966ff",
    "#66cccc", "#9933ff", "#ff3399", "#00ffcc",
];

pub const DEFAULT_EYE_COLORS: &[&str] = &[
    "#2D3748", "#4A5568", "#1A202C", "#3182CE", "#38A169", "#D69E2E", "#9F7AEA", "#ED64A6",
    "#F56565", "#00F5FF", "#FFD700", "#FF1493",
];

pub const DEFAULT_PATTERNS: &[&str] = &["gradient", "solid", "speckled", "striped"];

pub const DEFAULT_SPECIAL_MARKS: &[&str] = &[
    "dot_center",
    "oval_spots",
    "ring_mark",
    "rune_top",
    "sigil_eye",
];

pub const DEFAULT_SIZES: &[&str] = &["small", "medium", "large", "tiny"];

pub const DEFAULT_EGG_STATUSES: &[&str] = &["cracking", "warm", "glowing", "pulsing"];

pub const ADULT_TYPES: &[&str] = &[
    "pandi", "owli", "catti", "froggi", "cloudi", "crysti", "bloomi", "starri", "flammi", "droppi",
    "breezy", "rocky", "cacti", "mushie", "leafy", "rosey",
];

pub const TITLES: &[&str] = &[
    "Hatchling",
    "Watcher of the Nest",
    "Tender of Flames",
    "Whisperer",
    "Echo of Ancients",
    "Shellbound Hero",
    "Defender of the Grove",
    "The Primordial",
];

pub const CARE_ACTIONS: &[&str] = &["feed", "play", "clean", "medicine"];

pub fn is_care_action(action: &str) -> bool {
    CARE_ACTIONS.contains(&action)
}

pub fn tag_priority(name: &str) -> u32 {
    match name {
        "d" => 0,
        "b" => 1,
        "t" => 2,
        "client" => 3,
        "name" => 4,
        "source" => 10,
        "stage" => 200,
        "state" => 201,
        "breeding_ready" => 202,
        "generation" => 203,
        "is_sleeping" => 204,
        "hunger" => 300,
        "happiness" => 301,
        "health" => 302,
        "hygiene" => 303,
        "energy" => 304,
        "experience" => 305,
        "care_streak" => 306,
        "last_interaction" => 307,
        "base_color" => 400,
        "secondary_color" => 401,
        "pattern" => 402,
        "eye_color" => 403,
        "special_mark" => 404,
        "size" => 405,
        "adult_type" => 406,
        "personality" | "trait" => 500,
        "mood" => 501,
        "seed" => 502,
        _ => 1000,
    }
}
