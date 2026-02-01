//! ReviewCard component - displays a product review
use crate::utils::nip99::ProductReview;
use dioxus::prelude::*;
#[derive(Props, Clone, PartialEq)]
pub struct ReviewCardProps {
    pub review: ProductReview,
}
/// Review card with rating stars
#[component]
pub fn ReviewCard(props: ReviewCardProps) -> Element {
    let review = &props.review;
    let avg_rating = review.as_stars();
    let full_stars = avg_rating.floor() as usize;
    let has_half_star = (avg_rating - full_stars as f64) >= 0.5;
    let date_str = {
        let secs = review.created_at as i64;
        chrono::DateTime::from_timestamp(secs, 0)
            .map(|d| d.format("%b %d, %Y").to_string())
            .unwrap_or_else(|| "Unknown".to_string())
    };
    let reviewer_short = crate::utils::format::truncate_pubkey(&review.reviewer_pubkey);
    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4",
            div { class: "flex items-start gap-3 mb-3",
                div { class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center text-muted-foreground text-sm",
                    "👤"
                }
                div { class: "flex-1",
                    p { class: "font-medium", "{reviewer_short}" }
                    div { class: "flex items-center gap-2",
                        div { class: "flex items-center",
                            for i in 0..5 {
                                span {
                                    key: "{i}",
                                    class: if i < full_stars { "text-amber-400" } else if i == full_stars && has_half_star { "text-amber-400/50" } else { "text-gray-300 dark:text-gray-600" },
                                    "★"
                                }
                            }
                        }
                        span { class: "text-xs text-muted-foreground", "{date_str}" }
                    }
                }
            }
            if !review.content.is_empty() {
                p { class: "text-sm whitespace-pre-wrap", "{review.content}" }
            }
            div { class: "flex flex-wrap gap-2 mt-3",
                if let Some(value) = review.value_rating {
                    span { class: "text-xs bg-muted px-2 py-1 rounded", "Value: {value:.1}/5" }
                }
                if let Some(quality) = review.quality_rating {
                    span { class: "text-xs bg-muted px-2 py-1 rounded", "Quality: {quality:.1}/5" }
                }
                if let Some(delivery) = review.delivery_rating {
                    span { class: "text-xs bg-muted px-2 py-1 rounded", "Delivery: {delivery:.1}/5" }
                }
                if let Some(communication) = review.communication_rating {
                    span { class: "text-xs bg-muted px-2 py-1 rounded",
                        "Communication: {communication:.1}/5"
                    }
                }
            }
        }
    }
}
