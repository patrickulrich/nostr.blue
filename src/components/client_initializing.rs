use dioxus::prelude::*;

/// A skeleton loading placeholder shown while the Nostr client is initializing or data is loading
/// Mimics the feed layout for a smoother perceived loading experience
#[component]
pub fn ClientInitializing() -> Element {
    rsx! {
        div {
            class: "animate-pulse",
            role: "status",
            aria_live: "polite",
            aria_busy: "true",

            // Screen reader announcement
            span {
                class: "sr-only",
                "Loading..."
            }

            // Skeleton for 3 feed items
            for _ in 0..3 {
                SkeletonNoteCard {}
            }
        }
    }
}

/// A single skeleton note card that matches the NoteCard layout
#[component]
fn SkeletonNoteCard() -> Element {
    rsx! {
        article {
            class: "border-b border-border p-4",

            div {
                class: "flex gap-3",

                // Avatar placeholder
                div {
                    class: "flex-shrink-0",
                    div {
                        class: "w-12 h-12 rounded-full bg-muted"
                    }
                }

                // Content area
                div {
                    class: "flex-1 min-w-0",

                    // Header row (name + timestamp)
                    div {
                        class: "flex items-center gap-2 mb-2",
                        // Display name
                        div {
                            class: "h-4 w-24 bg-muted rounded"
                        }
                        // Username
                        div {
                            class: "h-3 w-20 bg-muted rounded"
                        }
                        // Dot separator
                        div {
                            class: "h-1 w-1 bg-muted rounded-full"
                        }
                        // Timestamp
                        div {
                            class: "h-3 w-12 bg-muted rounded"
                        }
                    }

                    // Content lines (3 lines of varying length)
                    div {
                        class: "space-y-2 mb-4",
                        div {
                            class: "h-4 bg-muted rounded w-full"
                        }
                        div {
                            class: "h-4 bg-muted rounded w-4/5"
                        }
                        div {
                            class: "h-4 bg-muted rounded w-3/5"
                        }
                    }

                    // Action buttons row
                    div {
                        class: "flex items-center gap-8 mt-4",
                        // Reply
                        div {
                            class: "h-4 w-8 bg-muted rounded"
                        }
                        // Like
                        div {
                            class: "h-4 w-8 bg-muted rounded"
                        }
                        // Repost
                        div {
                            class: "h-4 w-8 bg-muted rounded"
                        }
                        // Zap
                        div {
                            class: "h-4 w-10 bg-muted rounded"
                        }
                    }
                }
            }
        }
    }
}
