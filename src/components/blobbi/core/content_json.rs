use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlobbonautProfileContent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_missions: Option<PersistedDailyMissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progression: Option<ProgressionContent>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PersistedDailyMissions {
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub missions: Vec<PersistedDailyMission>,
    #[serde(default)]
    pub bonus_claimed: bool,
    #[serde(default = "default_rerolls")]
    pub rerolls_remaining: u32,
    #[serde(default)]
    pub total_xp_earned: u64,
    #[serde(default)]
    pub last_updated_at: u64,
}

fn default_rerolls() -> u32 {
    3
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PersistedDailyMission {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub required_count: u32,
    #[serde(default)]
    pub reward: u32,
    #[serde(default)]
    pub reward_coins: u32,
    #[serde(default)]
    pub weight: u32,
    #[serde(default)]
    pub required_stages: Vec<String>,
    #[serde(default)]
    pub current_count: u32,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub claimed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProgressionContent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global: Option<GlobalProgression>,
    #[serde(default)]
    pub games: std::collections::HashMap<String, GameProgression>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GlobalProgression {
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub xp: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameProgression {
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub xp: u64,
    #[serde(default)]
    pub unlocks: GameUnlocks,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameUnlocks {
    #[serde(default = "default_max_blobbis")]
    pub max_blobbis: u32,
    #[serde(default)]
    pub real_inventory_enabled: bool,
}

fn default_max_blobbis() -> u32 {
    1
}

impl PersistedDailyMission {
    pub fn is_valid(&self) -> bool {
        !self.id.is_empty() && self.required_count > 0 && self.reward > 0
    }
}

impl PersistedDailyMissions {
    pub fn valid_missions(&self) -> Vec<&PersistedDailyMission> {
        self.missions.iter().filter(|m| m.is_valid()).collect()
    }

    pub fn is_valid(&self) -> bool {
        !self.date.is_empty() && !self.missions.is_empty()
    }
}

pub struct ContentParseResult {
    pub content: BlobbonautProfileContent,
    pub parse_ok: bool,
    pub was_empty: bool,
}

pub fn safe_parse_content_with_status(content: &str) -> ContentParseResult {
    if content.trim().is_empty() {
        return ContentParseResult {
            content: BlobbonautProfileContent::default(),
            parse_ok: true,
            was_empty: true,
        };
    }

    let raw: serde_json::Map<String, serde_json::Value> = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("Blobbi: Failed to parse profile content JSON: {}", e);
            return ContentParseResult {
                content: BlobbonautProfileContent::default(),
                parse_ok: false,
                was_empty: false,
            };
        }
    };

    let known_keys = [
        "dailyMissions",
        "progression",
    ];

    let mut result = BlobbonautProfileContent::default();

    let daily_val = raw.get("dailyMissions");
    if let Some(v) = daily_val {
        match serde_json::from_value(v.clone()) {
            Ok(dm) => result.daily_missions = Some(dm),
            Err(e) => log::warn!("Blobbi: Failed to parse dailyMissions section: {}", e),
        }
    }

    let prog_val = raw.get("progression");
    if let Some(v) = prog_val {
        match serde_json::from_value(v.clone()) {
            Ok(p) => result.progression = Some(p),
            Err(e) => log::warn!("Blobbi: Failed to parse progression section: {}", e),
        }
    }

    for (key, value) in &raw {
        if !known_keys.contains(&key.as_str()) {
            result.extra.insert(key.clone(), value.clone());
        }
    }

    ContentParseResult {
        content: result,
        parse_ok: true,
        was_empty: false,
    }
}

pub fn state_fingerprint(content: &BlobbonautProfileContent) -> String {
    let mut s = String::new();
    if let Some(ref dm) = content.daily_missions {
        s.push_str(&dm.date);
        for m in &dm.missions {
            s.push_str(&format!("{}:{}:{}:{}", m.id, m.current_count, m.completed, m.claimed));
        }
        s.push_str(&format!("b:{}:r:{}", dm.bonus_claimed, dm.rerolls_remaining));
    }
    if let Some(ref prog) = content.progression {
        let mut games: Vec<_> = prog.games.iter().collect();
        games.sort_by_key(|(k, _)| *k);
        for (id, g) in games {
            s.push_str(&format!("g:{}:{}:{}", id, g.level, g.xp));
        }
    }
    s
}

pub fn merge_progression(
    existing: &ProgressionContent,
    update: &ProgressionContent,
) -> ProgressionContent {
    let mut merged = existing.clone();

    if let Some(ref update_global) = update.global {
        merged.global = Some(update_global.clone());
    }

    for (key, update_game) in &update.games {
        if let Some(existing_game) = merged.games.get_mut(key) {
            existing_game.level = update_game.level;
            existing_game.xp = update_game.xp;
            if update_game.unlocks.max_blobbis > 0 {
                existing_game.unlocks.max_blobbis = update_game.unlocks.max_blobbis;
            }
            if update_game.unlocks.real_inventory_enabled {
                existing_game.unlocks.real_inventory_enabled = true;
            }
        } else {
            merged.games.insert(key.clone(), update_game.clone());
        }
    }

    merged
}

pub fn derive_global_level(progression: &ProgressionContent) -> u32 {
    progression.games.values().map(|g| g.level).sum()
}

pub fn update_progression_with_global_level(
    raw_content: &str,
    progression: &ProgressionContent,
) -> String {
    let global_level = derive_global_level(progression);
    let mut prog = progression.clone();
    prog.global = Some(GlobalProgression {
        level: global_level,
        xp: prog.global.as_ref().map(|g| g.xp).unwrap_or(0),
    });
    update_progression_content(raw_content, &prog)
}

pub fn safe_parse_content(content: &str) -> BlobbonautProfileContent {
    if content.trim().is_empty() {
        return BlobbonautProfileContent::default();
    }

    let raw: serde_json::Map<String, serde_json::Value> =
        match serde_json::from_str(content) {
            Ok(v) => v,
            Err(_) => return BlobbonautProfileContent::default(),
        };

    let known_keys = [
        "dailyMissions",
        "progression",
    ];

    let mut result = BlobbonautProfileContent::default();

    let daily_val = raw.get("dailyMissions");
    if let Some(v) = daily_val {
        result.daily_missions = serde_json::from_value(v.clone()).ok();
    }

    let prog_val = raw.get("progression");
    if let Some(v) = prog_val {
        result.progression = serde_json::from_value(v.clone()).ok();
    }

    for (key, value) in &raw {
        if !known_keys.contains(&key.as_str()) {
            result.extra.insert(key.clone(), value.clone());
        }
    }

    result
}

pub fn serialize_content(content: &BlobbonautProfileContent) -> String {
    let mut map = serde_json::Map::new();

    if let Some(ref dm) = content.daily_missions {
        map.insert(
            "dailyMissions".to_string(),
            serde_json::to_value(dm).unwrap_or(serde_json::Value::Null),
        );
    }

    if let Some(ref prog) = content.progression {
        map.insert(
            "progression".to_string(),
            serde_json::to_value(prog).unwrap_or(serde_json::Value::Null),
        );
    }

    for (key, value) in &content.extra {
        map.insert(key.clone(), value.clone());
    }

    if map.is_empty() {
        return String::new();
    }

    serde_json::Value::Object(map).to_string()
}

pub fn update_content_section(
    raw_content: &str,
    section: &str,
    value: &serde_json::Value,
) -> String {
    let mut map: serde_json::Map<String, serde_json::Value> = if raw_content.trim().is_empty() {
        serde_json::Map::new()
    } else {
        serde_json::from_str::<serde_json::Value>(raw_content)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    };

    map.insert(section.to_string(), value.clone());

    serde_json::Value::Object(map).to_string()
}

pub fn update_daily_missions_content(
    raw_content: &str,
    missions: &PersistedDailyMissions,
) -> String {
    let value = serde_json::to_value(missions).unwrap_or(serde_json::Value::Null);
    update_content_section(raw_content, "dailyMissions", &value)
}

pub fn update_progression_content(
    raw_content: &str,
    progression: &ProgressionContent,
) -> String {
    let value = serde_json::to_value(progression).unwrap_or(serde_json::Value::Null);
    update_content_section(raw_content, "progression", &value)
}

pub fn normalize_profile_fields(profile: &mut serde_json::Value) {
    if let Some(obj) = profile.as_object_mut() {
        obj.entry("version".to_string())
            .or_insert_with(|| serde_json::Value::Number(serde_json::Number::from(1)));
        
        if !obj.contains_key("name") {
            obj.insert("name".to_string(), serde_json::Value::String(String::new()));
        }
        
        if !obj.contains_key("mood") {
            obj.insert("mood".to_string(), serde_json::Value::String("neutral".to_string()));
        }
        
        if let Some(stats) = obj.get_mut("stats") {
            if let Some(stats_obj) = stats.as_object_mut() {
                for key in &["hunger", "happiness", "health", "hygiene", "energy"] {
                    stats_obj.entry(key.to_string()).or_insert_with(|| {
                        serde_json::Value::Number(serde_json::Number::from_f64(70.0).unwrap_or(serde_json::Number::from(70)))
                    });
                }
            }
        }
    }
}
