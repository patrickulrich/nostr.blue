use dioxus::prelude::*;

use crate::routes::Route;

#[component]
pub fn GamesHub() -> Element {
    rsx! {
        div { class: "max-w-2xl mx-auto px-4 py-6 space-y-6",
            div { class: "flex items-center gap-3 mb-4",
                h1 { class: "text-2xl font-bold text-foreground", "Games" }
            }
            p { class: "text-muted-foreground text-sm",
                "Play games on Nostr. Challenge friends or join public games."
            }

            div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4 mt-6",
                Link {
                    to: Route::ChessHome {},
                    class: "block rounded-2xl border border-border bg-card p-6 shadow-sm hover:bg-accent/5 transition",
                    div { class: "flex items-center gap-4",
                        div { class: "w-12 h-12 rounded-xl bg-accent/10 flex items-center justify-center",
                            span { class: "text-2xl", "♟" }
                        }
                        div { class: "space-y-1",
                            h3 { class: "text-lg font-semibold text-foreground", "Chess" }
                            p { class: "text-sm text-muted-foreground",
                                "Play chess using the Jester protocol"
                            }
                        }
                    }
                }

                div { class: "rounded-2xl border border-border/50 bg-card/50 p-6 opacity-50",
                    div { class: "flex items-center gap-4",
                        div { class: "w-12 h-12 rounded-xl bg-muted flex items-center justify-center",
                            span { class: "text-2xl", "🎮" }
                        }
                        div { class: "space-y-1",
                            h3 { class: "text-lg font-semibold text-muted-foreground", "More Games" }
                            p { class: "text-sm text-muted-foreground",
                                "Coming soon..."
                            }
                        }
                    }
                }
            }
        }
    }
}
