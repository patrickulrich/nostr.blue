use dioxus::prelude::*;

#[component]
pub fn DVM() -> Element {
    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3",
                    h2 {
                        class: "text-xl font-bold",
                        "⚡ Data Vending Machines"
                    }
                }
            }

            // Coming soon placeholder
            div {
                class: "p-6 text-center",
                div {
                    class: "max-w-md mx-auto",
                    div {
                        class: "text-6xl mb-4",
                        "⚡"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2",
                        "Data Vending Machines"
                    }
                    p {
                        class: "text-muted-foreground text-sm",
                        "Request and provide AI services, content processing, and more through Nostr DVMs (NIP-90). Coming soon!"
                    }
                }
            }
        }
    }
}
