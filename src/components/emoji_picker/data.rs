use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct NativeEmojiEntry {
    pub glyph: String,
    pub name: String,
    pub group: &'static str,
    /// GitHub (gemoji) shortcodes, e.g. `["laughing", "satisfied"]` for 😆.
    pub shortcodes: Vec<String>,
}

fn group_label(group: emojis::Group) -> &'static str {
    match group {
        emojis::Group::SmileysAndEmotion => "Smileys & Emotion",
        emojis::Group::PeopleAndBody => "People & Body",
        emojis::Group::AnimalsAndNature => "Animals & Nature",
        emojis::Group::FoodAndDrink => "Food & Drink",
        emojis::Group::TravelAndPlaces => "Travel & Places",
        emojis::Group::Activities => "Activities",
        emojis::Group::Objects => "Objects",
        emojis::Group::Symbols => "Symbols",
        emojis::Group::Flags => "Flags",
    }
}

fn entry_for(emoji: &'static emojis::Emoji) -> NativeEmojiEntry {
    NativeEmojiEntry {
        glyph: emoji.as_str().to_string(),
        name: emoji.name().to_string(),
        group: group_label(emoji.group()),
        shortcodes: emoji.shortcodes().map(str::to_string).collect(),
    }
}

/// Base emoji only (default skin tone) in Unicode CLDR order, per
/// `emojis::iter()` — 1,914 entries at Unicode 17.0. Skin-tone variants are
/// reachable per-emoji via `emojis::get(glyph)?.skin_tones()` (see the picker's
/// tone submenu), not as grid rows.
pub static ALL_NATIVE_EMOJIS: LazyLock<Vec<NativeEmojiEntry>> =
    LazyLock::new(|| emojis::iter().map(entry_for).collect());

/// The nine standard CLDR categories in display order.
pub static NATIVE_EMOJI_CATEGORIES: LazyLock<Vec<(String, Vec<NativeEmojiEntry>)>> =
    LazyLock::new(|| {
        emojis::Group::iter()
            .map(|group| {
                (
                    group_label(group).to_string(),
                    group.emojis().map(entry_for).collect(),
                )
            })
            .collect()
    });

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The crate upgrades Unicode in non-breaking releases, so assert a range
    /// rather than an exact count. Unicode 17.0 ships 1,914 base emoji.
    #[test]
    fn native_emoji_count_reflects_modern_unicode() {
        let count = ALL_NATIVE_EMOJIS.len();
        assert!(
            (1_900..=2_000).contains(&count),
            "expected ~1,914 base emoji (Unicode 17), got {count}"
        );
    }

    #[test]
    fn nine_standard_categories_in_cldr_order() {
        assert_eq!(NATIVE_EMOJI_CATEGORIES.len(), 9);
        assert_eq!(NATIVE_EMOJI_CATEGORIES[0].0, "Smileys & Emotion");
        assert_eq!(NATIVE_EMOJI_CATEGORIES[1].0, "People & Body");
        assert_eq!(NATIVE_EMOJI_CATEGORIES[8].0, "Flags");
    }

    #[test]
    fn categories_partition_all_emojis() {
        let total: usize = NATIVE_EMOJI_CATEGORIES.iter().map(|(_, e)| e.len()).sum();
        assert_eq!(total, ALL_NATIVE_EMOJIS.len());
    }

    /// Everything added after Unicode 13.1 (the old `emoji` 0.2 crate's
    /// ceiling) must be present: low battery (14), shaking face (15), wing
    /// (15), face with bags under eyes (16), distorted face (17).
    #[test]
    fn post_unicode_13_1_emoji_present() {
        for glyph in ["🪫", "🫨", "🪽", "🫩", "🫪"] {
            assert!(
                ALL_NATIVE_EMOJIS.iter().any(|e| e.glyph == glyph),
                "{glyph} missing from native emoji dataset"
            );
        }
    }

    #[test]
    fn grid_contains_no_skin_tone_variants() {
        assert!(
            ALL_NATIVE_EMOJIS
                .iter()
                .all(|e| !e.name.contains("skin tone")),
            "skin-tone variants leaked into the base grid"
        );
    }

    #[test]
    fn glyphs_are_unique() {
        let unique: HashSet<&str> = ALL_NATIVE_EMOJIS.iter().map(|e| e.glyph.as_str()).collect();
        assert_eq!(unique.len(), ALL_NATIVE_EMOJIS.len());
    }

    #[test]
    fn skin_tone_submenu_lookup_round_trips() {
        let thumbs_up = emojis::get("👍").expect("👍 should exist");
        let tones: Vec<&str> = thumbs_up
            .skin_tones()
            .expect("👍 should support skin tones")
            .map(|e| e.as_str())
            .collect();
        assert_eq!(tones, ["👍", "👍🏻", "👍🏼", "👍🏽", "👍🏾", "👍🏿"]);
    }
}
