use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::shop::shop_items::ItemCategory;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum NeedPriority {
    #[default]
    None,
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NeedCheckResult {
    pub needs_item: bool,
    pub priority: NeedPriority,
    pub triggering_stat: Option<&'static str>,
    pub current_value: Option<f64>,
    pub threshold: Option<f64>,
}

pub const THRESHOLD_HUNGER: f64 = 40.0;
pub const THRESHOLD_HAPPINESS: f64 = 35.0;
pub const THRESHOLD_HYGIENE: f64 = 30.0;
pub const THRESHOLD_HEALTH: f64 = 50.0;
pub const THRESHOLD_ENERGY: f64 = 25.0;

pub fn calculate_priority(value: f64, threshold: f64) -> NeedPriority {
    if value >= threshold {
        return NeedPriority::None;
    }
    let deficit_pct = (threshold - value) / threshold;
    if deficit_pct >= 0.6 {
        NeedPriority::Critical
    } else if deficit_pct >= 0.4 {
        NeedPriority::High
    } else if deficit_pct >= 0.2 {
        NeedPriority::Normal
    } else {
        NeedPriority::Low
    }
}

pub fn stat_threshold(stat: &str) -> f64 {
    match stat {
        "hunger" => THRESHOLD_HUNGER,
        "happiness" => THRESHOLD_HAPPINESS,
        "hygiene" => THRESHOLD_HYGIENE,
        "health" => THRESHOLD_HEALTH,
        "energy" => THRESHOLD_ENERGY,
        _ => 50.0,
    }
}

pub fn check_stat_need(blobbi: &BlobbiCompanion, stat: &str) -> (bool, NeedPriority, f64, f64) {
    let value = blobbi.stat_value(stat);
    let threshold = stat_threshold(stat);
    let needed = value < threshold;
    let priority = calculate_priority(value, threshold);
    (needed, priority, value, threshold)
}

pub fn category_stats(category: ItemCategory) -> &'static [&'static str] {
    match category {
        ItemCategory::Food => &["hunger", "energy"],
        ItemCategory::Toy => &["happiness"],
        ItemCategory::Hygiene => &["hygiene"],
        ItemCategory::Medicine => &["health"],
        ItemCategory::Accessory => &[],
    }
}

pub fn check_item_category_need(blobbi: &BlobbiCompanion, category: ItemCategory) -> NeedCheckResult {
    let stats = category_stats(category);
    if stats.is_empty() {
        return NeedCheckResult::default();
    }

    let mut best = NeedCheckResult::default();
    for &stat in stats {
        let (needed, priority, value, threshold) = check_stat_need(blobbi, stat);
        if needed && priority > best.priority {
            best = NeedCheckResult {
                needs_item: true,
                priority,
                triggering_stat: Some(stat),
                current_value: Some(value),
                threshold: Some(threshold),
            };
        }
    }
    best
}

pub fn get_all_needs(blobbi: &BlobbiCompanion) -> Vec<(&'static str, NeedPriority, f64, f64)> {
    let mut needs = Vec::new();
    for stat in &["hunger", "happiness", "hygiene", "health", "energy"] {
        let (needed, priority, value, threshold) = check_stat_need(blobbi, stat);
        if needed {
            needs.push((*stat, priority, value, threshold));
        }
    }
    needs.sort_by_key(|b| std::cmp::Reverse(b.1));
    needs
}

#[allow(dead_code)]
pub fn has_critical_need(blobbi: &BlobbiCompanion) -> bool {
    get_all_needs(blobbi).iter().any(|n| n.1 == NeedPriority::Critical)
}

pub fn has_any_need(blobbi: &BlobbiCompanion) -> bool {
    !get_all_needs(blobbi).is_empty()
}

pub fn needed_categories(blobbi: &BlobbiCompanion) -> Vec<ItemCategory> {
    let mut cats = Vec::new();
    for &cat in &[ItemCategory::Food, ItemCategory::Toy, ItemCategory::Hygiene, ItemCategory::Medicine] {
        if check_item_category_need(blobbi, cat).needs_item {
            cats.push(cat);
        }
    }
    cats
}
