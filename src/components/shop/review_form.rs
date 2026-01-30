//! ReviewForm component - form for leaving product reviews

use crate::stores::shop_store::publish_review;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ReviewFormProps {
    pub product_coordinate: String,
    #[props(default)]
    pub on_submitted: Option<EventHandler<()>>,
}

/// Star rating input component
#[component]
fn StarRating(value: f64, on_change: EventHandler<f64>, label: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-2",
            span { class: "text-sm text-muted-foreground w-28", "{label}" }
            div { class: "flex gap-1",
                for i in 1..=5 {
                    button {
                        r#type: "button",
                        class: if (i as f64) <= value {
                            "text-2xl text-amber-400 hover:scale-110 transition"
                        } else {
                            "text-2xl text-gray-300 dark:text-gray-600 hover:text-amber-300 hover:scale-110 transition"
                        },
                        onclick: move |_| on_change.call(i as f64),
                        "★"
                    }
                }
            }
            span { class: "text-sm text-muted-foreground ml-2",
                "{value:.0}/5"
            }
        }
    }
}

/// Review form with star ratings
#[component]
pub fn ReviewForm(props: ReviewFormProps) -> Element {
    let mut overall_rating = use_signal(|| 0.0f64);
    let mut value_rating = use_signal(|| 0.0f64);
    let mut quality_rating = use_signal(|| 0.0f64);
    let mut delivery_rating = use_signal(|| 0.0f64);
    let mut communication_rating = use_signal(|| 0.0f64);
    let mut content = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut show_categories = use_signal(|| false);

    let can_submit = *overall_rating.read() > 0.0 && !*submitting.read();

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
            h3 { class: "font-semibold", "Write a Review" }

            // Overall rating (required)
            StarRating {
                value: *overall_rating.read(),
                on_change: move |v| overall_rating.set(v),
                label: "Overall *".to_string()
            }

            // Toggle for category ratings
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

            // Category ratings (optional)
            if *show_categories.read() {
                div { class: "space-y-2 pl-4 border-l-2 border-border",
                    StarRating {
                        value: *value_rating.read(),
                        on_change: move |v| value_rating.set(v),
                        label: "Value".to_string()
                    }
                    StarRating {
                        value: *quality_rating.read(),
                        on_change: move |v| quality_rating.set(v),
                        label: "Quality".to_string()
                    }
                    StarRating {
                        value: *delivery_rating.read(),
                        on_change: move |v| delivery_rating.set(v),
                        label: "Delivery".to_string()
                    }
                    StarRating {
                        value: *communication_rating.read(),
                        on_change: move |v| communication_rating.set(v),
                        label: "Communication".to_string()
                    }
                }
            }

            // Review text
            div {
                textarea {
                    class: "w-full h-24 px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500 resize-none text-sm",
                    placeholder: "Share your experience with this product... (optional)",
                    value: "{content}",
                    oninput: move |e| content.set(e.value())
                }
            }

            // Error message
            if let Some(err) = error.read().as_ref() {
                div { class: "text-sm text-destructive", "{err}" }
            }

            // Submit button
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
                            let overall = *overall_rating.read();
                            let value = if *value_rating.read() > 0.0 { Some(*value_rating.read()) } else { None };
                            let quality = if *quality_rating.read() > 0.0 { Some(*quality_rating.read()) } else { None };
                            let delivery = if *delivery_rating.read() > 0.0 { Some(*delivery_rating.read()) } else { None };
                            let communication = if *communication_rating.read() > 0.0 { Some(*communication_rating.read()) } else { None };
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
                                ).await {
                                    Ok(_) => {
                                        // Reset form
                                        overall_rating.set(0.0);
                                        value_rating.set(0.0);
                                        quality_rating.set(0.0);
                                        delivery_rating.set(0.0);
                                        communication_rating.set(0.0);
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
