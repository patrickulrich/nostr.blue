use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct NativeEmojiEntry {
    pub glyph: String,
    pub name: String,
    pub group: String,
}

pub static ALL_NATIVE_EMOJIS: LazyLock<Vec<NativeEmojiEntry>> = LazyLock::new(|| {
    emoji::lookup_by_name::iter_emoji()
        .map(|emoji| NativeEmojiEntry {
            glyph: emoji.glyph.to_string(),
            name: emoji.name.to_string(),
            group: emoji.group.to_string(),
        })
        .collect()
});

pub static NATIVE_EMOJI_CATEGORIES: LazyLock<Vec<(String, Vec<NativeEmojiEntry>)>> =
    LazyLock::new(|| {
        let mut grouped: HashMap<String, Vec<NativeEmojiEntry>> = HashMap::new();

        for emoji in ALL_NATIVE_EMOJIS.iter() {
            grouped
                .entry(emoji.group.clone())
                .or_default()
                .push(emoji.clone());
        }

        let ordered_groups = [
            "Smileys & Emotion",
            "People & Body",
            "Animals & Nature",
            "Food & Drink",
            "Travel & Places",
            "Activities",
            "Objects",
            "Symbols",
            "Flags",
        ];

        let mut categories = Vec::new();
        for group in ordered_groups {
            if let Some(emojis) = grouped.remove(group) {
                categories.push((group.to_string(), emojis));
            }
        }

        let mut remaining: Vec<_> = grouped.into_iter().collect();
        remaining.sort_by(|a, b| a.0.cmp(&b.0));
        categories.extend(remaining);
        categories
    });
