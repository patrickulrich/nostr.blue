use dioxus::prelude::*;
use crate::stores::emoji_store::{CUSTOM_EMOJIS, EMOJI_SETS};

#[derive(Props, Clone, PartialEq)]
pub struct EmojiPickerProps {
    pub on_emoji_selected: EventHandler<String>,
}

/// Comprehensive emoji categories with extensive emoji coverage
const EMOJI_CATEGORIES: &[(&str, &[&str])] = &[
    // Smileys & Emotion (expanded)
    ("😀 Smileys", &[
        "😀", "😃", "😄", "😁", "😆", "😅", "🤣", "😂", "🙂", "🙃", "🫠", "😉", "😊", "😇",
        "🥰", "😍", "🤩", "😘", "😗", "☺️", "😚", "😙", "🥲", "😋", "😛", "😜", "🤪", "😝",
        "🤑", "🤗", "🤭", "🫢", "🫣", "🤫", "🤔", "🫡", "🤐", "🤨", "😐", "😑", "😶", "🫥",
        "😶‍🌫️", "😏", "😒", "🙄", "😬", "😮‍💨", "🤥", "🫨", "😌", "😔", "😪", "🤤"
    ]),

    // Love & Hearts (expanded)
    ("❤️ Love", &[
        "❤️", "🧡", "💛", "💚", "💙", "💜", "🤎", "🖤", "🤍", "💔", "❤️‍🔥", "❤️‍🩹",
        "❣️", "💕", "💞", "💓", "💗", "💖", "💘", "💝", "💟", "☮️", "✝️", "☪️", "🕉",
        "☸️", "✡️", "🔯", "🕎", "☯️", "☦️", "🛐", "⛎", "♈", "♉", "♊", "♋", "♌", "♍"
    ]),

    // Hand Gestures (expanded)
    ("👍 Hands", &[
        "👍", "👎", "👌", "✌️", "🤞", "🫰", "🤟", "🤘", "🤙", "👈", "👉", "👆", "🖕", "👇",
        "☝️", "🫵", "👋", "🤚", "🖐", "✋", "🖖", "🫱", "🫲", "🫳", "🫴", "👏", "🙌",
        "👐", "🤲", "🤝", "🙏", "✍️", "💅", "🤳", "💪", "🦾", "🦿", "🦵", "🦶", "👂", "🦻"
    ]),

    // Emotions & Faces (expanded)
    ("😢 Emotions", &[
        "🥺", "🥹", "😢", "😭", "😤", "😠", "😡", "🤬", "🤯", "😳", "🥵", "🥶", "😱", "😨",
        "😰", "😥", "😓", "🫗", "🤗", "🫣", "😖", "😣", "😞", "😟", "😔", "😕", "🙁", "☹️",
        "😩", "😫", "🥱", "😴", "😪", "🤤", "😮", "😦", "😧", "😯", "😲", "🥳", "🥸", "😎"
    ]),

    // People & Body (new)
    ("👤 People", &[
        "👶", "👧", "🧒", "👦", "👩", "🧑", "👨", "👩‍🦱", "🧑‍🦱", "👨‍🦱", "👩‍🦰", "🧑‍🦰",
        "👨‍🦰", "👱‍♀️", "👱", "👱‍♂️", "👩‍🦳", "🧑‍🦳", "👨‍🦳", "👩‍🦲", "🧑‍🦲", "👨‍🦲",
        "🧔‍♀️", "🧔", "🧔‍♂️", "👵", "🧓", "👴", "👲", "👳‍♀️", "👳", "👳‍♂️", "🧕", "👮‍♀️",
        "👮", "👮‍♂️", "👷‍♀️", "👷", "👷‍♂️", "💂‍♀️", "💂", "💂‍♂️", "🕵️‍♀️", "🕵️", "🕵️‍♂️"
    ]),

    // Animals & Nature (expanded)
    ("🐶 Animals", &[
        "🐶", "🐕", "🦮", "🐕‍🦺", "🐩", "🐺", "🦊", "🦝", "🐱", "🐈", "🐈‍⬛", "🦁", "🐯",
        "🐅", "🐆", "🐴", "🫎", "🫏", "🐎", "🦄", "🦓", "🦌", "🦬", "🐮", "🐂", "🐃", "🐄",
        "🐷", "🐖", "🐗", "🐽", "🐏", "🐑", "🐐", "🐪", "🐫", "🦙", "🦒", "🐘", "🦣", "🦏",
        "🦛", "🐭", "🐁", "🐀", "🐹", "🐰", "🐇", "🐿️", "🦫", "🦔", "🦇", "🐻", "🐻‍❄️"
    ]),

    // Food & Drink (new)
    ("🍕 Food", &[
        "🍏", "🍎", "🍐", "🍊", "🍋", "🍌", "🍉", "🍇", "🍓", "🫐", "🍈", "🍒", "🍑", "🥭",
        "🍍", "🥥", "🥝", "🍅", "🍆", "🥑", "🥦", "🥬", "🥒", "🌶️", "🫑", "🌽", "🥕", "🫒",
        "🧄", "🧅", "🥔", "🍠", "🥐", "🥯", "🍞", "🥖", "🥨", "🧀", "🥚", "🍳", "🧈", "🥞",
        "🧇", "🥓", "🥩", "🍗", "🍖", "🌭", "🍔", "🍟", "🍕", "🫓", "🥪", "🥙", "🧆", "🌮"
    ]),

    // Activities & Sports (expanded)
    ("⚽ Activity", &[
        "⚽", "🏀", "🏈", "⚾", "🥎", "🎾", "🏐", "🏉", "🥏", "🎱", "🪀", "🏓", "🏸", "🏒",
        "🏑", "🥍", "🏏", "🪃", "🥅", "⛳", "🪁", "🏹", "🎣", "🤿", "🥊", "🥋", "🎽", "🛹",
        "🛼", "🛷", "⛸️", "🥌", "🎿", "⛷️", "🏂", "🪂", "🏋️", "🤼", "🤸", "🤺", "🤾", "🏌️",
        "🏇", "🧘", "🏄", "🏊", "🤽", "🚣", "🧗", "🚴", "🚵", "🤹", "🎪", "🎭", "🎨", "🎬"
    ]),

    // Travel & Places (new)
    ("✈️ Travel", &[
        "🚗", "🚕", "🚙", "🚌", "🚎", "🏎️", "🚓", "🚑", "🚒", "🚐", "🛻", "🚚", "🚛", "🚜",
        "🦯", "🦽", "🦼", "🛴", "🚲", "🛵", "🏍️", "🛺", "🚨", "🚔", "🚍", "🚘", "🚖", "🚡",
        "🚠", "🚟", "🚃", "🚋", "🚞", "🚝", "🚄", "🚅", "🚈", "🚂", "🚆", "🚇", "🚊", "🚉",
        "✈️", "🛫", "🛬", "🛩️", "💺", "🛰️", "🚀", "🛸", "🚁", "🛶", "⛵", "🚤", "🛥️", "🛳️"
    ]),

    // Objects (new)
    ("💡 Objects", &[
        "⌚", "📱", "📲", "💻", "⌨️", "🖥️", "🖨️", "🖱️", "🖲️", "🕹️", "🗜️", "💾", "💿", "📀",
        "📼", "📷", "📸", "📹", "🎥", "📽️", "🎞️", "📞", "☎️", "📟", "📠", "📺", "📻", "🎙️",
        "🎚️", "🎛️", "🧭", "⏱️", "⏲️", "⏰", "🕰️", "⌛", "⏳", "📡", "🔋", "🪫", "🔌", "💡",
        "🔦", "🕯️", "🪔", "🧯", "🛢️", "💸", "💵", "💴", "💶", "💷", "🪙", "💰", "💳", "🧾"
    ]),

    // Symbols (expanded)
    ("⚡ Symbols", &[
        "⚡", "🔥", "💯", "✅", "☑️", "✔️", "❌", "❎", "➕", "➖", "➗", "✖️", "🟰", "💲",
        "💱", "™️", "©️", "®️", "〰️", "➰", "➿", "🔚", "🔙", "🔛", "🔝", "🔜", "✳️", "✴️",
        "❇️", "‼️", "⁉️", "❓", "❔", "❕", "❗", "〽️", "⚠️", "🚸", "🔱", "⚜️", "🔰", "♻️",
        "⭐", "🌟", "✨", "⚡", "💫", "💥", "💢", "💦", "💨", "🕊️", "🚀", "💎", "🔔", "🔕"
    ]),

    // Flags (popular countries)
    ("🏁 Flags", &[
        "🏁", "🚩", "🎌", "🏴", "🏳️", "🏳️‍🌈", "🏳️‍⚧️", "🏴‍☠️", "🇺🇸", "🇬🇧", "🇨🇦", "🇦🇺",
        "🇩🇪", "🇫🇷", "🇮🇹", "🇪🇸", "🇵🇹", "🇧🇷", "🇲🇽", "🇯🇵", "🇰🇷", "🇨🇳", "🇮🇳", "🇷🇺",
        "🇿🇦", "🇳🇬", "🇪🇬", "🇸🇦", "🇦🇪", "🇹🇷", "🇬🇷", "🇳🇱", "🇧🇪", "🇨🇭", "🇦🇹", "🇸🇪",
        "🇳🇴", "🇩🇰", "🇫🇮", "🇵🇱", "🇨🇿", "🇭🇺", "🇷🇴", "🇧🇬", "🇮🇪", "🇦🇷", "🇨🇱", "🇨🇴"
    ]),
];

