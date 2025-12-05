//! Reaction button component with emoji picker
//! Encapsulates the like button, reaction picker, and click-outside-to-close behavior

use dioxus::prelude::*;
use crate::hooks::{UseReaction, ReactionState, ReactionEmoji, format_count};
use crate::components::InlineReactionPicker;
use crate::components::ReactionDefaultsModal;
use crate::components::icons::HeartIcon;
use crate::stores::reactions_store::get_default_reaction;

#[derive(Props, Clone, PartialEq)]
pub struct ReactionButtonProps {
    /// The reaction hook instance from use_reaction()
    pub reaction: UseReaction,
    /// Whether a signer is available
    pub has_signer: bool,
    /// Icon size class (e.g., "h-4 w-4", "w-5 h-5", "w-6 h-6")
    #[props(default = "h-4 w-4".to_string())]
    pub icon_class: String,
    /// Additional button classes
    #[props(default = String::new())]
    pub button_class: String,
    /// Text size class for count
    #[props(default = "text-xs".to_string())]
    pub count_class: String,
}

#[component]
pub fn ReactionButton(props: ReactionButtonProps) -> Element {
    let mut show_picker = use_signal(|| false);
    let mut show_defaults_modal = use_signal(|| false);
    let mut custom_emoji_failed = use_signal(|| false);

    // Reset custom emoji failed state when reaction changes
    let user_reaction_for_effect = props.reaction.user_reaction.clone();
    use_effect(use_reactive(&*user_reaction_for_effect.read(), move |_| {
        custom_emoji_failed.set(false);
    }));

    // Viewport-aware positioning signals
    let button_id = use_signal(|| format!("reaction-btn-{}", uuid::Uuid::new_v4()));
    let mut picker_top = use_signal(|| 0.0);
    let mut picker_left = use_signal(|| 0.0);
    let mut position_below = use_signal(|| false);

    let is_liked = *props.reaction.is_liked.read();
    let like_count = *props.reaction.like_count.read();
    let is_pending = matches!(*props.reaction.state.read(), ReactionState::Pending);
    let user_reaction = props.reaction.user_reaction.read().clone();

    let base_class = if is_liked {
        "flex items-center text-red-500"
    } else {
        "flex items-center text-muted-foreground hover:text-red-500"
    };

    let button_class = if props.button_class.is_empty() {
        format!("{} hover:bg-red-500/10 gap-1 px-2 py-1.5 rounded transition", base_class)
    } else {
        format!("{} {}", base_class, props.button_class)
    };

    // Determine what to display based on user's reaction
    let icon_class = props.icon_class.clone();

    rsx! {
        div {
            class: "relative",

            // Like button - click for quick like, right-click for reaction picker
            button {
                id: "{button_id}",
                class: "{button_class}",
                disabled: !props.has_signer || is_pending,
                aria_label: if is_liked { "Remove reaction" } else { "Add reaction" },
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    if props.has_signer {
                        // Use user's default reaction instead of simple toggle
                        if is_liked {
                            // Already liked - unlike it
                            props.reaction.react_with.call(ReactionEmoji::Unlike);
                        } else if let Some(default) = get_default_reaction() {
                            // Use user's preferred default reaction
                            props.reaction.react_with.call(default.to_reaction_emoji());
                        } else {
                            // Fallback to standard like
                            props.reaction.toggle_like.call(());
                        }
                    }
                },
                // Right-click to show reaction picker
                oncontextmenu: move |e: MouseEvent| {
                    e.prevent_default();
                    e.stop_propagation();
                    if props.has_signer {
                        let current = *show_picker.peek();
                        if !current {
                            // Calculate viewport-aware position when opening
                            #[cfg(target_family = "wasm")]
                            {
                                let btn_id = button_id.read().clone();
                                if let Some(window) = web_sys::window() {
                                    if let Some(document) = window.document() {
                                        if let Some(element) = document.get_element_by_id(&btn_id) {
                                            let rect = element.get_bounding_client_rect();
                                            let viewport_height = window.inner_height()
                                                .ok().and_then(|h| h.as_f64()).unwrap_or(800.0);
                                            let picker_height = 50.0; // Approximate picker height

                                            // Check if button is in top half of viewport
                                            let button_center_y = rect.top() + (rect.height() / 2.0);
                                            if button_center_y < (viewport_height / 2.0) {
                                                // Position below button
                                                picker_top.set(rect.bottom() + 8.0);
                                                position_below.set(true);
                                            } else {
                                                // Position above button
                                                picker_top.set(rect.top() - picker_height - 8.0);
                                                position_below.set(false);
                                            }
                                            picker_left.set(rect.left());
                                        }
                                    }
                                }
                            }
                        }
                        show_picker.set(!current);
                    }
                },
                // Display emoji based on user's reaction
                match &user_reaction {
                    Some(ReactionEmoji::Custom { url, shortcode }) => {
                        let shortcode_display = format!(":{}:", shortcode);
                        rsx! {
                            if *custom_emoji_failed.read() {
                                span {
                                    class: "{icon_class} flex items-center justify-center text-xs text-gray-500",
                                    "{shortcode_display}"
                                }
                            } else {
                                img {
                                    class: "{icon_class} object-contain",
                                    src: "{url}",
                                    alt: ":{shortcode}:",
                                    loading: "lazy",
                                    onerror: move |_| {
                                        custom_emoji_failed.set(true);
                                    }
                                }
                            }
                        }
                    }
                    Some(ReactionEmoji::Standard(emoji)) => {
                        rsx! {
                            span {
                                class: "{icon_class} flex items-center justify-center",
                                "{emoji}"
                            }
                        }
                    }
                    // Like (+) or no reaction - show heart
                    _ => {
                        rsx! {
                            HeartIcon {
                                class: icon_class.clone(),
                                filled: is_liked
                            }
                        }
                    }
                }
                if like_count > 0 {
                    span {
                        class: "{props.count_class}",
                        { format_count(like_count) }
                    }
                }
            }

            // Reaction picker dropdown with backdrop
            if *show_picker.read() {
                // Invisible backdrop to catch outside clicks
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        show_picker.set(false);
                    },
                }
                div {
                    class: "fixed z-50",
                    style: format!("top: {}px; left: {}px;", *picker_top.read(), *picker_left.read()),
                    InlineReactionPicker {
                        on_reaction: move |emoji: ReactionEmoji| {
                            props.reaction.react_with.call(emoji);
                            show_picker.set(false);
                        },
                        on_settings: move |_| {
                            show_picker.set(false);
                            show_defaults_modal.set(true);
                        }
                    }
                }
            }

            // Reaction defaults modal
            if *show_defaults_modal.read() {
                ReactionDefaultsModal {
                    on_close: move |_| show_defaults_modal.set(false)
                }
            }
        }
    }
}
