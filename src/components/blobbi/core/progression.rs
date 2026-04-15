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

    pub fn level_progress_pct(&self) -> f64 {
        if self.xp_needed == 0 {
            100.0
        } else {
            (self.xp_in_level as f64 / self.xp_needed as f64) * 100.0
        }
    }
}
