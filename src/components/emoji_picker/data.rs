use std::collections::HashMap;
use std::sync::LazyLock;

use super::subdivisions::{SubdivisionEntry, SUBDIVISIONS};

#[derive(Clone, Debug, PartialEq)]
pub struct NativeEmojiEntry {
    pub glyph: String,
    pub name: String,
    pub group: &'static str,
    /// GitHub (gemoji) shortcodes, e.g. `["laughing", "satisfied"]` for 😆.
    /// Subdivision flags carry `["us-ky", "usky", "ky"]`-style aliases.
    pub shortcodes: Vec<String>,
    /// Whether this emoji has skin-tone variants (reachable via
    /// `emojis::get(glyph)?.skin_tones()` — 6 for single-person emoji, up to
    /// 26 for couples like 👩‍❤️‍👨).
    pub has_tones: bool,
    /// ISO 3166-2 subdivision flag (emoji tag sequence like 🏴󠁵󠁳󠁫󠁹󠁿) rather
    /// than a crate-provided RGI emoji. Drives search tie-breaks (countries
    /// rank above subdivisions) and the fallback text label.
    pub is_subdivision: bool,
    /// Containment depth (1 = direct child of the country, 2 = child of a
    /// subdivision). Used as a search tie-break so states rank above their
    /// counties (e.g. "york" → New York before North Yorkshire).
    pub depth: u8,
    /// Human-readable ISO code ("US-KY") rendered under the glyph on
    /// platforms whose emoji font cannot display subdivision tag sequences.
    pub label: Option<String>,
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
    let mut shortcodes: Vec<String> = emoji.shortcodes().map(str::to_string).collect();
    // The crate's shortcode set is inconsistent about ISO-2 aliases (🇺🇸 has
    // "us" but 🇨🇦 only "canada"), so derive the two-letter code from the
    // regional indicators — queries like "ca" must exact-match 🇨🇦.
    if emoji.group() == emojis::Group::Flags {
        if let Some(country) = country_code_from_flag_glyph(emoji.as_str()) {
            if !shortcodes.iter().any(|code| code == &country) {
                shortcodes.push(country);
            }
        }
    }
    NativeEmojiEntry {
        glyph: emoji.as_str().to_string(),
        name: emoji.name().to_string(),
        group: group_label(emoji.group()),
        shortcodes,
        has_tones: emoji.skin_tone().is_some(),
        is_subdivision: false,
        depth: 0,
        label: None,
    }
}

/// Build an emoji tag-sequence flag glyph for an ISO 3166-2 subdivision code
/// (e.g. `"usky"` → 🏴󠁵󠁳󠁫󠁹󠁿): U+1F3F4 black flag, one tag character per ASCII
/// codepoint (U+E0000 + byte), terminated by U+E007F cancel tag.
pub fn subdivision_flag_glyph(code: &str) -> String {
    const CANCEL_TAG: char = '\u{E007F}';
    let mut glyph = String::with_capacity(4 * (code.len() + 2));
    glyph.push('\u{1F3F4}');
    for byte in code.bytes() {
        debug_assert!(
            byte.is_ascii_lowercase() || byte.is_ascii_digit(),
            "subdivision codes are lowercase ASCII"
        );
        glyph.push(char::from_u32(0xE0000 + u32::from(byte)).expect("valid tag character"));
    }
    glyph.push(CANCEL_TAG);
    glyph
}

/// Decode a two-regional-indicator country-flag glyph (🇺🇸) into its lowercase
/// ISO 3166-1 alpha-2 code ("us"). Returns `None` for any other glyph shape.
fn country_code_from_flag_glyph(glyph: &str) -> Option<String> {
    let mut chars = glyph.chars();
    let mut code = String::new();
    for _ in 0..2 {
        let indicator = chars.next()?;
        let value = indicator as u32;
        if !(0x1F1E6..=0x1F1FF).contains(&value) {
            return None;
        }
        code.push(char::from(b'a' + u8::try_from(value - 0x1F1E6).expect("regional indicator")));
    }
    if chars.next().is_some() {
        return None;
    }
    Some(code)
}

fn entry_for_subdivision(sub: &SubdivisionEntry) -> NativeEmojiEntry {
    let country = &sub.code[..2];
    let segment = &sub.code[2..];
    NativeEmojiEntry {
        glyph: subdivision_flag_glyph(sub.code),
        name: format!("flag: {}", sub.name),
        group: "Flags",
        shortcodes: vec![
            format!("{country}-{segment}"),
            sub.code.to_string(),
            segment.to_string(),
        ],
        has_tones: false,
        is_subdivision: true,
        depth: sub.depth,
        label: Some(format!("{}-{}", country.to_uppercase(), segment.to_uppercase())),
    }
}

