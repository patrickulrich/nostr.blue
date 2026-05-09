use crate::components::blobbi::actions::action_types::BlobbiActionType;
use crate::utils::nip_bb::BlobbiStage;

pub const INTERACT_ACTIONS: &[&str] = &["feed", "play", "clean", "medicine", "sing", "play_music", "rest"];

#[derive(Clone, Debug)]
pub struct MissionDefinition {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub action: BlobbiActionType,
    pub matches_actions: &'static [&'static str],
    pub required_count: u32,
    pub reward_xp: u64,
    pub reward_coins: u64,
    pub weight: u32,
    pub required_stages: &'static [BlobbiStage],
}

pub static MISSION_POOL: &[MissionDefinition] = &[
    MissionDefinition {
        id: "interact_3",
        title: "Quick Care",
        description: "Interact with your Blobbi 3 times",
        action: BlobbiActionType::Play,
        matches_actions: INTERACT_ACTIONS,
        required_count: 3,
        reward_xp: 15,
        reward_coins: 10,
        weight: 10,
        required_stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "interact_6",
        title: "Attentive Caretaker",
        description: "Interact with your Blobbi 6 times",
        action: BlobbiActionType::Play,
        matches_actions: INTERACT_ACTIONS,
        required_count: 6,
        reward_xp: 30,
        reward_coins: 20,
        weight: 8,
        required_stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "feed_1",
        title: "Snack Time",
        description: "Feed your Blobbi once",
        action: BlobbiActionType::Feed,
        matches_actions: &["feed"],
        required_count: 1,
        reward_xp: 10,
        reward_coins: 10,
        weight: 10,
        required_stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "feed_2",
        title: "Hungry Blobbi",
        description: "Feed your Blobbi 2 times",
        action: BlobbiActionType::Feed,
        matches_actions: &["feed"],
        required_count: 2,
        reward_xp: 20,
        reward_coins: 15,
        weight: 8,
        required_stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "feed_3",
        title: "Feast Day",
        description: "Feed your Blobbi 3 times",
        action: BlobbiActionType::Feed,
        matches_actions: &["feed"],
        required_count: 3,
        reward_xp: 35,
        reward_coins: 25,
        weight: 5,
        required_stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "sleep_1",
        title: "Nap Time",
        description: "Put your Blobbi to sleep once",
        action: BlobbiActionType::Rest,
        matches_actions: &["rest"],
        required_count: 1,
        reward_xp: 15,
        reward_coins: 10,
        weight: 6,
        required_stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "take_photo_1",
        title: "Snapshot",
        description: "Take a photo of your Blobbi",
        action: BlobbiActionType::UseItem,
        matches_actions: &["take_photo"],
        required_count: 1,
        reward_xp: 25,
        reward_coins: 15,
        weight: 4,
        required_stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "take_photo_2",
        title: "Photo Album",
        description: "Take 2 photos of your Blobbi",
        action: BlobbiActionType::UseItem,
        matches_actions: &["take_photo"],
        required_count: 2,
        reward_xp: 40,
        reward_coins: 30,
        weight: 2,
        required_stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "clean_1",
        title: "Quick Cleanup",
        description: "Clean your Blobbi once",
        action: BlobbiActionType::Clean,
        matches_actions: &["clean"],
        required_count: 1,
        reward_xp: 10,
        reward_coins: 10,
        weight: 10,
        required_stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "clean_2",
        title: "Squeaky Clean",
        description: "Clean your Blobbi 2 times",
        action: BlobbiActionType::Clean,
        matches_actions: &["clean"],
        required_count: 2,
        reward_xp: 20,
        reward_coins: 15,
        weight: 6,
        required_stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "sing_1",
        title: "Sing Along",
        description: "Sing to your Blobbi once",
        action: BlobbiActionType::Sing,
        matches_actions: &["sing"],
        required_count: 1,
        reward_xp: 15,
        reward_coins: 10,
        weight: 6,
        required_stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "sing_2",
        title: "Karaoke Session",
        description: "Sing to your Blobbi 2 times",
        action: BlobbiActionType::Sing,
        matches_actions: &["sing"],
        required_count: 2,
        reward_xp: 25,
        reward_coins: 20,
        weight: 3,
        required_stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "play_music_1",
        title: "DJ Time",
        description: "Play music once",
        action: BlobbiActionType::PlayMusic,
        matches_actions: &["play_music"],
        required_count: 1,
        reward_xp: 15,
        reward_coins: 10,
        weight: 6,
        required_stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "play_music_2",
        title: "Music Marathon",
        description: "Play music 2 times",
        action: BlobbiActionType::PlayMusic,
        matches_actions: &["play_music"],
        required_count: 2,
        reward_xp: 25,
        reward_coins: 20,
        weight: 3,
        required_stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "medicine_1",
        title: "Health Check",
        description: "Give medicine once",
        action: BlobbiActionType::Medicine,
        matches_actions: &["medicine"],
        required_count: 1,
        reward_xp: 20,
        reward_coins: 15,
        weight: 5,
        required_stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
    },
    MissionDefinition {
        id: "medicine_2",
        title: "Doctor Visit",
        description: "Give medicine 2 times",
        action: BlobbiActionType::Medicine,
        matches_actions: &["medicine"],
        required_count: 2,
        reward_xp: 35,
        reward_coins: 25,
        weight: 3,
        required_stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
    },
];

fn mulberry32(seed: u32) -> impl FnMut() -> f64 {
    let mut state = seed;
    move || {
        state = state.wrapping_add(0x6D2B79F5);
        let mut t = state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        let raw = t ^ (t >> 14);
        (raw as f64) / 4294967296.0
    }
}

fn generate_daily_seed(date_string: &str, pubkey: &str) -> u32 {
    let input = if pubkey.is_empty() {
        date_string.to_string()
    } else {
        format!("{}:{}", date_string, pubkey)
    };
    let mut hash: i32 = 0;
    for b in input.bytes() {
        hash = (hash << 5).wrapping_sub(hash).wrapping_add(b as i32);
    }
    hash.unsigned_abs()
}

pub fn missions_for_stages(stages: &[BlobbiStage]) -> Vec<&'static MissionDefinition> {
    MISSION_POOL
        .iter()
        .filter(|m| stages.iter().any(|s| m.required_stages.contains(s)))
        .collect()
}

pub fn get_mission_by_id(id: &str) -> Option<&'static MissionDefinition> {
    MISSION_POOL.iter().find(|m| m.id == id)
}

pub fn select_daily_missions(
    count: usize,
    date_seed: &str,
    pubkey: &str,
    stages: &[BlobbiStage],
) -> Vec<&'static MissionDefinition> {
    let pool = missions_for_stages(stages);
    if pool.is_empty() {
        return vec![];
    }

    let seed = generate_daily_seed(date_seed, pubkey);
    let mut rng = mulberry32(seed);

    let mut available: Vec<&'static MissionDefinition> = pool;
    let mut selected: Vec<&'static MissionDefinition> = vec![];

    while selected.len() < count && !available.is_empty() {
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

        selected.push(available[chosen_idx]);
        available.remove(chosen_idx);
    }

    selected
}
