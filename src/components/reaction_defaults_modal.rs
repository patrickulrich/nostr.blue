//! Modal for customizing preferred reaction emojis
//! Supports drag-to-reorder and adding both standard unicode and NIP-30 custom emojis

use dioxus::prelude::*;
use crate::stores::reactions_store::{
    PreferredReaction, PREFERRED_REACTIONS, MAX_REACTIONS, save_preferred_reactions
};
use crate::stores::emoji_store::{CUSTOM_EMOJIS, EMOJI_SETS, CustomEmojisStoreStoreExt, EmojiSetsStoreStoreExt};
use crate::components::EmojiPicker;
use crate::components::icons::SettingsIcon;

#[derive(Props, Clone, PartialEq)]
pub struct ReactionDefaultsModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn ReactionDefaultsModal(props: ReactionDefaultsModalProps) -> Element {
    // Local state (copy of global for editing)
    let mut local_reactions = use_signal(|| PREFERRED_REACTIONS.read().clone());
    let mut new_emoji_input = use_signal(|| String::new());
    let mut saving = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    // Drag state
    let mut dragging_index = use_signal(|| None::<usize>);
    let mut drag_over_index = use_signal(|| None::<usize>);

    // Handle adding new emoji from text input
    let mut add_emoji_from_input = move |_| {
        let input = new_emoji_input.read().trim().to_string();
        if input.is_empty() || local_reactions.read().len() >= MAX_REACTIONS {
            return;
        }

        let reaction = if input.starts_with(':') && input.ends_with(':') && input.len() > 2 {
            // Custom emoji - look up URL from user's emoji list
            let shortcode = input[1..input.len()-1].to_string();

            // Try to find the custom emoji in user's emoji store
            let custom_emojis_store = CUSTOM_EMOJIS.read();
            let custom_emojis_data = custom_emojis_store.data();
            let custom_emojis_list = custom_emojis_data.read();
            let found_emoji = custom_emojis_list.iter()
                .find(|e| e.shortcode == shortcode);

            if let Some(emoji) = found_emoji {
                PreferredReaction::Custom {
                    shortcode: shortcode.clone(),
                    url: emoji.image_url.clone()
                }
            } else {
                // Custom emoji not found, show error
                error_msg.set(Some(format!("Custom emoji :{}: not found in your emoji list", shortcode)));
                return;
            }
        } else {
            // Standard unicode emoji
            PreferredReaction::Standard { emoji: input }
        };

        // Check for duplicates
        let reactions = local_reactions.read();
        let is_duplicate = reactions.iter().any(|r| match (r, &reaction) {
            (PreferredReaction::Standard { emoji: a }, PreferredReaction::Standard { emoji: b }) => a == b,
            (PreferredReaction::Custom { shortcode: a, .. }, PreferredReaction::Custom { shortcode: b, .. }) => a == b,
            _ => false,
        });

        if is_duplicate {
            error_msg.set(Some("This emoji is already in your list".to_string()));
            return;
        }

        drop(reactions);
        local_reactions.write().push(reaction);
        new_emoji_input.set(String::new());
        error_msg.set(None);
    };

    // Handle adding emoji from picker
    let add_emoji_from_picker = move |emoji: String| {
        if local_reactions.read().len() >= MAX_REACTIONS {
            return;
        }

        let trimmed = emoji.trim();

        // Check if this is a custom emoji URL (EmojiPicker returns URLs for custom emojis)
        let reaction = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            // Look up the custom emoji by URL to get the shortcode
            // First check in user's direct custom emojis
            let custom_emojis_store = CUSTOM_EMOJIS.read();
            let custom_emojis_data = custom_emojis_store.data();
            let custom_emojis_list = custom_emojis_data.read();

            let found_in_custom = custom_emojis_list.iter().find(|e| e.image_url == trimmed);

            if let Some(found) = found_in_custom {
                PreferredReaction::Custom {
                    shortcode: found.shortcode.clone(),
                    url: found.image_url.clone(),
                }
            } else {
                // Not in direct custom emojis, check emoji sets
                drop(custom_emojis_list);
                let _ = custom_emojis_data;
                drop(custom_emojis_store);

                let emoji_sets_store = EMOJI_SETS.read();
                let emoji_sets_data = emoji_sets_store.data();
                let emoji_sets_list = emoji_sets_data.read();

                let mut found_in_set: Option<(String, String)> = None;
                for set in emoji_sets_list.iter() {
                    if let Some(emoji) = set.emojis.iter().find(|e| e.image_url == trimmed) {
                        found_in_set = Some((emoji.shortcode.clone(), emoji.image_url.clone()));
                        break;
                    }
                }

                if let Some((shortcode, url)) = found_in_set {
                    PreferredReaction::Custom { shortcode, url }
                } else {
                    // URL not found anywhere, skip it
                    log::warn!("Custom emoji URL not found in user's emoji stores: {}", trimmed);
                    return;
                }
            }
        } else {
            // Standard unicode emoji
            PreferredReaction::Standard { emoji: trimmed.to_string() }
        };

