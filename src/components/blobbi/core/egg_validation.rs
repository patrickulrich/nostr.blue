use crate::utils::nips::nip_bb::constants::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl std::fmt::Display for Rarity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rarity::Common => write!(f, "Common"),
            Rarity::Uncommon => write!(f, "Uncommon"),
            Rarity::Rare => write!(f, "Rare"),
            Rarity::Legendary => write!(f, "Legendary"),
        }
    }
}

pub struct ColorRarity {
    pub color: &'static str,
    pub rarity: Rarity,
}

fn color_rarity_table() -> Vec<ColorRarity> {
    let mut table = Vec::new();
    for (i, &color) in DEFAULT_BASE_COLORS.iter().enumerate() {
        let rarity = if i < 4 {
            Rarity::Common
        } else if i < 8 {
            Rarity::Uncommon
        } else if i < 10 {
            Rarity::Rare
        } else {
            Rarity::Legendary
        };
        table.push(ColorRarity { color, rarity });
    }
    table
}

pub fn get_color_rarity(color: &str) -> Rarity {
    color_rarity_table()
        .iter()
        .find(|entry| entry.color.eq_ignore_ascii_case(color))
        .map(|entry| entry.rarity)
        .unwrap_or(Rarity::Common)
}

pub fn get_pattern_rarity(pattern: &str) -> Rarity {
    match pattern.to_lowercase().as_str() {
        "solid" | "gradient" => Rarity::Common,
        "speckled" => Rarity::Uncommon,
        "striped" => Rarity::Rare,
        _ => Rarity::Common,
    }
}

pub fn get_size_rarity(size: &str) -> Rarity {
    match size.to_lowercase().as_str() {
        "medium" => Rarity::Common,
        "small" | "large" => Rarity::Uncommon,
        "tiny" => Rarity::Rare,
        _ => Rarity::Common,
    }
}

pub fn get_special_mark_rarity(mark: &str) -> Rarity {
    match mark.to_lowercase().as_str() {
        "dot_center" | "oval_spots" => Rarity::Common,
        "ring_mark" => Rarity::Uncommon,
        "rune_top" => Rarity::Rare,
        "sigil_eye" => Rarity::Legendary,
        _ => Rarity::Common,
    }
}

pub struct ValidationResult {
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn is_ok(&self) -> bool {
        self.warnings.is_empty()
    }
}

pub fn validate_egg_properties(
    base_color: &str,
    pattern: &str,
    size: &str,
    special_mark: &str,
) -> ValidationResult {
    let mut warnings = Vec::new();

    if !DEFAULT_BASE_COLORS.iter().any(|c| c.eq_ignore_ascii_case(base_color)) {
        warnings.push(format!("Unknown base color: {base_color}"));
    }

    if !DEFAULT_PATTERNS
        .iter()
        .any(|p| p.eq_ignore_ascii_case(pattern))
    {
        warnings.push(format!("Unknown pattern: {pattern}"));
    }

    if !DEFAULT_SIZES.iter().any(|s| s.eq_ignore_ascii_case(size)) {
        warnings.push(format!("Unknown size: {size}"));
    }

    if !DEFAULT_SPECIAL_MARKS
        .iter()
        .any(|m| m.eq_ignore_ascii_case(special_mark))
    {
        warnings.push(format!("Unknown special mark: {special_mark}"));
    }

    let color_rarity = get_color_rarity(base_color);
    let pattern_rarity = get_pattern_rarity(pattern);
    let size_rarity = get_size_rarity(size);
    let mark_rarity = get_special_mark_rarity(special_mark);

    if matches!(color_rarity, Rarity::Legendary) {
        warnings.push(format!(
            "Base color {base_color} is {} rarity",
            color_rarity
        ));
    }
    if matches!(mark_rarity, Rarity::Legendary) {
        warnings.push(format!(
            "Special mark {special_mark} is {} rarity",
            mark_rarity
        ));
    }

    let legendary_count = [&color_rarity, &pattern_rarity, &size_rarity, &mark_rarity]
        .iter()
        .filter(|r| matches!(***r, Rarity::Legendary))
        .count();

    if legendary_count > 1 {
        warnings.push(format!(
            "Egg has {legendary_count} legendary properties — extremely rare combination"
        ));
    }

    ValidationResult { warnings }
}
