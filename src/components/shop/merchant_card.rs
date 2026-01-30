//! MerchantCard component - displays merchant info on product page

use crate::routes::Route;
use crate::utils::format::truncate_pubkey;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct MerchantCardProps {
    pub pubkey: String,
    #[props(default)]
    pub name: Option<String>,
    #[props(default)]
    pub product_count: Option<usize>,
    #[props(default)]
    pub review_count: Option<usize>,
    #[props(default)]
    pub avg_rating: Option<f32>,
}

/// Merchant info card
#[component]
pub fn MerchantCard(props: MerchantCardProps) -> Element {
    // Use safe truncation to avoid panic on short/non-ASCII strings
    let display_name = props
        .name
        .clone()
        .unwrap_or_else(|| truncate_pubkey(&props.pubkey));

    rsx! {
        Link {
            to: Route::Profile { pubkey: props.pubkey.clone() },
            class: "block bg-card border border-border rounded-lg p-4 hover:border-ring transition",

            div { class: "flex items-center gap-3",
                // Simple avatar placeholder
                div { class: "w-12 h-12 rounded-full bg-muted flex items-center justify-center text-muted-foreground text-lg",
                    "🏪"
                }

                div { class: "flex-1 min-w-0",
                    h4 { class: "font-medium truncate", "{display_name}" }

                    // Stats row
                    div { class: "flex items-center gap-3 text-xs text-muted-foreground mt-1",
                        if let Some(count) = props.product_count {
                            span { "{count} products" }
                        }

                        if let Some(rating) = props.avg_rating {
                            span { class: "flex items-center gap-1",
                                span { class: "text-amber-400", "★" }
                                "{rating:.1}"
                                if let Some(reviews) = props.review_count {
                                    span { " ({reviews})" }
                                }
                            }
                        }
                    }
                }

                // Arrow
                span { class: "text-muted-foreground", "→" }
            }
        }
    }
}
