use dioxus::prelude::*;

/// A friendly loading indicator shown while the Nostr client is initializing
#[component]
pub fn ClientInitializing() -> Element {
    rsx! {
        div {
            class: "flex flex-col items-center justify-center py-20",

            // Bouncing N animation
            div {
                class: "mb-6 animate-bounce",
                div {
                    class: "w-20 h-20 flex items-center justify-center rounded-xl bg-gradient-to-br from-purple-500 to-pink-500 shadow-lg",
                    span {
                        class: "text-5xl font-bold text-white",
                        "N"
                    }
                }
            }

            // Loading text
            div {
                class: "text-center",
                h2 {
                    class: "text-xl font-semibold text-foreground mb-2",
                    "Client Initializing"
                }
                p {
                    class: "text-sm text-muted-foreground",
                    "Connecting to the Nostr network..."
                }
            }

            // Animated dots
            div {
                class: "flex gap-2 mt-6",
                div {
                    class: "w-3 h-3 rounded-full bg-purple-500",
                    style: "animation: pulse 1.5s ease-in-out 0s infinite;",
                }
                div {
                    class: "w-3 h-3 rounded-full bg-purple-500",
                    style: "animation: pulse 1.5s ease-in-out 0.2s infinite;",
                }
                div {
                    class: "w-3 h-3 rounded-full bg-purple-500",
                    style: "animation: pulse 1.5s ease-in-out 0.4s infinite;",
                }
            }
        }

        // Add custom animation keyframes
        style {
            r#"
            @keyframes pulse {{
                0%, 100% {{
                    opacity: 0.3;
                    transform: scale(0.8);
                }}
                50% {{
                    opacity: 1;
                    transform: scale(1.2);
                }}
            }}
            "#
        }
    }
}