        // Check for duplicates
        let reactions = local_reactions.read();
        let is_duplicate = reactions.iter().any(|r| match (r, &reaction) {
            (PreferredReaction::Standard { emoji: a }, PreferredReaction::Standard { emoji: b }) => a == b,
            (PreferredReaction::Custom { shortcode: a, .. }, PreferredReaction::Custom { shortcode: b, .. }) => a == b,
            _ => false,
        });

        if !is_duplicate {
            drop(reactions);
            local_reactions.write().push(reaction);
        }
    };

    // Handle save
    let handle_save = move |_| {
        saving.set(true);
        error_msg.set(None);
        let reactions = local_reactions.read().clone();

        spawn(async move {
            match save_preferred_reactions(reactions).await {
                Ok(_) => {
                    props.on_close.call(());
                }
                Err(e) => {
                    error_msg.set(Some(e));
                    saving.set(false);
                }
            }
        });
    };

    // Handle backdrop click
    let handle_backdrop_click = move |_| {
        if !*saving.read() {
            props.on_close.call(());
        }
    };

    let reactions_count = local_reactions.read().len();
    let can_add_more = reactions_count < MAX_REACTIONS;

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4",
            onclick: handle_backdrop_click,

            // Modal content
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full p-6 max-h-[90vh] overflow-y-auto",
                onclick: |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between mb-4",
                    div {
                        class: "flex items-center gap-2",
                        SettingsIcon { class: "w-5 h-5 text-gray-500" }
                        h3 { class: "text-xl font-semibold text-gray-900 dark:text-white", "Customize Reactions" }
                    }
                    button {
                        class: "text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 text-xl font-bold",
                        onclick: move |_| props.on_close.call(()),
                        disabled: *saving.read(),
                        "×"
                    }
                }

                p {
                    class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    "Drag to reorder. First emoji is your default reaction when you click the heart."
                }

                // Draggable emoji list
                div {
                    class: "flex flex-wrap gap-2 p-3 bg-gray-100 dark:bg-gray-700 rounded-lg mb-4 min-h-[60px]",

                    for (index, reaction) in local_reactions.read().iter().cloned().enumerate() {
                        // Each emoji item is both draggable AND a drop zone
                        div {
                            key: "{reaction.display()}",
                            class: "relative group",
                            draggable: "true",

                            // Drag events
                            ondragstart: move |e| {
                                dragging_index.set(Some(index));
                                let _ = e.data_transfer().set_data("text/plain", &index.to_string());
                            },
                            ondragend: move |_| {
                                dragging_index.set(None);
                                drag_over_index.set(None);
                            },
                            ondragover: move |e| {
                                e.prevent_default();
                                drag_over_index.set(Some(index));
                            },
                            ondragleave: move |_| {
                                if drag_over_index() == Some(index) {
                                    drag_over_index.set(None);
                                }
                            },
                            ondrop: move |e| {
                                e.prevent_default();
                                if let Some(from_str) = e.data_transfer().get_data("text/plain") {
                                    if let Ok(from_idx) = from_str.parse::<usize>() {
                                        local_reactions.with_mut(|list| {
                                            if from_idx != index && from_idx < list.len() && index < list.len() {
                                                let item = list.remove(from_idx);
                                                // After remove, indices shift down. If dragging from before
                                                // the target, the target index is now one less.
                                                let insert_idx = if from_idx < index { index - 1 } else { index };
                                                let insert_idx = insert_idx.min(list.len());
                                                list.insert(insert_idx, item);
                                            }
                                        });
                                    }
                                }
                                drag_over_index.set(None);
                            },

                            // Content wrapper with drag feedback
                            div {
                                class: format!(
                                    "p-2 bg-white dark:bg-gray-600 rounded cursor-move transition-all {} {}",
                                    if dragging_index() == Some(index) { "opacity-50 scale-95" } else { "opacity-100" },
                                    if drag_over_index() == Some(index) && dragging_index() != Some(index) {
                                        "ring-2 ring-blue-500 ring-offset-2"
                                    } else { "" }
                                ),

                                // Default badge for first item
                                if index == 0 {
                                    span {
                                        class: "absolute -top-2 left-1/2 -translate-x-1/2 text-[9px] bg-blue-500 text-white px-1.5 py-0.5 rounded-full whitespace-nowrap font-medium",
                                        "DEFAULT"
                                    }
                                }

                                // Render emoji based on type
                                match &reaction {
                                    PreferredReaction::Standard { emoji } => rsx! {
                                        span { class: "text-2xl select-none", "{emoji}" }
                                    },
                                    PreferredReaction::Custom { shortcode, url } => rsx! {
                                        if url.is_empty() {
                                            span { class: "text-sm text-gray-500", ":{shortcode}:" }
                                        } else {
                                            img {
                                                class: "w-7 h-7 object-contain",
                                                src: "{url}",
                                                alt: ":{shortcode}:",
                                                loading: "lazy"
                                            }
                                        }
                                    }
                                }

                                // Remove button (visible on hover)
                                button {
                                    class: "absolute -top-1 -right-1 w-5 h-5 bg-red-500 hover:bg-red-600 text-white rounded-full text-xs opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center shadow",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        local_reactions.write().remove(index);
                                    },
                                    "×"
                                }
                            }
                        }
                    }

                    // Empty state
                    if local_reactions.read().is_empty() {
                        div {
                            class: "w-full text-center text-gray-400 py-4",
                            "No reactions added. Add some below!"
                        }
                    }
                }

                // Count indicator
                div {
                    class: "text-xs text-gray-500 dark:text-gray-400 mb-3 text-right",
                    "{reactions_count} / {MAX_REACTIONS} reactions"
                }

                // Add emoji section
                div {
                    class: "mb-4",
                    label {
                        class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                        "Add emoji:"
                    }
                    div {
                        class: "flex gap-2",

                        // Emoji picker (icon-only mode)
                        EmojiPicker {
                            on_emoji_selected: add_emoji_from_picker,
                            icon_only: true
                        }

                        // Text input
                        input {
                            class: "flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded bg-white dark:bg-gray-700 text-gray-900 dark:text-white placeholder-gray-400 focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            placeholder: "Type emoji or :shortcode:",
                            value: "{new_emoji_input}",
                            disabled: !can_add_more,
                            oninput: move |e| new_emoji_input.set(e.value()),
                            onkeypress: move |e| {
                                if e.key() == Key::Enter {
                                    add_emoji_from_input(());
                                }
                            },
                        }

                        // Add button
                        button {
                            class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded font-medium disabled:opacity-50 disabled:cursor-not-allowed",
                            disabled: !can_add_more || new_emoji_input.read().trim().is_empty(),
                            onclick: move |_| add_emoji_from_input(()),
                            "Add"
                        }
                    }

                    // Help text
                    p {
                        class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                        "Use :shortcode: format for custom emojis from your emoji list"
                    }
                }

                // Error message
                if let Some(err) = error_msg.read().as_ref() {
                    div {
                        class: "bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 rounded p-3 mb-4",
                        p { class: "text-red-600 dark:text-red-400 text-sm", "{err}" }
                    }
                }

                // Footer buttons
                div {
                    class: "flex justify-between items-center pt-2 border-t border-gray-200 dark:border-gray-700",

                    // Reset button
                    button {
                        class: "text-sm text-gray-500 hover:text-gray-700 dark:hover:text-gray-300",
                        onclick: move |_| {
                            local_reactions.set(crate::stores::reactions_store::default_reactions());
                        },
                        disabled: *saving.read(),
                        "Reset to defaults"
                    }

                    div {
                        class: "flex gap-2",
                        button {
                            class: "px-4 py-2 text-gray-600 dark:text-gray-400 hover:text-gray-800 dark:hover:text-gray-200",
                            onclick: move |_| props.on_close.call(()),
                            disabled: *saving.read(),
                            "Cancel"
                        }
                        button {
                            class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded font-medium disabled:opacity-50",
                            disabled: *saving.read() || local_reactions.read().is_empty(),
                            onclick: handle_save,
                            if *saving.read() {
                                "Saving..."
                            } else {
                                "Save"
                            }
                        }
                    }
                }
            }
        }
    }
}