/// Group subdivision entries by their parent country, preserving the
/// generator's within-country order (territory tails last, shallower
/// containment depth first, then alphabetical by name).
fn subdivisions_by_country() -> HashMap<String, Vec<NativeEmojiEntry>> {
    let mut map: HashMap<String, Vec<NativeEmojiEntry>> = HashMap::new();
    for sub in SUBDIVISIONS.iter() {
        map.entry(sub.code[..2].to_string())
            .or_default()
            .push(entry_for_subdivision(sub));
    }
    map
}

/// Interleave each country's subdivisions directly after its flag in the
/// Flags category: a flat, top-down distribution (US flag → US states → DC).
fn flags_with_subdivisions(crate_flags: Vec<NativeEmojiEntry>) -> Vec<NativeEmojiEntry> {
    let by_country = subdivisions_by_country();
    let mut out = Vec::with_capacity(crate_flags.len() + SUBDIVISIONS.len());
    for entry in crate_flags {
        let country = country_code_from_flag_glyph(&entry.glyph);
        out.push(entry);
        if let Some(country) = country {
            if let Some(subs) = by_country.get(&country) {
                out.extend(subs.iter().cloned());
            }
        }
    }
    out
}

/// All base emoji (default skin tone only) in Unicode CLDR order per
/// `emojis::iter()` — 1,914 crate entries at Unicode 17.0 — plus 5,024 ISO
/// 3166-2 subdivision flags interleaved into the Flags group. Skin-tone
/// variants are reachable per-emoji via `emojis::get(glyph)?.skin_tones()`
/// (see the picker's tone submenu), not as grid rows.
pub static ALL_NATIVE_EMOJIS: LazyLock<Vec<NativeEmojiEntry>> = LazyLock::new(|| {
    NATIVE_EMOJI_CATEGORIES
        .iter()
        .flat_map(|(_, items)| items.iter().cloned())
        .collect()
});

/// The nine standard CLDR categories in display order, with subdivision
/// flags merged into the Flags category after their parent country flag.
pub static NATIVE_EMOJI_CATEGORIES: LazyLock<Vec<(String, Vec<NativeEmojiEntry>)>> =
    LazyLock::new(|| {
        let mut categories: Vec<(String, Vec<NativeEmojiEntry>)> = emojis::Group::iter()
            .map(|group| {
                (
                    group_label(group).to_string(),
                    group.emojis().map(entry_for).collect(),
                )
            })
            .collect();
        if let Some(flags) = categories.last_mut() {
            debug_assert_eq!(flags.0, "Flags", "Flags must be the last CLDR group");
            let crate_flags = std::mem::take(&mut flags.1);
            flags.1 = flags_with_subdivisions(crate_flags);
        }
        categories
    });

/// Subdivision entries in canonical interleaved order. This defines the index
/// contract for the runtime font-probe verdicts (`Vec<bool>` aligned by
/// position) and the renderability demotion partition.
pub static CANONICAL_SUBDIVISIONS: LazyLock<Vec<NativeEmojiEntry>> = LazyLock::new(|| {
    NATIVE_EMOJI_CATEGORIES[8]
        .1
        .iter()
        .filter(|entry| entry.is_subdivision)
        .cloned()
        .collect()
});

