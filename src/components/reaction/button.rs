//! Reaction button component with emoji picker
//! Encapsulates the like button, reaction picker, and click-outside-to-close behavior
use crate::components::icons::HeartIcon;
use super::defaults_modal::ReactionDefaultsModal;
use super::picker::InlineReactionPicker;
use crate::hooks::{format_count, ReactionEmoji, ReactionState, UseReaction};
use crate::stores::reactions_store::get_default_reaction;
use dioxus::prelude::*;

const PICKER_WIDTH_PX: f64 = 280.0;
const PICKER_HEIGHT_PX: f64 = 50.0;
const PICKER_GAP_PX: f64 = 8.0;
const PICKER_EDGE_PADDING_PX: f64 = 8.0;

fn compute_picker_position(
    anchor_x: f64,
    anchor_top: f64,
    anchor_bottom: f64,
    viewport_width: f64,
    viewport_height: f64,
) -> (f64, f64, bool) {
    let max_left = (viewport_width - PICKER_WIDTH_PX - PICKER_EDGE_PADDING_PX)
        .max(PICKER_EDGE_PADDING_PX);
    let left = (anchor_x - (PICKER_WIDTH_PX / 2.0))
        .clamp(PICKER_EDGE_PADDING_PX, max_left);
    let top_candidate = anchor_top - PICKER_HEIGHT_PX - PICKER_GAP_PX;
    let (top, position_below) = if top_candidate >= PICKER_EDGE_PADDING_PX {
        (top_candidate, false)
    } else {
        let max_top = (viewport_height - PICKER_HEIGHT_PX - PICKER_EDGE_PADDING_PX)
            .max(PICKER_EDGE_PADDING_PX);
        (
            (anchor_bottom + PICKER_GAP_PX).clamp(PICKER_EDGE_PADDING_PX, max_top),
            true,
        )
    };
    (top, left, position_below)
}

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
    let user_reaction_for_effect = props.reaction.user_reaction;
    use_effect(
        use_reactive(
            &*user_reaction_for_effect.read(),
            move |_| {
                custom_emoji_failed.set(false);
            },
        ),
    );
    let button_id = use_signal(|| format!("reaction-btn-{}", uuid::Uuid::new_v4()));
    #[allow(unused_mut, unused_variables)]
    let mut picker_top = use_signal(|| 0.0);
    #[allow(unused_mut)]
    let mut picker_left = use_signal(|| 0.0);
    #[allow(unused_mut, unused_variables)]
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
        format!(
            "{} hover:bg-red-500/10 gap-1 px-2 py-1.5 rounded transition",
            base_class,
        )
    } else {
        format!("{} {}", base_class, props.button_class)
    };
    let icon_class = props.icon_class.clone();
    rsx! {
        div { class: "relative",
            button {
                id: "{button_id}",
                class: "{button_class}",
                disabled: !props.has_signer || is_pending,
                aria_label: if is_liked { "Remove reaction" } else { "Add reaction" },
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    if props.has_signer {
                        if is_liked {
                            props.reaction.react_with.call(ReactionEmoji::Unlike);
                        } else if let Some(default) = get_default_reaction() {
                            props.reaction.react_with.call(default.to_reaction_emoji());
                        } else {
                            props.reaction.toggle_like.call(());
                        }
                    }
                },
                oncontextmenu: move |e: MouseEvent| {
                    e.prevent_default();
                    e.stop_propagation();
                    if props.has_signer {
                        let current = *show_picker.peek();
                        if current {
                            show_picker.set(false);
                            return;
                        }
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
                                            let anchor_x = rect.left() + (rect.width() / 2.0);
                                            let anchor_top = rect.top();
                                            let anchor_bottom = rect.bottom();
                                            let (top, left, is_below) = compute_picker_position(
                                                anchor_x,
                                                anchor_top,
                                                anchor_bottom,
                                                viewport_width,
                                                viewport_height,
                                            );
                                            picker_top.set(top);
                                            picker_left.set(left);
                                            position_below.set(is_below);
                                            show_picker.set(true);
                                        }
                                    }
                                }
                            }
                            #[cfg(not(feature = "web"))]
                            {
                                let coords = e.client_coordinates();
                                spawn(async move {
                                    let result = document::eval(
                                        "(() => { return [window.innerWidth || 1024, window.innerHeight || 800]; })()",
                                    )
                                    .await;
                                    let (window_width, window_height) = match result {
                                        Ok(val) => {
                                            if let Some(arr) = val.as_array() {
                                                let width = arr
                                                    .first()
                                                    .and_then(|v| v.as_f64())
                                                    .unwrap_or(1024.0);
                                                let height = arr
                                                    .get(1)
                                                    .and_then(|v| v.as_f64())
                                                    .unwrap_or(800.0);
                                                (width, height)
                                            } else {
                                                (1024.0, 800.0)
                                            }
                                        }
                                        Err(_) => (1024.0, 800.0),
                                    };
                                    let (top, left, is_below) = compute_picker_position(
                                        coords.x,
                                        coords.y,
                                        coords.y,
                                        window_width,
                                        window_height,
                                    );
                                    picker_top.set(top);
                                    picker_left.set(left);
                                    position_below.set(is_below);
                                    show_picker.set(true);
                                });
                            }
                        }
                    }
                },
                match &user_reaction {
                    Some(ReactionEmoji::Custom { url, shortcode }) => {
                        let shortcode_display = format!(":{}:", shortcode);
                        rsx! {
                            if *custom_emoji_failed.read() {
                                span { class: "{icon_class} flex items-center justify-center text-xs text-gray-500",
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
                                    },
                                }
                            }
                        }
                    }
                    Some(ReactionEmoji::Standard(emoji)) => {
                        rsx! {
                            span { class: "{icon_class} flex items-center justify-center", "{emoji}" }
                        }
                    }
                    _ => {
                        rsx! {
                            HeartIcon { class: icon_class.clone(), filled: is_liked }
                        }
                    }
                }
                if like_count > 0 {
                    span { class: "{props.count_class}", {format_count(like_count)} }
                }
            }
            if *show_picker.read() {
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
                        },
                    }
                }
            }
            if *show_defaults_modal.read() {
                ReactionDefaultsModal { on_close: move |_| show_defaults_modal.set(false) }
            }
        }
    }
}
