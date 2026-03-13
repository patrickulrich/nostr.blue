mod data;

use self::data::{NativeEmojiEntry, ALL_NATIVE_EMOJIS, NATIVE_EMOJI_CATEGORIES};

use crate::components::EmojiPackManagerModal;
use crate::stores::emoji_store::{
    save_recent_emoji, CustomEmojisStoreStoreExt, EmojiSetsStoreStoreExt, CUSTOM_EMOJIS,
    EMOJI_SETS, RECENT_EMOJIS,
};
use crate::utils::custom_emoji::EmojiSelection;
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

#[derive(Props, Clone, PartialEq)]
pub struct EmojiPickerProps {
    pub on_emoji_selected: EventHandler<EmojiSelection>,
    #[props(default = false)]
    pub icon_only: bool,
    #[props(default = "Add emoji".to_string())]
    pub aria_label: String,
    #[props(default = false)]
    pub disabled: bool,
}

#[derive(Clone, PartialEq)]
enum EmojiCategory {
    Recent,
    Custom,
    Set(String),
    All,
    Standard(usize),
}

#[derive(Clone, PartialEq)]
enum SearchResult {
    Native(NativeEmojiEntry),
    Custom {
        shortcode: String,
        url: String,
        pack_coordinate: Option<String>,
    },
}

fn installed_custom_search_items() -> Vec<(String, String, Option<String>)> {
    let custom_store = CUSTOM_EMOJIS.read();
    let custom_data = custom_store.data();
    let custom_list = custom_data.read();

    let sets_store = EMOJI_SETS.read();
    let sets_data = sets_store.data();
    let sets_list = sets_data.read();

    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for emoji in custom_list.iter() {
        if seen.insert((emoji.shortcode.clone(), emoji.image_url.clone(), None)) {
            items.push((emoji.shortcode.clone(), emoji.image_url.clone(), None));
        }
    }

    for set in sets_list.iter() {
        let coordinate = format!("30030:{}:{}", set.author, set.identifier);
        for emoji in &set.emojis {
            if seen.insert((
                emoji.shortcode.clone(),
                emoji.image_url.clone(),
                Some(coordinate.clone()),
            )) {
                items.push((
                    emoji.shortcode.clone(),
                    emoji.image_url.clone(),
                    Some(coordinate.clone()),
                ));
            }
        }
    }

    items
}

fn resolve_recent_selection(
    entry: &str,
    installed_custom_map: &HashMap<String, (String, Option<String>)>,
) -> Option<EmojiSelection> {
    if !is_valid_http_url(entry) {
        return Some(EmojiSelection::Native {
            emoji: entry.to_string(),
        });
    }

    if let Some((shortcode, pack_coordinate)) = installed_custom_map.get(entry) {
        return Some(EmojiSelection::Custom {
            shortcode: shortcode.clone(),
            url: entry.to_string(),
            pack_coordinate: pack_coordinate.clone(),
        });
    }

    None
}

fn emoji_selection_shortcode(selection: &EmojiSelection) -> String {
    match selection {
        EmojiSelection::Native { emoji } => emoji.clone(),
        EmojiSelection::Custom { shortcode, .. } => shortcode.clone(),
    }
}

fn custom_result_rank(shortcode: &str, query: &str) -> i32 {
    if shortcode == query {
        0
    } else if shortcode.starts_with(query) {
        1
    } else if shortcode.contains(query) {
        2
    } else {
        3
    }
}