/// Reverse index: subdivision glyph → position in `CANONICAL_SUBDIVISIONS`.
pub static SUBDIVISION_INDEX: LazyLock<HashMap<String, usize>> = LazyLock::new(|| {
    CANONICAL_SUBDIVISIONS
        .iter()
        .enumerate()
        .map(|(idx, entry)| (entry.glyph.clone(), idx))
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
        let base = ALL_NATIVE_EMOJIS
            .iter()
            .filter(|e| !e.is_subdivision)
            .count();
        assert!(
            (1_900..=2_000).contains(&base),
            "expected ~1,914 base emoji (Unicode 17), got {base}"
        );
        let subdivisions = ALL_NATIVE_EMOJIS
            .iter()
            .filter(|e| e.is_subdivision)
            .count();
        assert_eq!(subdivisions, SUBDIVISIONS.len());
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

    /// The tag-sequence builder must produce byte-identical output to the
    /// RGI subdivision sequences the crate ships (England/Scotland/Wales).
    #[test]
    fn subdivision_glyph_builder_matches_rgi_sequences() {
        for (code, name) in [("gbeng", "flag: England"), ("gbsct", "flag: Scotland"), ("gbwls", "flag: Wales")] {
            let glyph = subdivision_flag_glyph(code);
            let emoji = emojis::get(&glyph)
                .unwrap_or_else(|| panic!("{code} glyph should resolve in the emojis crate"));
            assert_eq!(emoji.name(), name);
        }
        assert_eq!(
            subdivision_flag_glyph("usky"),
            "\u{1F3F4}\u{E0075}\u{E0073}\u{E006B}\u{E0079}\u{E007F}"
        );
        assert_eq!(
            subdivision_flag_glyph("jp13"),
            "\u{1F3F4}\u{E006A}\u{E0070}\u{E0031}\u{E0033}\u{E007F}"
        );
    }

    #[test]
    fn country_code_decoding() {
        assert_eq!(country_code_from_flag_glyph("🇺🇸").as_deref(), Some("us"));
        assert_eq!(country_code_from_flag_glyph("🇬🇧").as_deref(), Some("gb"));
        assert_eq!(country_code_from_flag_glyph("🏁"), None);
        assert_eq!(country_code_from_flag_glyph("🏴"), None);
        assert_eq!(country_code_from_flag_glyph("🏴󠁧󠁢󠁥󠁮󠁧󠁿"), None);
    }

    fn country_block(country_glyph: &str) -> Vec<&NativeEmojiEntry> {
        let flags = &NATIVE_EMOJI_CATEGORIES[8].1;
        let start = flags
            .iter()
            .position(|e| e.glyph == country_glyph)
            .unwrap_or_else(|| panic!("{country_glyph} missing from Flags category"))
            + 1;
        flags[start..]
            .iter()
            .take_while(|e| e.is_subdivision)
            .collect()
    }

    /// Top-down distribution: each country flag is immediately followed by
    /// its subdivisions, primary divisions before territory tails.
    #[test]
    fn subdivisions_interleave_after_parent_country_flag() {
        let us = country_block("🇺🇸");
        assert_eq!(us.len(), 51, "50 states + DC");
        assert_eq!(us[0].label.as_deref(), Some("US-AL"));
        assert_eq!(us.last().unwrap().label.as_deref(), Some("US-DC"));

        let ca = country_block("🇨🇦");
        assert_eq!(ca.len(), 13, "10 provinces + 3 territories");
        assert_eq!(ca.last().unwrap().label.as_deref(), Some("CA-YT"));

        // A country without ISO subdivisions has no trailing block.
        assert!(country_block("🇦🇶").is_empty());
    }

    /// Containment ordering: France's regions (depth 1) precede their
    /// departments (depth 2), e.g. Auvergne-Rhône-Alpes before Paris.
    #[test]
    fn deeper_subdivisions_follow_their_parents() {
        let fr = country_block("🇫🇷");
        let ara = fr.iter().position(|e| e.label.as_deref() == Some("FR-ARA")).unwrap();
        let paris = fr.iter().position(|e| e.label.as_deref() == Some("FR-75C")).unwrap();
        assert!(ara < paris);
        assert_eq!(fr[ara].depth, 1);
        assert_eq!(fr[paris].depth, 2);
    }

    /// Every generated subdivision must have found a home after a country
    /// flag (no orphaned entries dropped by the interleave).
    #[test]
    fn every_subdivision_has_a_country_flag_home() {
        let placed = NATIVE_EMOJI_CATEGORIES[8].1
            .iter()
            .filter(|e| e.is_subdivision)
            .count();
        assert_eq!(placed, SUBDIVISIONS.len());
    }

    /// Subdivision search aliases: hyphenated, raw, and segment forms.
    #[test]
    fn subdivision_shortcodes_cover_all_alias_forms() {
        let flags = &NATIVE_EMOJI_CATEGORIES[8].1;
        let ky = flags
            .iter()
            .find(|e| e.label.as_deref() == Some("US-KY"))
            .expect("Kentucky entry");
        assert_eq!(ky.shortcodes, ["us-ky", "usky", "ky"]);
        assert_eq!(ky.name, "flag: Kentucky");
        assert!(!ky.has_tones);
    }

    /// The canonical subdivision order is the runtime probe's index contract:
    /// it must cover every generated subdivision exactly once.
    #[test]
    fn canonical_subdivisions_cover_every_entry_once() {
        assert_eq!(CANONICAL_SUBDIVISIONS.len(), SUBDIVISIONS.len());
        assert_eq!(SUBDIVISION_INDEX.len(), SUBDIVISIONS.len());
        for (idx, entry) in CANONICAL_SUBDIVISIONS.iter().enumerate() {
            assert_eq!(
                SUBDIVISION_INDEX.get(&entry.glyph),
                Some(&idx),
                "index mismatch for {}",
                entry.label.as_deref().unwrap_or_default()
            );
        }
        // The canonical order matches the interleaved Flags category order.
        let flags_order: Vec<&str> = NATIVE_EMOJI_CATEGORIES[8]
            .1
            .iter()
            .filter(|e| e.is_subdivision)
            .map(|e| e.glyph.as_str())
            .collect();
        let canonical: Vec<&str> =
            CANONICAL_SUBDIVISIONS.iter().map(|e| e.glyph.as_str()).collect();
        assert_eq!(flags_order, canonical);
    }
}
