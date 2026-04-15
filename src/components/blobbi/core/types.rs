use nostr_sdk::Event;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobbiStage {
    #[default]
    Egg,
    Baby,
    Adult,
}

impl BlobbiStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlobbiStage::Egg => "egg",
            BlobbiStage::Baby => "baby",
            BlobbiStage::Adult => "adult",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "baby" => BlobbiStage::Baby,
            "adult" => BlobbiStage::Adult,
            _ => BlobbiStage::Egg,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            BlobbiStage::Egg => "Egg",
            BlobbiStage::Baby => "Baby",
            BlobbiStage::Adult => "Adult",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobbiState {
    #[default]
    Active,
    Sleeping,
    Hibernating,
}

impl BlobbiState {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlobbiState::Active => "active",
            BlobbiState::Sleeping => "sleeping",
            BlobbiState::Hibernating => "hibernating",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "sleeping" => BlobbiState::Sleeping,
            "hibernating" => BlobbiState::Hibernating,
            _ => BlobbiState::Active,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Copy)]
pub struct BlobbiStats {
    pub hunger: f64,
    pub happiness: f64,
    pub health: f64,
    pub hygiene: f64,
    pub energy: f64,
}

impl BlobbiStats {
    pub fn full() -> Self {
        BlobbiStats {
            hunger: 100.0,
            happiness: 100.0,
            health: 100.0,
            hygiene: 100.0,
            energy: 100.0,
        }
    }

    pub fn average(&self) -> f64 {
        (self.hunger + self.happiness + self.health + self.hygiene + self.energy) / 5.0
    }

    pub fn lowest(&self) -> (&'static str, f64) {
        let mut lowest = ("hunger", self.hunger);
        if self.happiness < lowest.1 {
            lowest = ("happiness", self.happiness);
        }
        if self.health < lowest.1 {
            lowest = ("health", self.health);
        }
        if self.hygiene < lowest.1 {
            lowest = ("hygiene", self.hygiene);
        }
        if self.energy < lowest.1 {
            lowest = ("energy", self.energy);
        }
        lowest
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlobbiVisualTraits {
    pub base_color: String,
    pub secondary_color: Option<String>,
    pub eye_color: String,
    pub pattern: String,
    pub special_mark: String,
    pub size: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlobbiPersonality {
    pub traits: Vec<String>,
    pub mood: String,
    pub favorite_food: Option<String>,
    pub voice_type: Option<String>,
    pub title: Option<String>,
    pub skills: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlobbiTaskProgress {
    pub id: String,
    pub completed: bool,
    pub progress: u32,
    pub target: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlobbiCompanion {
    pub event_id: Option<String>,
    pub d: String,
    pub name: String,
    pub stage: BlobbiStage,
    pub state: BlobbiState,
    pub stats: BlobbiStats,
    pub visual_traits: BlobbiVisualTraits,
    pub personality: BlobbiPersonality,
    pub generation: u32,
    pub breeding_ready: bool,
    pub experience: u64,
    pub care_streak: u32,
    pub is_sleeping: bool,
    pub sleep_started_at: Option<u64>,
    pub last_sleep_update: Option<u64>,
    pub source: Option<String>,
    pub last_interaction: Option<u64>,
    pub last_decay_at: Option<u64>,
    pub last_meal: Option<u64>,
    pub last_clean: Option<u64>,
    pub last_warm: Option<u64>,
    pub last_talk: Option<u64>,
    pub last_sing: Option<u64>,
    pub last_medicine: Option<u64>,
    pub last_check: Option<u64>,
    pub seed: Option<String>,
    pub adult_type: Option<String>,
    pub evolution_time: Option<u64>,
    pub is_dirty: bool,
    pub has_buff: Option<String>,
    pub has_debuff: Option<String>,
    pub incubation_time: Option<u64>,
    pub incubation_progress: Option<f64>,
    pub egg_temperature: Option<f64>,
    pub egg_status: Option<String>,
    pub shell_integrity: Option<f64>,
    pub start_incubation: Option<u64>,
    pub start_evolution: Option<u64>,
    pub theme: Option<String>,
    pub crossover_app: Option<String>,
    pub manifestation: Option<String>,
    pub blessing: Option<String>,
    pub visual_effect: Option<String>,
    pub adopted_by: Option<String>,
    pub adopted_from: Option<String>,
    pub visible_to_others: Option<bool>,
    pub tasks: Vec<BlobbiTaskProgress>,
    pub content: String,
    pub raw_event: Option<Event>,
}

impl BlobbiCompanion {
    pub fn is_egg(&self) -> bool {
        self.stage == BlobbiStage::Egg
    }

    pub fn is_baby(&self) -> bool {
        self.stage == BlobbiStage::Baby
    }

    #[allow(dead_code)]
    pub fn is_adult(&self) -> bool {
        self.stage == BlobbiStage::Adult
    }

    pub fn is_sleeping(&self) -> bool {
        self.is_sleeping || self.state == BlobbiState::Sleeping
    }

    pub fn is_divine(&self) -> bool {
        self.theme.as_deref() == Some("divine")
    }

    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            "Unnamed Blobbi"
        } else {
            &self.name
        }
    }

    pub fn stat_value(&self, stat: &str) -> f64 {
        match stat {
            "hunger" => self.stats.hunger,
            "happiness" => self.stats.happiness,
            "health" => self.stats.health,
            "hygiene" => self.stats.hygiene,
            "energy" => self.stats.energy,
            "egg_temperature" => self.egg_temperature.unwrap_or(0.0),
            "shell_integrity" => self.shell_integrity.unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub fn set_stat_value(&mut self, stat: &str, value: f64) {
        match stat {
            "hunger" => self.stats.hunger = value,
            "happiness" => self.stats.happiness = value,
            "health" => self.stats.health = value,
            "hygiene" => self.stats.hygiene = value,
            "energy" => self.stats.energy = value,
            "egg_temperature" => self.egg_temperature = Some(value),
            "shell_integrity" => self.shell_integrity = Some(value),
            _ => {}
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StorageItem {
    pub item_id: String,
    pub quantity: u32,
}

impl StorageItem {
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 2 {
            Some(StorageItem {
                item_id: parts[0].to_string(),
                quantity: parts[1].parse().ok()?,
            })
        } else {
            None
        }
    }

    pub fn to_string_value(&self) -> String {
        format!("{}:{}", self.item_id, self.quantity)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlobbonautProfile {
    pub d: String,
    pub name: String,
    pub coins: u64,
    pub petting_level: u32,
    pub level: u32,
    pub current_companion: Option<String>,
    pub onboarding_done: bool,
    pub has: Vec<String>,
    pub storage: Vec<StorageItem>,
    pub achievements: Vec<String>,
    pub lifetime_blobbis: u32,
    pub starter_blobbi: Option<String>,
    pub favorite_blobbi: Option<String>,
    pub style: Option<String>,
    pub background: Option<String>,
    pub title: Option<String>,
    pub raw_event: Option<Event>,
}
