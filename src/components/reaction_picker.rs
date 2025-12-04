//! Inline reaction picker for notes and posts
//! Shows user's preferred reaction emojis

use dioxus::prelude::*;
use std::collections::HashSet;
use crate::hooks::ReactionEmoji;
use crate::stores::reactions_store::{PREFERRED_REACTIONS, PreferredReaction};
use crate::components::icons::SettingsIcon;

/// Inline reaction picker that appears on hover/click
/// Shows user's preferred reactions with optional settings button
#[derive(Props, Clone, PartialEq)]
pub struct InlineReactionPickerProps {
    /// Called when a reaction is selected
    pub on_reaction: EventHandler<ReactionEmoji>,
    /// Called when settings button is clicked (opens defaults modal)
    #[props(default)]
    pub on_settings: Option<EventHandler<()>>,
}

#[component]
pub fn InlineReactionPicker(props: InlineReactionPickerProps) -> Element {
    let preferred_reactions = PREFERRED_REACTIONS.read();
    // Track failed image URLs for fallback display
    let mut failed_images: Signal<HashSet<String>> = use_signal(HashSet::new);

    rsx! {
        div {
            class: "flex items-center gap-0.5 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-full shadow-lg px-2 py-1",
            onclick: move |e| e.stop_propagation(),

            // Render preferred reactions
            for (idx, reaction) in preferred_reactions.iter().enumerate() {
                {
                    let reaction_clone = reaction.clone();
                    let reaction_for_click = reaction.clone();
                    match &reaction_clone {
                        PreferredReaction::Standard { emoji } => {
                            let emoji_str = emoji.clone();
                            rsx! {
                                button {
                                    key: "inline-std-{idx}",
                                    class: "text-lg hover:scale-125 transition-transform p-0.5",
                                    title: "{emoji_str}",
                                    onclick: move |_| {
                                        props.on_reaction.call(reaction_for_click.to_reaction_emoji());
                                    },
                                    "{emoji_str}"
                                }
                            }
                        }
                        PreferredReaction::Custom { shortcode, url } => {
                            let title_text = format!(":{shortcode}:");
                            let url_str = url.clone();
                            let url_for_error = url.clone();
                            let has_error = failed_images.read().contains(url);
                            rsx! {
                                button {
                                    key: "inline-custom-{idx}",
                                    class: "hover:scale-125 transition-transform p-0.5 flex items-center justify-center",
                                    title: "{title_text}",
                                    onclick: move |_| {
                                        props.on_reaction.call(reaction_for_click.to_reaction_emoji());
                                    },
                                    if url_str.is_empty() || has_error {
                                        span { class: "text-xs text-gray-500", "{title_text}" }
                                    } else {
                                        img {
                                            src: "{url_str}",
                                            alt: "{title_text}",
                                            class: "w-5 h-5 object-contain",
                                            loading: "lazy",
                                            onerror: move |_| {
                                                failed_images.write().insert(url_for_error.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Settings button (if callback provided)
            if let Some(on_settings) = props.on_settings.clone() {
                div {
                    class: "ml-1 pl-1 border-l border-gray-200 dark:border-gray-600",
                    button {
                        class: "p-0.5 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition text-gray-400 hover:text-gray-600 dark:hover:text-gray-300",
                        title: "Customize reactions",
                        onclick: move |_| {
                            on_settings.call(());
                        },
                        SettingsIcon { class: "w-4 h-4" }
                    }
                }
            }
        }
    }
}
