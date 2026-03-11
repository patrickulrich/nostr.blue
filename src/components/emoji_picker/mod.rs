#![allow(unused_imports)]

mod data;

use data::EMOJI_CATEGORIES;

use crate::stores::emoji_store::{
    save_recent_emoji, CustomEmojisStoreStoreExt, EmojiSetsStoreStoreExt, CUSTOM_EMOJIS,
    EMOJI_SETS, RECENT_EMOJIS,
};
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use std::collections::HashSet;
use std::sync::LazyLock;

static UNIQUE_EMOJI_CATEGORIES: LazyLock<Vec<Vec<&'static str>>> = LazyLock::new(|| {
    EMOJI_CATEGORIES
        .iter()
        .map(|(_, emojis)| {
            let mut seen = HashSet::new();
            emojis
                .iter()
                .copied()
                .filter(|emoji| seen.insert(*emoji))
                .collect::<Vec<_>>()
        })
        .collect()
});
#[derive(Props, Clone, PartialEq)]
pub struct EmojiPickerProps {
    pub on_emoji_selected: EventHandler<String>,
    #[props(default = false)]
    pub icon_only: bool,
    /// ARIA label for the emoji picker button (for accessibility)
    #[props(default = "Add emoji".to_string())]
    pub aria_label: String,
    /// Whether the picker is disabled (e.g., during form submission)
    #[props(default = false)]
    pub disabled: bool,
}
#[derive(Clone, PartialEq)]
enum EmojiCategory {
    Recent,
    Custom,
    Set(String),
    Standard(usize),
}
#[component]
pub fn EmojiPicker(props: EmojiPickerProps) -> Element {
    let mut show_picker = use_signal(|| false);
    let mut selected_category = use_signal(|| EmojiCategory::Recent);
    let mut search_query = use_signal(String::new);
    #[allow(unused_mut)]
    let mut position_below = use_signal(|| false);
    let button_id = use_signal(|| format!("emoji-picker-{}", uuid::Uuid::new_v4()));
    #[allow(unused_mut)]
    let mut picker_top = use_signal(|| 0.0);
    #[allow(unused_mut)]
    let mut picker_bottom = use_signal(|| 0.0);
    #[allow(unused_mut)]
    let mut picker_left = use_signal(|| 0.0);
    #[allow(unused_mut)]
    let mut is_mobile = use_signal(|| false);
    let mut failed_images: Signal<HashSet<String>> = use_signal(HashSet::new);
    let custom_emojis = CUSTOM_EMOJIS.read();
    let emoji_sets = EMOJI_SETS.read();
    let recent_emojis = RECENT_EMOJIS.read();
    let search_lower = use_memo(move || search_query.read().to_lowercase());
    let is_searching = !search_lower.read().is_empty();
    let search_results = use_memo(move || {
        let query = search_lower.read();
        if query.is_empty() {
            return Vec::new();
        }
        emoji::search::search_name(&query)
            .into_iter()
            .take(50)
            .map(|e| e.glyph.to_string())
            .collect::<Vec<_>>()
    });
    rsx! {
        div { class: "relative",
            button {
                id: "{button_id}",
                class: if props.disabled {
                    if props.icon_only { "p-2 rounded-lg opacity-50 cursor-not-allowed" } else { "px-3 py-2 bg-muted text-foreground rounded-lg text-sm font-medium opacity-50 cursor-not-allowed" }
                } else {
                    if props.icon_only { "p-2 hover:bg-accent rounded-lg transition" } else { "px-3 py-2 bg-muted text-foreground hover:bg-accent rounded-lg text-sm font-medium transition" }
                },
                title: if props.icon_only { "Add emoji" } else { "" },
                aria_label: if props.icon_only { "{props.aria_label}" } else { "" },
                disabled: props.disabled,
                onclick: move |_| {
                    if props.disabled { return; }
                    let current = *show_picker.read();
                    show_picker.set(!current);
                    if !current {
                        #[cfg(feature = "web")]
                        {
                            let btn_id = button_id.read().clone();
                            if let Some(window) = web_sys::window() {
                                if let Some(document) = window.document() {
                                    if let Some(element) = document.get_element_by_id(&btn_id) {
                                        let rect = element.get_bounding_client_rect();
                                        let viewport_width = window
                                            .inner_width()
                                            .ok()
                                            .and_then(|w| w.as_f64())
                                            .unwrap_or(1024.0);
                                        let viewport_height = window
                                            .inner_height()
                                            .ok()
                                            .and_then(|h| h.as_f64())
                                            .unwrap_or(800.0);
                                        let is_mobile_view = viewport_width < 640.0;
                                        is_mobile.set(is_mobile_view);
                                        if is_mobile_view {
                                            picker_top.set(16.0);
                                            position_below.set(true);
                                        } else {
                                            let button_center_y = rect.top() + (rect.height() / 2.0);
                                            let is_in_top_half = button_center_y
                                                < (viewport_height / 2.0);
                                            picker_left.set(rect.left());
                                            if is_in_top_half {
                                                picker_top.set(rect.bottom() + 8.0);
                                                position_below.set(true);
                                            } else {
                                                picker_bottom.set(viewport_height - rect.top() + 8.0);
                                                position_below.set(false);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        #[cfg(not(feature = "web"))]
                        {
                            is_mobile.set(false);
                            position_below.set(true);
                            picker_top.set(16.0);
                            picker_left.set(16.0);
                            picker_bottom.set(0.0);
                        }
                    }
                },
                if props.icon_only {
                    "😀"
                } else {
                    "😀 Emoji"
                }
            }
            if *show_picker.read() {
                div {
                    class: "fixed inset-0 z-[60]",
                    onclick: move |_| show_picker.set(false),
                    div {
                        class: "fixed bg-background border border-border rounded-lg shadow-xl",
                        class: "w-[calc(100vw-2rem)] sm:w-80",
                        style: if *is_mobile.read() { "top: 1rem; left: 1rem; right: 1rem;".to_string() } else if *position_below.read() { format!("top: {}px; left: {}px;", *picker_top.read(), *picker_left.read()) } else { format!("bottom: {}px; left: {}px;", *picker_bottom.read(), *picker_left.read()) },
                        onclick: move |e| e.stop_propagation(),
                    div { class: "flex items-center justify-between p-3 border-b border-border",
                        h3 { class: "text-sm font-semibold", "Select Emoji" }
                        button {
                            class: "text-muted-foreground hover:text-foreground",
                            onclick: move |_| show_picker.set(false),
                            "✕"
                        }
                    }
                    div { class: "p-2 border-b border-border",
                        input {
                            r#type: "text",
                            class: "w-full px-3 py-2 text-sm bg-muted border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                            placeholder: "Search emojis...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                    }
                    if !is_searching {
                        div { class: "flex gap-1 p-2 border-b border-border overflow-x-auto",
                            button {
                                key: "recent",
                                class: if *selected_category.read() == EmojiCategory::Recent { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs whitespace-nowrap" },
                                onclick: move |_| selected_category.set(EmojiCategory::Recent),
                                "🕐 Recent"
                            }
                            if !custom_emojis.data().read().is_empty() {
                                {
                                    let custom_key = "custom";
                                    rsx! {
                                        button {
                                            key: "{custom_key}",
                                            class: if *selected_category.read() == EmojiCategory::Custom { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs whitespace-nowrap" },
                                            onclick: move |_| selected_category.set(EmojiCategory::Custom),
                                            "⭐ Custom"
                                        }
                                    }
                                }
                            }
                            for set in emoji_sets.data().read().iter() {
                                {
                                    let identifier = set.identifier.clone();
                                    let identifier_for_key = identifier.clone();
                                    let identifier_for_class = identifier.clone();
                                    let set_name = set.name.clone().unwrap_or_else(|| set.identifier.clone());
                                    let display_name = format!("📦 {}", set_name);
                                    rsx! {
                                        button {
                                            key: "set-{identifier_for_key}",
                                            class: if *selected_category.read() == EmojiCategory::Set(identifier_for_class) { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs whitespace-nowrap" },
                                            onclick: move |_| selected_category.set(EmojiCategory::Set(identifier.clone())),
                                            "{display_name}"
                                        }
                                    }
                                }
                            }
                            for (idx , (category_name , _)) in EMOJI_CATEGORIES.iter().enumerate() {
                                button {
                                    key: "std-{idx}",
                                    class: if *selected_category.read() == EmojiCategory::Standard(idx) { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs whitespace-nowrap" },
                                    onclick: move |_| selected_category.set(EmojiCategory::Standard(idx)),
                                    "{category_name}"
                                }
                            }
                        }
                    }
                    div { class: "p-3 max-h-60 overflow-y-auto",
                        if is_searching {
                            div { class: "grid grid-cols-5 sm:grid-cols-6 md:grid-cols-7 gap-2",
                                for (emoji_idx , emoji_str) in search_results.read().iter().enumerate() {
                                    {
                                        let emoji_for_click = emoji_str.clone();
                                        let emoji_display = emoji_str.clone();
                                        rsx! {
                                            button {
                                                key: "search-{emoji_idx}",
                                                class: "text-2xl hover:bg-accent rounded p-2 transition",
                                                onclick: move |_| {
                                                    save_recent_emoji(emoji_for_click.clone());
                                                    props.on_emoji_selected.call(emoji_for_click.clone());
                                                    show_picker.set(false);
                                                    search_query.set(String::new());
                                                },
                                                "{emoji_display}"
                                            }
                                        }
                                    }
                                }
                                for (emoji_idx , custom_emoji) in custom_emojis.data().read().iter().enumerate() {
                                    if custom_emoji.shortcode.to_lowercase().contains(search_lower.read().as_str()) {
                                        {
                                            let shortcode = custom_emoji.shortcode.clone();
                                            let url = custom_emoji.image_url.clone();
                                            let url_for_click = url.clone();
                                            let url_for_error = url.clone();
                                            let title_text = format!(":{shortcode}:");
                                            let alt_text = format!(":{shortcode}:");
                                            let shortcode_display = format!(":{shortcode}:");
                                            let has_error = !is_valid_http_url(&url) || failed_images.read().contains(&url);
                                            rsx! {
                                                button {
                                                    key: "search-custom-{emoji_idx}",
                                                    class: "hover:bg-accent rounded p-2 transition flex items-center justify-center",
                                                    title: "{title_text}",
                                                    onclick: move |_| {
                                                        save_recent_emoji(url_for_click.clone());
                                                        props.on_emoji_selected.call(format!(" {url_for_click} "));
                                                        show_picker.set(false);
                                                        search_query.set(String::new());
                                                    },
                                                    if has_error {
                                                        span { class: "text-xs text-muted-foreground truncate max-w-[4rem]", "{shortcode_display}" }
                                                    } else {
                                                        img {
                                                            src: "{url}",
                                                            alt: "{alt_text}",
                                                            class: "w-8 h-8 object-contain",
                                                            loading: "lazy",
                                                            onerror: move |_| {
                                                                failed_images.write().insert(url_for_error.clone());
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            match selected_category.read().clone() {
                                EmojiCategory::Recent => rsx! {
                                    div { class: "grid grid-cols-5 sm:grid-cols-6 md:grid-cols-7 gap-2",
                                        for (emoji_idx , emoji) in recent_emojis.iter().enumerate() {
                                            {
                                                let emoji_str = emoji.clone();
                                                let emoji_for_click = emoji_str.clone();
                                                let emoji_for_error = emoji_str.clone();
                                                let is_url = is_valid_http_url(&emoji_str);
                                                let has_error = is_url && failed_images.read().contains(&emoji_str);
                                                rsx! {
                                                    button {
                                                        key: "recent-{emoji_idx}",
                                                        class: "text-2xl hover:bg-accent rounded p-2 transition flex items-center justify-center",
                                                        onclick: move |_| {
                                                            save_recent_emoji(emoji_for_click.clone());
                                                            if is_url {
                                                                props.on_emoji_selected.call(format!(" {} ", emoji_for_click));
                                                            } else {
                                                                props.on_emoji_selected.call(emoji_for_click.clone());
                                                            }
                                                            show_picker.set(false);
                                                        },
                                                        if is_url {
                                                            if has_error {
                                                                span { class: "text-xs text-muted-foreground", "🖼️" }
                                                            } else {
                                                                img {
                                                                    src: "{emoji_str}",
                                                                    alt: "custom emoji",
                                                                    class: "w-8 h-8 object-contain",
                                                                    loading: "lazy",
                                                                    onerror: move |_| {
                                                                        failed_images.write().insert(emoji_for_error.clone());
                                                                    },
                                                                }
                                                            }
                                                        } else {
                                                            "{emoji_str}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if recent_emojis.is_empty() {
                                            p { class: "col-span-full text-center text-muted-foreground text-sm py-4",
                                                "No recent emojis yet. Select some emojis to see them here!"
                                            }
                                        }
                                    }
                                },
                                EmojiCategory::Custom => rsx! {
                                    div { class: "grid grid-cols-4 sm:grid-cols-5 gap-2",
                                        for (emoji_idx , custom_emoji) in custom_emojis.data().read().iter().enumerate() {
                                            {
                                                let shortcode = custom_emoji.shortcode.clone();
                                                let url = custom_emoji.image_url.clone();
                                                let url_for_click = url.clone();
                                                let url_for_save = url.clone();
                                                let url_for_error = url.clone();
                                                let title_text = format!(":{shortcode}:");
                                                let alt_text = format!(":{shortcode}:");
                                                let shortcode_display = format!(":{shortcode}:");
                                                let has_error = !is_valid_http_url(&url) || failed_images.read().contains(&url);
                                                rsx! {
                                                    button {
                                                        key: "custom-{emoji_idx}",
                                                        class: "hover:bg-accent rounded p-2 transition flex items-center justify-center",
                                                        title: "{title_text}",
                                                        onclick: move |_| {
                                                            save_recent_emoji(url_for_save.clone());
                                                            props.on_emoji_selected.call(format!(" {url_for_click} "));
                                                            show_picker.set(false);
                                                        },
                                                        if has_error {
                                                            span { class: "text-xs text-muted-foreground truncate max-w-[4rem]", "{shortcode_display}" }
                                                        } else {
                                                            img {
                                                                src: "{url}",
                                                                alt: "{alt_text}",
                                                                class: "w-8 h-8 object-contain",
                                                                loading: "lazy",
                                                                onerror: move |_| {
                                                                    failed_images.write().insert(url_for_error.clone());
                                                                },
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                EmojiCategory::Set(identifier) => {
                                    let sets_data = emoji_sets.data();
                                    let sets_guard = sets_data.read();
                                    let set = sets_guard.iter().find(|s| s.identifier == identifier);
                                    let set_id = identifier.clone();
                                    rsx! {
                                        div { class: "grid grid-cols-4 sm:grid-cols-5 gap-2",
                                            if let Some(set) = set {
                                                for (emoji_idx , custom_emoji) in set.emojis.iter().enumerate() {
                                                    {
                                                        let shortcode = custom_emoji.shortcode.clone();
                                                        let url = custom_emoji.image_url.clone();
                                                        let url_for_click = url.clone();
                                                        let url_for_save = url.clone();
                                                        let url_for_error = url.clone();
                                                        let title_text = format!(":{shortcode}:");
                                                        let alt_text = format!(":{shortcode}:");
                                                        let shortcode_display = format!(":{shortcode}:");
                                                        let has_error = !is_valid_http_url(&url) || failed_images.read().contains(&url);
                                                        rsx! {
                                                            button {
                                                                key: "set-{set_id}-{emoji_idx}",
                                                                class: "hover:bg-accent rounded p-2 transition flex items-center justify-center",
                                                                title: "{title_text}",
                                                                onclick: move |_| {
                                                                    save_recent_emoji(url_for_save.clone());
                                                                    props.on_emoji_selected.call(format!(" {url_for_click} "));
                                                                    show_picker.set(false);
                                                                },
                                                                if has_error {
                                                                    span { class: "text-xs text-muted-foreground truncate max-w-[4rem]", "{shortcode_display}" }
                                                                } else {
                                                                    img {
                                                                        src: "{url}",
                                                                        alt: "{alt_text}",
                                                                        class: "w-8 h-8 object-contain",
                                                                        loading: "lazy",
                                                                        onerror: move |_| {
                                                                            failed_images.write().insert(url_for_error.clone());
                                                                        },
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            } else {
                                                p { class: "col-span-full text-center text-muted-foreground text-sm py-4", "Emoji set not found" }
                                            }
                                        }
                                    }
                                }
                                EmojiCategory::Standard(idx) => rsx! {
                                    {
                                        let unique_emojis = &UNIQUE_EMOJI_CATEGORIES[idx];
                                        rsx! {
                                    div { class: "grid grid-cols-5 sm:grid-cols-6 md:grid-cols-7 gap-2",
                                        for (emoji_idx , emoji) in unique_emojis.iter().enumerate() {
                                            {
                                                let emoji_str = emoji.to_string();
                                                let emoji_for_click = emoji_str.clone();
                                                rsx! {
                                                    button {
                                                        key: "std-{idx}-{emoji_idx}",
                                                        class: "text-2xl hover:bg-accent rounded p-2 transition",
                                                        onclick: move |_| {
                                                            save_recent_emoji(emoji_for_click.clone());
                                                            props.on_emoji_selected.call(emoji_for_click.clone());
                                                            show_picker.set(false);
                                                        },
                                                        "{emoji_str}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                        }
                                    }
                                },
                            }
                        }
                    }
                    }
                }
            }
        }
    }
}
