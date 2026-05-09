use crate::components::blobbi::core::content_json::{GameProgression, ProgressionContent};
use crate::components::blobbi::core::streak;
use crate::components::blobbi::core::xp;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProgressionState {
    pub level: u32,
    pub xp: u64,
    pub xp_in_level: u64,
    pub xp_needed: u64,
    pub streak: u32,
    pub streak_bonus_pct: f64,
}

impl ProgressionState {
    pub fn compute(xp_total: u64, care_streak: u32) -> Self {
        let level = xp::level_from_xp(xp_total);
        let (xp_in_level, xp_needed) = xp::xp_progress_in_current_level(xp_total, level);
        let streak_bonus_pct = streak::streak_bonus(care_streak);
        Self {
            level,
            xp: xp_total,
            xp_in_level,
            xp_needed,
            streak: care_streak,
            streak_bonus_pct,
        }
    }

    pub fn from_progression_content(content: &ProgressionContent) -> Self {
        let global_level = derive_global_level(content);
        let blobbi_game = content.games.get("blobbi");
        let xp = blobbi_game.map(|g| g.xp).unwrap_or(0);
        let streak = 0;
        let level = xp::level_from_xp(xp);
        let (xp_in_level, xp_needed) = xp::xp_progress_in_current_level(xp, level);
        Self {
            level: global_level,
            xp,
            xp_in_level,
            xp_needed,
            streak,
            streak_bonus_pct: 0.0,
        }
    }

    pub fn level_progress_pct(&self) -> f64 {
        if self.xp_needed == 0 {
            100.0
        } else {
            (self.xp_in_level as f64 / self.xp_needed as f64) * 100.0
        }
    }
}

pub fn derive_global_level(content: &ProgressionContent) -> u32 {
    content
        .games
        .values()
        .map(|g| g.level)
        .sum()
}

pub fn create_default_progression() -> ProgressionContent {
    let mut games = std::collections::HashMap::new();
    games.insert(
        "blobbi".to_string(),
        GameProgression {
            level: 1,
            xp: 0,
            unlocks: Default::default(),
        },
    );
    ProgressionContent {
        global: Some(crate::components::blobbi::core::content_json::GlobalProgression {
            level: 1,
            xp: 0,
        }),
        games,
    }
}

pub fn merge_progression(
    base: &ProgressionContent,
    partial: &serde_json::Value,
) -> ProgressionContent {
    let mut result = base.clone();

    for (key, game) in &base.games {
        result.games.entry(key.clone()).or_insert_with(|| game.clone());
    }

    if let Some(obj) = partial.as_object() {
        if let Some(games_val) = obj.get("games") {
            if let Some(games_obj) = games_val.as_object() {
                for (key, val) in games_obj {
                    if let Ok(gp) = serde_json::from_value::<GameProgression>(val.clone()) {
                        if let Some(existing) = result.games.get_mut(key) {
                            if gp.level > existing.level {
                                existing.level = gp.level;
                            }
                            existing.xp = existing.xp.max(gp.xp);
                            if gp.unlocks.max_blobbis > existing.unlocks.max_blobbis {
                                existing.unlocks.max_blobbis = gp.unlocks.max_blobbis;
                            }
                            if gp.unlocks.real_inventory_enabled {
                                existing.unlocks.real_inventory_enabled = true;
                            }
                        } else {
                            result.games.insert(key.clone(), gp);
                        }
                    }
                }
            }
        }
        if let Some(global_val) = obj.get("global") {
            if let Ok(g) = serde_json::from_value(global_val.clone()) {
                result.global = Some(g);
            }
        }
    }

    let global_level = derive_global_level(&result);
    result.global = Some(crate::components::blobbi::core::content_json::GlobalProgression {
        level: global_level,
        xp: result.global.as_ref().map(|g| g.xp).unwrap_or(0),
    });

    result
}

pub fn upsert_level_tag(level: u32, tags: &mut Vec<Vec<String>>) {
    let level_str = level.to_string();
    if let Some(tag) = tags.iter_mut().find(|t| !t.is_empty() && t[0] == "level") {
        if tag.len() >= 2 {
            tag[1] = level_str;
        } else {
            tag.push(level_str);
        }
    } else {
        tags.push(vec!["level".to_string(), level_str]);
    }
}
