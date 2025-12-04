//! Compact reaction picker for notes and posts
//! Shows quick reaction emojis and user's custom emojis

use dioxus::prelude::*;
use std::collections::HashSet;
use crate::hooks::ReactionEmoji;
use crate::stores::emoji_store::{CUSTOM_EMOJIS, CustomEmojisStoreStoreExt};
use crate::stores::reactions_store::{PREFERRED_REACTIONS, PreferredReaction};
use crate::components::icons::SettingsIcon;

#[derive(Props, Clone, PartialEq)]
pub struct ReactionPickerProps {
    /// Called when a reaction is selected
    pub on_reaction: EventHandler<ReactionEmoji>,
    /// Called when the picker should close
    #[props(default)]
    pub on_close: Option<EventHandler<()>>,
    /// Whether to show custom emojis section
    #[props(default = true)]
    pub show_custom: bool,
}

#[component]
pub fn ReactionPicker(props: ReactionPickerProps) -> Element {
    let custom_emojis = CUSTOM_EMOJIS.read();
    let custom_emojis_data = custom_emojis.data();
    let custom_emojis_list = custom_emojis_data.read();
    let has_custom = !custom_emojis_list.is_empty();

    // Track failed image URLs for fallback display
    let mut failed_images: Signal<HashSet<String>> = use_signal(HashSet::new);

    rsx! {
        div {
            class: "bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg p-2",
            onclick: move |e| e.stop_propagation(),

            // Quick reactions row (from user's preferred reactions)
            div {
                class: "flex gap-1 mb-2",
                for (idx, reaction) in PREFERRED_REACTIONS.read().iter().enumerate() {
                    {
                        let reaction_clone = reaction.clone();
                        let reaction_for_click = reaction.clone();
                        match &reaction_clone {
                            PreferredReaction::Standard { emoji } => {
                                let emoji_str = emoji.clone();
                                rsx! {
                                    button {
                                        key: "quick-std-{idx}",
                                        class: "text-xl hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-1.5 transition",
                                        title: "{emoji_str}",
                                        onclick: move |_| {
                                            props.on_reaction.call(reaction_for_click.to_reaction_emoji());
                                            if let Some(on_close) = &props.on_close {
                                                on_close.call(());
                                            }
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
                                        key: "quick-custom-{idx}",
                                        class: "hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-1.5 transition flex items-center justify-center",
                                        title: "{title_text}",
                                        onclick: move |_| {
                                            props.on_reaction.call(reaction_for_click.to_reaction_emoji());
                                            if let Some(on_close) = &props.on_close {
                                                on_close.call(());
                                            }
                                        },
                                        if url_str.is_empty() || has_error {
                                            span { class: "text-xs text-gray-500", "{title_text}" }
                                        } else {
                                            img {
                                                src: "{url_str}",
                                                alt: "{title_text}",
                                                class: "w-6 h-6 object-contain",
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
            }

            // Custom emojis section (if user has any and show_custom is true)
            if props.show_custom && has_custom {
                div {
                    class: "border-t border-gray-200 dark:border-gray-700 pt-2",
                    p {
                        class: "text-xs text-gray-500 dark:text-gray-400 mb-1 px-1",
                        "Custom"
                    }
                    div {
                        class: "flex flex-wrap gap-1 max-h-24 overflow-y-auto",
                        // Show first 14 custom emojis
                        for (idx, custom_emoji) in custom_emojis_list.iter().take(14).enumerate() {
                            {
                                let shortcode = custom_emoji.shortcode.clone();
                                let url = custom_emoji.image_url.clone();
                                let url_for_click = url.clone();
                                let shortcode_for_click = shortcode.clone();
                                let url_for_error = url.clone();
                                let title_text = format!(":{shortcode}:");
                                let shortcode_display = format!(":{shortcode}:");
                                let has_error = failed_images.read().contains(&url);
                                rsx! {
                                    button {
                                        key: "custom-{idx}",
                                        class: "hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-1 transition flex items-center justify-center",
                                        title: "{title_text}",
                                        onclick: move |_| {
                                            props.on_reaction.call(ReactionEmoji::Custom {
                                                shortcode: shortcode_for_click.clone(),
                                                url: url_for_click.clone(),
                                            });
                                            if let Some(on_close) = &props.on_close {
                                                on_close.call(());
                                            }
                                        },
                                        if has_error {
                                            span { class: "text-xs text-gray-500 truncate max-w-[3rem]", "{shortcode_display}" }
                                        } else {
                                            img {
                                                src: "{url}",
                                                alt: "{title_text}",
                                                class: "w-6 h-6 object-contain",
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
            }
        }
    }
}

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
            if props.on_settings.is_some() {
                div {
                    class: "ml-1 pl-1 border-l border-gray-200 dark:border-gray-600",
                    button {
                        class: "p-0.5 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition text-gray-400 hover:text-gray-600 dark:hover:text-gray-300",
                        title: "Customize reactions",
                        onclick: move |_| {
                            if let Some(ref on_settings) = props.on_settings {
                                on_settings.call(());
                            }
                        },
                        SettingsIcon { class: "w-4 h-4" }
                    }
                }
            }
        }
    }
}
