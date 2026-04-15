use crate::components::blobbi::actions::action_types::BlobbiActionType;

#[allow(dead_code)]
pub fn xp_for_action(action: BlobbiActionType) -> u64 {
    action.xp_value()
}

#[allow(dead_code)]
pub fn level_from_xp(xp: u64) -> u32 {
    let mut level = 1u32;
    let mut accumulated = 0u64;
    loop {
        let needed = xp_for_level(level + 1) - accumulated;
        if xp < accumulated + needed {
            break;
        }
        accumulated += needed;
        level += 1;
        if level > 999 {
            break;
        }
    }
    level
}

#[allow(dead_code)]
pub fn xp_for_level(level: u32) -> u64 {
    if level <= 1 {
        return 0;
    }
    let level = level as u64;
    50 * level * (level - 1) / 2
}

#[allow(dead_code)]
pub fn xp_progress_in_current_level(xp: u64, level: u32) -> (u64, u64) {
    let current_level_xp = xp_for_level(level);
    let next_level_xp = xp_for_level(level + 1);
    let progress = xp.saturating_sub(current_level_xp);
    let needed = next_level_xp.saturating_sub(current_level_xp);
    (progress, needed)
}

#[allow(dead_code)]
pub fn coins_for_level_up(level: u32) -> u64 {
    10 + (level as u64) * 5
}