#[derive(Clone, PartialEq)]
enum EmojiCategory {
    Standard(usize), // Index into EMOJI_CATEGORIES
    Custom,          // Custom emojis from user's emoji list
    Set(String),     // Emoji set by identifier
}

#[component]
pub fn EmojiPicker(props: EmojiPickerProps) -> Element {
    let mut show_picker = use_signal(|| false);
    let mut selected_category = use_signal(|| EmojiCategory::Standard(0));
    let mut position_below = use_signal(|| false); // Whether to show popup below button
    let button_id = use_signal(|| format!("emoji-picker-{}", uuid::Uuid::new_v4()));

    // Read custom emojis and sets from global state
    let custom_emojis = CUSTOM_EMOJIS.read();
    let emoji_sets = EMOJI_SETS.read();

    rsx! {
        div {
            class: "relative",

            // Emoji button
            button {
                id: "{button_id}",
                class: "px-3 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg text-sm font-medium transition",
                onclick: move |_| {
                    let current = *show_picker.read();
                    show_picker.set(!current);

                    // Calculate position when opening
                    if !current {
                        #[cfg(target_family = "wasm")]
                        {
                            let btn_id = button_id.read().clone();
                            if let Some(window) = web_sys::window() {
                                if let Some(document) = window.document() {
                                    if let Some(element) = document.get_element_by_id(&btn_id) {
                                        let rect = element.get_bounding_client_rect();
                                        let viewport_height = window
                                            .inner_height()
                                            .ok()
                                            .and_then(|h| h.as_f64())
                                            .unwrap_or(800.0);

                                        let button_center_y = rect.top() + (rect.height() / 2.0);
                                        let is_in_top_half = button_center_y < (viewport_height / 2.0);

                                        // If button is in top half, show popup below; otherwise show above
                                        position_below.set(is_in_top_half);
                                    }
                                }
                            }
                        }
                    }
                },
                "😀 Emoji"
            }

            // Emoji picker popover
            if *show_picker.read() {
                div {
                    class: if *position_below.read() {
                        "absolute top-full left-0 mt-2 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-xl z-50 w-80"
                    } else {
                        "absolute bottom-full left-0 mb-2 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-xl z-50 w-80"
                    },
                    onclick: move |e| e.stop_propagation(),

                    // Header
                    div {
                        class: "flex items-center justify-between p-3 border-b border-gray-200 dark:border-gray-700",
                        h3 {
                            class: "text-sm font-semibold",
                            "Select Emoji"
                        }
                        button {
                            class: "text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200",
                            onclick: move |_| show_picker.set(false),
                            "✕"
                        }
                    }

                    // Category tabs
                    div {
                        class: "flex gap-1 p-2 border-b border-gray-200 dark:border-gray-700 overflow-x-auto",

                        // Standard emoji categories
                        for (idx, (category_name, _)) in EMOJI_CATEGORIES.iter().enumerate() {
                            button {
                                key: "std-{idx}",
                                class: if *selected_category.read() == EmojiCategory::Standard(idx) {
                                    "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded text-xs font-medium whitespace-nowrap"
                                } else {
                                    "px-2 py-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded text-xs whitespace-nowrap"
                                },
                                onclick: move |_| selected_category.set(EmojiCategory::Standard(idx)),
                                "{category_name}"
                            }
                        }

                        // Custom emojis tab (if user has any)
                        if !custom_emojis.is_empty() {
                            button {
                                key: "custom",
                                class: if *selected_category.read() == EmojiCategory::Custom {
                                    "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded text-xs font-medium whitespace-nowrap"
                                } else {
                                    "px-2 py-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded text-xs whitespace-nowrap"
                                },
                                onclick: move |_| selected_category.set(EmojiCategory::Custom),
                                "⭐ Custom"
                            }
                        }

                        // Emoji set tabs
                        for set in emoji_sets.iter() {
                            {
                                let identifier = set.identifier.clone();
                                let identifier_for_key = identifier.clone();
                                let identifier_for_class = identifier.clone();
                                let set_name = set.name.clone().unwrap_or_else(|| set.identifier.clone());
                                let display_name = format!("📦 {}", set_name);
                                rsx! {
                                    button {
                                        key: "set-{identifier_for_key}",
                                        class: if *selected_category.read() == EmojiCategory::Set(identifier_for_class) {
                                            "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded text-xs font-medium whitespace-nowrap"
                                        } else {
                                            "px-2 py-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded text-xs whitespace-nowrap"
                                        },
                                        onclick: move |_| selected_category.set(EmojiCategory::Set(identifier.clone())),
                                        "{display_name}"
                                    }
                                }
                            }
                        }
                    }

                    // Emoji grid
                    div {
                        class: "p-3 max-h-60 overflow-y-auto",

                        // Render based on selected category
                        match selected_category.read().clone() {
                            EmojiCategory::Standard(idx) => rsx! {
                                div {
                                    class: "grid grid-cols-7 gap-2",
                                    for (emoji_idx, emoji) in EMOJI_CATEGORIES[idx].1.iter().enumerate() {
                                        button {
                                            key: "std-{idx}-{emoji_idx}",
                                            class: "text-2xl hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-2 transition",
                                            onclick: move |_| {
                                                props.on_emoji_selected.call(emoji.to_string());
                                                show_picker.set(false);
                                            },
                                            "{emoji}"
                                        }
                                    }
                                }
                            },
                            EmojiCategory::Custom => rsx! {
                                div {
                                    class: "grid grid-cols-5 gap-2",
                                    for (emoji_idx, custom_emoji) in custom_emojis.iter().enumerate() {
                                        {
                                            let shortcode = custom_emoji.shortcode.clone();
                                            let url = custom_emoji.image_url.clone();
                                            let url_for_click = url.clone();
                                            let title_text = format!(":{shortcode}:");
                                            let alt_text = format!(":{shortcode}:");
                                            rsx! {
                                                button {
                                                    key: "custom-{emoji_idx}",
                                                    class: "hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-2 transition flex items-center justify-center",
                                                    title: "{title_text}",
                                                    onclick: move |_| {
                                                        props.on_emoji_selected.call(format!(" {url_for_click} "));
                                                        show_picker.set(false);
                                                    },
                                                    img {
                                                        src: "{url}",
                                                        alt: "{alt_text}",
                                                        class: "w-8 h-8 object-contain"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            EmojiCategory::Set(identifier) => {
                                let set = emoji_sets.iter().find(|s| s.identifier == identifier);
                                let set_id = identifier.clone();
                                rsx! {
                                    div {
                                        class: "grid grid-cols-5 gap-2",
                                        if let Some(set) = set {
                                            for (emoji_idx, custom_emoji) in set.emojis.iter().enumerate() {
                                                {
                                                    let shortcode = custom_emoji.shortcode.clone();
                                                    let url = custom_emoji.image_url.clone();
                                                    let url_for_click = url.clone();
                                                    let title_text = format!(":{shortcode}:");
                                                    let alt_text = format!(":{shortcode}:");
                                                    rsx! {
                                                        button {
                                                            key: "set-{set_id}-{emoji_idx}",
                                                            class: "hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-2 transition flex items-center justify-center",
                                                            title: "{title_text}",
                                                            onclick: move |_| {
                                                                props.on_emoji_selected.call(format!(" {url_for_click} "));
                                                                show_picker.set(false);
                                                            },
                                                            img {
                                                                src: "{url}",
                                                                alt: "{alt_text}",
                                                                class: "w-8 h-8 object-contain"
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
                    }
                }
            }
        }
    }
}