#[component]
pub fn EmojiPicker(props: EmojiPickerProps) -> Element {
    let mut show_picker = use_signal(|| false);
    let mut show_manage_modal = use_signal(|| false);
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

    let installed_custom_items = use_memo(installed_custom_search_items);
    let installed_custom_map = use_memo(move || {
        installed_custom_items
            .read()
            .iter()
            .map(|(shortcode, url, pack_coordinate)| {
                (url.clone(), (shortcode.clone(), pack_coordinate.clone()))
            })
            .collect::<HashMap<_, _>>()
    });
    let search_lower = use_memo(move || search_query.read().trim().to_lowercase());
    let is_searching = !search_lower.read().is_empty();

    let search_results = use_memo(move || {
        let query = search_lower.read();
        if query.is_empty() {
            return Vec::<SearchResult>::new();
        }

        let mut results = installed_custom_items
            .read()
            .iter()
            .filter(|(shortcode, _, _)| shortcode.to_lowercase().contains(query.as_str()))
            .map(|(shortcode, url, pack_coordinate)| SearchResult::Custom {
                shortcode: shortcode.clone(),
                url: url.clone(),
                pack_coordinate: pack_coordinate.clone(),
            })
            .collect::<Vec<_>>();

        let mut seen_native = HashSet::new();
        let native_matches = emoji::search::search_annotation(query.as_str(), "en")
            .into_iter()
            .filter_map(|emoji| {
                if seen_native.insert(emoji.glyph) {
                    Some(SearchResult::Native(NativeEmojiEntry {
                        glyph: emoji.glyph.to_string(),
                        name: emoji.name.to_string(),
                        group: emoji.group.to_string(),
                    }))
                } else {
                    None
                }
            });

        results.extend(native_matches);
        results.sort_by(|a, b| match (a, b) {
            (
                SearchResult::Custom { shortcode: a, .. },
                SearchResult::Custom { shortcode: b, .. },
            ) => custom_result_rank(a, query.as_str()).cmp(&custom_result_rank(b, query.as_str())),
            (SearchResult::Custom { .. }, SearchResult::Native(_)) => Ordering::Less,
            (SearchResult::Native(_), SearchResult::Custom { .. }) => Ordering::Greater,
            (SearchResult::Native(a), SearchResult::Native(b)) => a.name.cmp(&b.name),
        });
        results.truncate(80);
        results
    });

    let mut emit_selection = move |selection: EmojiSelection| {
        match &selection {
            EmojiSelection::Native { emoji } => save_recent_emoji(emoji.clone()),
            EmojiSelection::Custom { url, .. } => save_recent_emoji(url.clone()),
        }
        props.on_emoji_selected.call(selection);
        show_picker.set(false);
        search_query.set(String::new());
    };

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
                    if props.disabled {
                        return;
                    }
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
                                        let viewport_width = window.inner_width().ok().and_then(|w| w.as_f64()).unwrap_or(1024.0);
                                        let viewport_height = window.inner_height().ok().and_then(|h| h.as_f64()).unwrap_or(800.0);
                                        let is_mobile_view = viewport_width < 640.0;
                                        is_mobile.set(is_mobile_view);
                                        if is_mobile_view {
                                            picker_top.set(16.0);
                                            position_below.set(true);
                                        } else {
                                            let button_center_y = rect.top() + (rect.height() / 2.0);
                                            let is_in_top_half = button_center_y < (viewport_height / 2.0);
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
                        class: "fixed bg-background border border-border rounded-lg shadow-xl flex flex-col",
                        class: "w-[calc(100vw-2rem)] sm:w-[25rem]",
                        style: if *is_mobile.read() {
                            "top: 1rem; left: 1rem; right: 1rem; max-height: calc(100vh - 2rem);".to_string()
                        } else if *position_below.read() {
                            format!("top: {}px; left: {}px; max-height: min(36rem, calc(100vh - {}px - 1rem));", *picker_top.read(), *picker_left.read(), *picker_top.read())
                        } else {
                            format!("bottom: {}px; left: {}px; max-height: min(36rem, calc(100vh - {}px - 1rem));", *picker_bottom.read(), *picker_left.read(), *picker_bottom.read())
                        },
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
                                placeholder: "Search emojis and packs...",
                                value: "{search_query}",
                                oninput: move |evt| search_query.set(evt.value()),
                            }
                        }
                        if !is_searching {
                            div { class: "flex gap-1 p-2 border-b border-border overflow-x-auto",
                                button {
                                    class: if *selected_category.read() == EmojiCategory::Recent { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs whitespace-nowrap" },
                                    onclick: move |_| selected_category.set(EmojiCategory::Recent),
                                    "🕐 Recent"
                                }
                                if !custom_emojis.data().read().is_empty() || !emoji_sets.data().read().is_empty() {
                                    button {
                                        class: if *selected_category.read() == EmojiCategory::Custom { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs whitespace-nowrap" },
                                        onclick: move |_| selected_category.set(EmojiCategory::Custom),
                                        "⭐ Custom"
                                    }
                                }
                                for set in emoji_sets.data().read().iter() {
                                    {
                                        let coordinate = format!("30030:{}:{}", set.author, set.identifier);
                                        let display_name = set.name.clone().unwrap_or_else(|| set.identifier.clone());
                                        rsx! {
                                            button {
                                                key: "set-{coordinate}",
                                                class: if *selected_category.read() == EmojiCategory::Set(coordinate.clone()) { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs whitespace-nowrap" },
                                                onclick: move |_| selected_category.set(EmojiCategory::Set(coordinate.clone())),
                                                "📦 {display_name}"
                                            }
                                        }
                                    }
                                }
                                button {
                                    class: if *selected_category.read() == EmojiCategory::All { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs whitespace-nowrap" },
                                    onclick: move |_| selected_category.set(EmojiCategory::All),
                                    "🌐 All"
                                }
                                for (idx, (category_name, _)) in NATIVE_EMOJI_CATEGORIES.iter().enumerate() {
                                    button {
                                        key: "std-{idx}",
                                        class: if *selected_category.read() == EmojiCategory::Standard(idx) { "px-2 py-1 bg-accent text-foreground rounded text-xs font-medium whitespace-nowrap" } else { "px-2 py-1 text-muted-foreground hover:bg-accent rounded text-xs whitespace-nowrap" },
                                        onclick: move |_| selected_category.set(EmojiCategory::Standard(idx)),
                                        "{category_name}"
                                    }
                                }
                            }
                        }
                        div { class: "flex-1 overflow-y-auto p-3",
                            if is_searching {
                                if search_results.read().is_empty() {
                                    div { class: "py-8 text-center text-sm text-muted-foreground",
                                        "No emoji matches found."
                                    }
                                } else {
                                    div { class: "grid grid-cols-5 sm:grid-cols-6 gap-2",
                                        for (index, result) in search_results.read().iter().enumerate() {
                                            match result {
                                                SearchResult::Native(emoji) => {
                                                    let glyph = emoji.glyph.clone();
                                                    let title = emoji.name.clone();
                                                    rsx! {
                                                        button {
                                                            key: "search-native-{index}",
                                                            class: "rounded-lg p-2 text-2xl hover:bg-accent transition",
                                                            title: "{title}",
                                                            onclick: move |_| {
                                                                emit_selection(EmojiSelection::Native { emoji: glyph.clone() });
                                                            },
                                                            "{glyph}"
                                                        }
                                                    }
                                                }
                                                SearchResult::Custom { shortcode, url, pack_coordinate } => {
                                                    let shortcode_value = shortcode.clone();
                                                    let url_value = url.clone();
                                                    let title = format!(":{shortcode_value}:");
                                                    let invalid_url = !is_valid_http_url(url);
                                                    let transient_failed = failed_images.read().contains(url);
                                                    let has_error = invalid_url || transient_failed;
                                                    let pack_ref = pack_coordinate.clone();
                                                    rsx! {
                                                        button {
                                                            key: "search-custom-{index}",
                                                            class: "hover:bg-accent rounded p-2 transition flex items-center justify-center",
                                                            title: "{title}",
                                                            onclick: move |_| {
                                                                if invalid_url {
                                                                    return;
                                                                }
                                                                emit_selection(EmojiSelection::Custom {
                                                                    shortcode: shortcode_value.clone(),
                                                                    url: url_value.clone(),
                                                                    pack_coordinate: pack_ref.clone(),
                                                                });
                                                            },
                                                            if has_error {
                                                                span { class: "text-xs text-muted-foreground truncate max-w-[4rem]", ":{shortcode}:" }
                                                            } else {
                                                                img {
                                                                    src: "{url}",
                                                                    alt: ":{shortcode}:",
                                                                    class: "w-8 h-8 object-contain",
                                                                    loading: "lazy",
                                                                    onerror: {
                                                                        let url = url.clone();
                                                                        move |_| {
                                                                            failed_images.write().insert(url.clone());
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
                            } else {
                                match selected_category.read().clone() {
                                    EmojiCategory::Recent => rsx! {
                                        div { class: "grid grid-cols-5 sm:grid-cols-6 gap-2",
                                            for (emoji_idx, (entry, selection)) in recent_emojis
                                                .iter()
                                                .filter_map(|emoji| resolve_recent_selection(emoji, &installed_custom_map.read()).map(|selection| (emoji.clone(), selection)))
                                                .enumerate()
                                            {
                                                {
                                                    {
                                                        let selection_for_click = selection.clone();
                                                        let has_error = is_valid_http_url(&entry) && failed_images.read().contains(&entry);
                                                        let fallback_text = emoji_selection_shortcode(&selection);
                                                        rsx! {
                                                            button {
                                                                key: "recent-{emoji_idx}",
                                                                class: "rounded-lg p-2 hover:bg-accent transition flex items-center justify-center",
                                                                onclick: move |_| emit_selection(selection_for_click.clone()),
                                                                if is_valid_http_url(&entry) {
                                                                    if has_error {
                                                                        span { class: "text-xs text-muted-foreground", ":{fallback_text}:" }
                                                                    } else {
                                                                        img {
                                                                            src: "{entry}",
                                                                            alt: "recent custom emoji",
                                                                            class: "w-8 h-8 object-contain",
                                                                            loading: "lazy",
                                                                            onerror: {
                                                                                let entry = entry.clone();
                                                                                move |_| {
                                                                                    failed_images.write().insert(entry.clone());
                                                                                }
                                                                            },
                                                                        }
                                                                    }
                                                                } else {
                                                                    span { class: "text-2xl", "{entry}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            if recent_emojis
                                                .iter()
                                                .all(|emoji| resolve_recent_selection(emoji, &installed_custom_map.read()).is_none())
                                            {
                                                p { class: "col-span-full text-center text-muted-foreground text-sm py-4",
                                                    "No recent emojis yet. Select some emojis to see them here!"
                                                }
                                            }
                                        }
                                    },
                                    EmojiCategory::Custom => rsx! {
                                        div { class: "grid grid-cols-4 sm:grid-cols-5 gap-2",
                                            for (index, (shortcode, url, pack_coordinate)) in installed_custom_items.read().iter().enumerate() {
                                                {
                                                    let shortcode_value = shortcode.clone();
                                                    let url_value = url.clone();
                                                    let pack_ref = pack_coordinate.clone();
                                                    let invalid_url = !is_valid_http_url(url);
                                                    let transient_failed = failed_images.read().contains(url);
                                                    let has_error = invalid_url || transient_failed;
                                                    rsx! {
                                                        button {
                                                            key: "custom-{index}",
                                                            class: "hover:bg-accent rounded p-2 transition flex items-center justify-center",
                                                            title: ":{shortcode_value}:",
                                                            onclick: move |_| {
                                                                if invalid_url {
                                                                    return;
                                                                }
                                                                emit_selection(EmojiSelection::Custom {
                                                                    shortcode: shortcode_value.clone(),
                                                                    url: url_value.clone(),
                                                                    pack_coordinate: pack_ref.clone(),
                                                                });
                                                            },
                                                            if has_error {
                                                                span { class: "text-xs text-muted-foreground truncate max-w-[4rem]", ":{shortcode}:" }
                                                            } else {
                                                                img {
                                                                    src: "{url}",
                                                                    alt: ":{shortcode}:",
                                                                    class: "w-8 h-8 object-contain",
                                                                    loading: "lazy",
                                                                    onerror: {
                                                                        let url = url.clone();
                                                                        move |_| {
                                                                            failed_images.write().insert(url.clone());
                                                                        }
                                                                    },
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    EmojiCategory::Set(coordinate) => {
                                        let sets_data = emoji_sets.data();
                                        let sets_guard = sets_data.read();
                                        let set = sets_guard.iter().find(|s| format!("30030:{}:{}", s.author, s.identifier) == coordinate);
                                        rsx! {
                                            div { class: "grid grid-cols-4 sm:grid-cols-5 gap-2",
                                                if let Some(set) = set {
                                                    for (index, custom_emoji) in set.emojis.iter().enumerate() {
                                                        {
                                                            let shortcode = custom_emoji.shortcode.clone();
                                                            let url = custom_emoji.image_url.clone();
                                                            let pack_coordinate = Some(format!("30030:{}:{}", set.author, set.identifier));
                                                            let invalid_url = !is_valid_http_url(&url);
                                                            let transient_failed = failed_images.read().contains(&url);
                                                            let has_error = invalid_url || transient_failed;
                                                            rsx! {
                                                                button {
                                                                    key: "set-{coordinate}-{index}",
                                                                    class: "hover:bg-accent rounded p-2 transition flex items-center justify-center",
                                                                    title: ":{shortcode}:",
                                                                    onclick: move |_| {
                                                                        if invalid_url {
                                                                            return;
                                                                        }
                                                                        emit_selection(EmojiSelection::Custom {
                                                                            shortcode: shortcode.clone(),
                                                                            url: url.clone(),
                                                                            pack_coordinate: pack_coordinate.clone(),
                                                                        });
                                                                    },
                                                                    if has_error {
                                                                        span { class: "text-xs text-muted-foreground truncate max-w-[4rem]", ":{shortcode}:" }
                                                                    } else {
                                                                        img {
                                                                            src: "{url}",
                                                                            alt: ":{shortcode}:",
                                                                            class: "w-8 h-8 object-contain",
                                                                            loading: "lazy",
                                                                            onerror: {
                                                                                let url = url.clone();
                                                                                move |_| {
                                                                                    failed_images.write().insert(url.clone());
                                                                                }
                                                                            },
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    p { class: "col-span-full text-center text-muted-foreground text-sm py-4",
                                                        "Emoji set not found"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    EmojiCategory::All => rsx! {
                                        div { class: "grid grid-cols-5 sm:grid-cols-6 gap-2",
                                            for (index, emoji) in ALL_NATIVE_EMOJIS.iter().enumerate() {
                                                {
                                                    let glyph = emoji.glyph.clone();
                                                    let title = emoji.name.clone();
                                                    rsx! {
                                                        button {
                                                            key: "all-{index}",
                                                            class: "text-2xl hover:bg-accent rounded p-2 transition",
                                                            title: "{title}",
                                                            onclick: move |_| emit_selection(EmojiSelection::Native { emoji: glyph.clone() }),
                                                            "{glyph}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    EmojiCategory::Standard(idx) => {
                                        let items = NATIVE_EMOJI_CATEGORIES
                                            .get(idx)
                                            .map(|(_, items)| items.as_slice())
                                            .unwrap_or(&[]);
                                        rsx! {
                                            div { class: "grid grid-cols-5 sm:grid-cols-6 gap-2",
                                                for (emoji_idx, emoji) in items.iter().enumerate() {
                                                    {
                                                        let glyph = emoji.glyph.clone();
                                                        let title = emoji.name.clone();
                                                        rsx! {
                                                            button {
                                                                key: "std-{idx}-{emoji_idx}",
                                                                class: "text-2xl hover:bg-accent rounded p-2 transition",
                                                                title: "{title}",
                                                                onclick: move |_| emit_selection(EmojiSelection::Native { emoji: glyph.clone() }),
                                                                "{glyph}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "border-t border-border p-3 bg-muted/40 rounded-b-lg",
                            button {
                                class: "w-full rounded-xl border border-border px-4 py-3 text-sm font-medium hover:bg-accent transition flex items-center justify-center gap-2",
                                onclick: move |_| show_manage_modal.set(true),
                                span { "📦" }
                                "Manage Emoji Packs"
                            }
                        }
                    }
                }
                EmojiPackManagerModal {
                    show: show_manage_modal,
                    on_close: move |_| show_manage_modal.set(false),
                }
            }
        }
    }
}
