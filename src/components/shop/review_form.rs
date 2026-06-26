//! ReviewForm component - form for leaving product reviews
//!
/// Ratings follow the market-spec (Kind 31555): the primary `thumb` rating is
/// binary (0.0 = negative, 1.0 = positive) and category ratings are fractional
/// in the 0..=1 range. Category inputs are collected as 1-5 stars and mapped to
/// 0..=1 (stars / 5.0) on submit for an intuitive UI.
use crate::stores::shop_store::publish_review;
use dioxus::prelude::*;
#[derive(Props, Clone, PartialEq)]
pub struct ReviewFormProps {
    pub product_coordinate: String,
    #[props(default)]
    pub on_submitted: Option<EventHandler<()>>,
}
/// Thumb (up/down) input for the required primary rating (market-spec `thumb`).
#[component]
fn ThumbRating(value: Option<bool>, on_change: EventHandler<Option<bool>>, label: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-2",
            span { class: "text-sm text-muted-foreground w-28", "{label}" }
            div { class: "flex gap-2",
                button {
                    r#type: "button",
                    class: if matches!(value, Some(true)) {
                        "text-3xl text-emerald-500 hover:scale-110 transition"
                    } else {
                        "text-3xl text-gray-300 dark:text-gray-600 hover:text-emerald-400 hover:scale-110 transition"
                    },
                    onclick: move |_| on_change.call(Some(true)),
                    "👍"
                }
                button {
                    r#type: "button",
                    class: if matches!(value, Some(false)) {
                        "text-3xl text-rose-500 hover:scale-110 transition"
                    } else {
                        "text-3xl text-gray-300 dark:text-gray-600 hover:text-rose-400 hover:scale-110 transition"
                    },
                    onclick: move |_| on_change.call(Some(false)),
                    "👎"
                }
            }
        }
    }
}
/// Star rating input component (1-5 stars, used for optional categories; mapped to 0..=1).
#[component]
fn StarRating(value: u8, on_change: EventHandler<u8>, label: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-2",
            span { class: "text-sm text-muted-foreground w-28", "{label}" }
            div { class: "flex gap-1",
                for i in 1..=5 {
                    button {
                        r#type: "button",
                        class: if i <= value { "text-2xl text-amber-400 hover:scale-110 transition" } else { "text-2xl text-gray-300 dark:text-gray-600 hover:text-amber-300 hover:scale-110 transition" },
                        onclick: move |_| on_change.call(i),
                        "★"
                    }
                }
            }
        }
    }
}
/// Review form with a required thumb rating and optional category ratings (0..=1).
#[component]
pub fn ReviewForm(props: ReviewFormProps) -> Element {
    // Primary thumb rating: None (not chosen), Some(true) positive (1.0), Some(false) negative (0.0).
    let mut thumb = use_signal(|| None::<bool>);
    // Category ratings as 1-5 stars (0 = not provided); converted to 0..=1 on submit.
    let mut value_rating = use_signal(|| 0u8);
    let mut quality_rating = use_signal(|| 0u8);
    let mut delivery_rating = use_signal(|| 0u8);
    let mut communication_rating = use_signal(|| 0u8);
    let mut content = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut show_categories = use_signal(|| false);
    let can_submit = thumb.read().is_some() && !*submitting.read();
    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
            h3 { class: "font-semibold", "Write a Review" }
            ThumbRating {
                value: *thumb.read(),
                on_change: move |v| thumb.set(v),
                label: "Overall *".to_string(),
            }
            button {
                r#type: "button",
                class: "text-sm text-blue-500 hover:underline",
                onclick: move |_| {
                    let current = *show_categories.read();
                    show_categories.set(!current);
                },
                if *show_categories.read() {
                    "Hide category ratings"
                } else {
                    "Add category ratings (optional)"
                }
            }
            if *show_categories.read() {
                div { class: "space-y-2 pl-4 border-l-2 border-border",
                    StarRating {
                        value: *value_rating.read(),
                        on_change: move |v| value_rating.set(v),
                        label: "Value".to_string(),
                    }
                    StarRating {
                        value: *quality_rating.read(),
                        on_change: move |v| quality_rating.set(v),
                        label: "Quality".to_string(),
                    }
                    StarRating {
                        value: *delivery_rating.read(),
                        on_change: move |v| delivery_rating.set(v),
                        label: "Delivery".to_string(),
                    }
                    StarRating {
                        value: *communication_rating.read(),
                        on_change: move |v| communication_rating.set(v),
                        label: "Communication".to_string(),
                    }
                }
            }
            div {
                textarea {
                    class: "w-full h-24 px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500 resize-none text-sm",
                    placeholder: "Share your experience with this product... (optional)",
                    value: "{content}",
                    oninput: move |e| content.set(e.value()),
                }
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "text-sm text-destructive", "{err}" }
            }
            {
                let coord = props.product_coordinate.clone();
                let on_submitted = props.on_submitted;
                rsx! {
                    button {
                        r#type: "button",
                        class: "w-full py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50",
                        disabled: !can_submit,
                        onclick: move |_| {
                            submitting.set(true);
                            error.set(None);
                            let coord = coord.clone();
                            // Primary thumb rating: 1.0 positive, 0.0 negative.
                            let thumb_val = *thumb.read();
                            let overall = thumb_val.map(|t| if t { 1.0 } else { 0.0 }).unwrap_or(0.0);
                            // Category ratings: convert 1-5 stars to the 0..=1 range.
                            let to_score = |stars: u8| -> Option<f64> {
                                if stars > 0 { Some(stars as f64 / 5.0) } else { None }
                            };
                            let value = to_score(*value_rating.read());
                            let quality = to_score(*quality_rating.read());
                            let delivery = to_score(*delivery_rating.read());
                            let communication = to_score(*communication_rating.read());
                            let review_content = content.read().clone();
                            let on_submitted = on_submitted;
                            spawn(async move {
                                match publish_review(
                                        &coord,
                                        overall,
                                        review_content,
                                        value,
                                        quality,
                                        delivery,
                                        communication,
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        thumb.set(None);
                                        value_rating.set(0);
                                        quality_rating.set(0);
                                        delivery_rating.set(0);
                                        communication_rating.set(0);
                                        content.set(String::new());
                                        show_categories.set(false);
                                        if let Some(handler) = on_submitted {
                                            handler.call(());
                                        }
                                    }
                                    Err(e) => {
                                        error.set(Some(e));
                                    }
                                }
                                submitting.set(false);
                            });
                        },
                        if *submitting.read() {
                            "Submitting..."
                        } else {
                            "Submit Review"
                        }
                    }
                }
            }
        }
    }
}
