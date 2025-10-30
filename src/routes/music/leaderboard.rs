use dioxus::prelude::*;
use crate::routes::Route;

#[component]
pub fn MusicLeaderboard() -> Element {
    rsx! {
        div {
            class: "max-w-4xl mx-auto p-4 space-y-6",

            // Header
            div {
                class: "flex items-center justify-between",
                h1 {
                    class: "text-3xl font-bold",
                    "🏆 Music Leaderboard"
                }
                Link {
                    to: Route::MusicHome {},
                    class: "px-4 py-2 bg-muted hover:bg-muted/80 rounded-full transition",
                    "← Back to Music"
                }
            }

            // Description
            div {
                class: "bg-card p-6 rounded-lg border border-border",
                p {
                    class: "text-muted-foreground",
                    "Weekly music voting leaderboard powered by Nostr (NIP-51)."
                }
                p {
                    class: "text-sm text-muted-foreground mt-2",
                    "Vote for your favorite tracks using the heart button. Leaderboard resets weekly."
                }
            }

            // Placeholder
            div {
                class: "text-center py-12 text-muted-foreground",
                p { "Leaderboard feature coming soon!" }
                p {
                    class: "text-sm mt-2",
                    "Vote on tracks to see them appear here"
                }
            }
        }
    }
}
