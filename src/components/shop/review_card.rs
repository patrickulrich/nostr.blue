//! ReviewCard component - displays a product review

use dioxus::prelude::*;
use crate::utils::nip99::ProductReview;

#[derive(Props, Clone, PartialEq)]
pub struct ReviewCardProps {
    pub review: ProductReview,
}

/// Review card with rating stars
#[component]
pub fn ReviewCard(props: ReviewCardProps) -> Element {
    let review = &props.review;

    // Calculate average rating from all categories using as_stars()
    let avg_rating = review.as_stars();

    let full_stars = avg_rating.floor() as usize;
    let has_half_star = (avg_rating - full_stars as f64) >= 0.5;

    // Format date
    let date_str = {
        let secs = review.created_at as i64;
        chrono::DateTime::from_timestamp(secs, 0)
            .map(|d| d.format("%b %d, %Y").to_string())
            .unwrap_or_else(|| "Unknown".to_string())
    };

    // Truncate reviewer pubkey for display (safe UTF-8 handling)
    let reviewer_short = crate::utils::format::truncate_pubkey(&review.reviewer_pubkey);

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4",
            // Header with avatar and rating
            div { class: "flex items-start gap-3 mb-3",
                // Simple avatar placeholder
                div { class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center text-muted-foreground text-sm",
                    "👤"
                }

                div { class: "flex-1",
                    // Reviewer name
                    p { class: "font-medium",
                        "{reviewer_short}"
                    }

                    // Rating stars and date
                    div { class: "flex items-center gap-2",
                        // Stars
                        div { class: "flex items-center",
                            for i in 0..5 {
                                span {
                                    key: "{i}",
                                    class: if i < full_stars {
                                        "text-amber-400"
                                    } else if i == full_stars && has_half_star {
                                        "text-amber-400/50"
                                    } else {
                                        "text-gray-300 dark:text-gray-600"
                                    },
                                    "★"
                                }
                            }
                        }
                        span { class: "text-xs text-muted-foreground",
                            "{date_str}"
                        }
                    }
                }
            }

            // Review content
            if !review.content.is_empty() {
                p { class: "text-sm whitespace-pre-wrap",
                    "{review.content}"
                }
            }

            // Individual category ratings (if available)
            div { class: "flex flex-wrap gap-2 mt-3",
                if let Some(value) = review.value_rating {
                    span { class: "text-xs bg-muted px-2 py-1 rounded",
                        "Value: {value:.1}/5"
                    }
                }
                if let Some(quality) = review.quality_rating {
                    span { class: "text-xs bg-muted px-2 py-1 rounded",
                        "Quality: {quality:.1}/5"
                    }
                }
                if let Some(delivery) = review.delivery_rating {
                    span { class: "text-xs bg-muted px-2 py-1 rounded",
                        "Delivery: {delivery:.1}/5"
                    }
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
