use crate::stores::emoji_store::{
    CustomEmoji, CustomEmojisStoreStoreExt, EmojiSetsStoreStoreExt, CUSTOM_EMOJIS, EMOJI_SETS,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};

pub static CUSTOM_EMOJI_SHORTCODE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r":([A-Za-z0-9_]+):").expect("custom emoji shortcode regex should compile")
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmojiSelection {
    Native {
        emoji: String,
    },
    Custom {
        shortcode: String,
        url: String,
        pack_coordinate: Option<String>,
    },
}

impl EmojiSelection {
    pub fn insertion_text(&self) -> String {
        match self {
            Self::Native { emoji } => emoji.clone(),
            Self::Custom { shortcode, .. } => format!(":{shortcode}:"),
        }
    }
}

pub fn is_valid_shortcode(shortcode: &str) -> bool {
    !shortcode.is_empty()
        && shortcode
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn collect_installed_custom_emojis() -> Vec<CustomEmoji> {
    let custom_store = CUSTOM_EMOJIS.read();
    let custom_data = custom_store.data();
    let custom_list = custom_data.read();

    let sets_store = EMOJI_SETS.read();
    let sets_data = sets_store.data();
    let sets_list = sets_data.read();

    let mut seen = HashSet::new();
    let mut emojis = Vec::new();

    for emoji in custom_list.iter() {
        if seen.insert(emoji.shortcode.clone()) {
            emojis.push(emoji.clone());
        }
    }

    for set in sets_list.iter() {
        for emoji in &set.emojis {
            if seen.insert(emoji.shortcode.clone()) {
                emojis.push(emoji.clone());
            }
        }
    }

    emojis
}

pub fn installed_custom_emoji_map() -> HashMap<String, String> {
    collect_installed_custom_emojis()
        .into_iter()
        .map(|emoji| (emoji.shortcode, emoji.image_url))
        .collect()
}

pub fn build_custom_emoji_tags(content: &str) -> Vec<Tag> {
    let emoji_map = installed_custom_emoji_map();
    let mut seen = HashSet::new();
    let mut tags = Vec::new();

    for captures in CUSTOM_EMOJI_SHORTCODE_RE.captures_iter(content) {
        let Some(shortcode_match) = captures.get(1) else {
            continue;
        };

        let shortcode = shortcode_match.as_str();
        if !is_valid_shortcode(shortcode) || !seen.insert(shortcode.to_string()) {
            continue;
        }

        let Some(url_str) = emoji_map.get(shortcode) else {
            continue;
        };

        let Ok(url) = Url::parse(url_str) else {
            log::warn!("Skipping invalid custom emoji URL for shortcode {}: {}", shortcode, url_str);
            continue;
        };

        tags.push(Tag::from_standardized_without_cell(TagStandard::Emoji {
            shortcode: shortcode.to_string(),
            url,
        }));
    }

    tags
}

pub fn render_custom_emoji_text(
    text: &str,
    emoji_map: &HashMap<String, String>,
    image_class: &str,
) -> Element {
    let mut nodes: Vec<Element> = Vec::new();
    let mut last_end = 0usize;

    for (match_index, captures) in CUSTOM_EMOJI_SHORTCODE_RE.captures_iter(text).enumerate() {
        let Some(full_match) = captures.get(0) else {
            continue;
        };
        let Some(shortcode_match) = captures.get(1) else {
            continue;
        };

        let shortcode = shortcode_match.as_str();
        let Some(url) = emoji_map.get(shortcode) else {
            continue;
        };

        if full_match.start() > last_end {
            let text_part = text[last_end..full_match.start()].to_string();
            nodes.push(rsx! { span { key: "text-{match_index}-{last_end}", "{text_part}" } });
        }

        let url_value = url.clone();
        let alt_text = format!(":{shortcode}:");
        let key = format!("emoji-{match_index}-{}", full_match.start());
        nodes.push(rsx! {
            img {
                key: "{key}",
                src: "{url_value}",
                alt: "{alt_text}",
                class: "{image_class}",
                loading: "lazy",
            }
        });

        last_end = full_match.end();
    }

    if nodes.is_empty() {
        return rsx! { span { "{text}" } };
    }

    if last_end < text.len() {
        let tail = text[last_end..].to_string();
        nodes.push(rsx! { span { key: "text-tail-{last_end}", "{tail}" } });
    }

    rsx! {
        Fragment {
            for node in nodes {
                {node}
            }
        }
    }
}
