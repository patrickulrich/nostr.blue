//! Reputation card component for Mostro counterparty reputation.
//!
//! Phase 8: displays a star rating, total reviews count, and reputation
//! score from the kind 38384 rating event cache (`mostro::ratings::RATINGS`).
//!
//! Mounted on the trade detail page (counterparty reputation) and on the
//! take screen (before committing to a trade).

use dioxus::prelude::*;

/// Props for the reputation card.
#[derive(Props, Clone, PartialEq)]
pub struct ReputationCardProps {
    /// Hex master pubkey of the user whose reputation to display.
    pub pubkey_hex: String,
}

/// A compact reputation card showing stars, rating, and review count.
///
/// Reads from `mostro::ratings::RATINGS` (populated by kind 38384
/// subscriptions). If no rating is cached, renders a neutral "No
/// reputation data" message.
#[component]
pub fn ReputationCard(props: ReputationCardProps) -> Element {
    // Subscribe to the ratings signal so we re-render when the rating
    // arrives. Use `peek` (immutable, doesn't touch LRU order) since
    // we're reading, not promoting the entry.
    let rating = crate::stores::mostro::ratings::RATINGS
        .read()
        .peek(&props.pubkey_hex)
        .cloned();

    if let Some(r) = rating {
        let stars = crate::stores::mostro::ratings::format_stars(r.total_rating);
        let rating_display = format!("{:.1}", r.total_rating);
        let reviews = r.total_reviews;
        rsx! {
            div { class: "flex items-center gap-2 p-2 bg-accent/30 rounded-lg",
                span { class: "text-yellow-500 text-sm", "{stars}" }
                span { class: "text-sm font-medium", "{rating_display}" }
                span { class: "text-xs text-muted-foreground",
                    "({reviews} {reviews_count_word(reviews)})"
                }
            }
        }
    } else {
        rsx! {
            div { class: "flex items-center gap-2 p-2 bg-muted/30 rounded-lg",
                span { class: "text-xs text-muted-foreground", "No reputation data" }
            }
        }
    }
}

fn reviews_count_word(n: u64) -> &'static str {
    if n == 1 { "review" } else { "reviews" }
}
