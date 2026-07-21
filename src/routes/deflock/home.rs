use crate::routes::Route;
use crate::services::deflock;
use crate::stores::deflock_store;
use dioxus::prelude::*;

#[component]
pub fn DeflockHome() -> Element {
    use_effect(move || {
        if deflock_store::WORLDWIDE_COUNT.read().is_some() {
            return;
        }
        spawn(async move {
            match deflock::fetch_camera_count().await {
                Ok(count) => {
                    *deflock_store::WORLDWIDE_COUNT.write() = Some(count);
                }
                Err(e) => {
                    log::warn!("Deflock: failed to fetch camera count: {}", e);
                }
            }
        });
    });

    let worldwide_count = *deflock_store::WORLDWIDE_COUNT.read();
    let loaded_count = deflock_store::CAMERAS.read().len();

    rsx! {
        div { class: "min-h-screen pb-safe-bottom",
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center justify-between px-4 py-3",
                    h1 { class: "text-xl font-bold flex items-center gap-2",
                        span { class: "text-red-400", "📷" }
                        "DeFlock"
                    }
                    a {
                        href: "https://deflock.org",
                        target: "_blank",
                        rel: "noopener",
                        class: "text-xs text-muted-foreground hover:text-foreground",
                        "Powered by OpenStreetMap"
                    }
                }
            }

            div { class: "px-4 py-12 text-center max-w-2xl mx-auto",
                div { class: "text-6xl mb-4", "📷" }
                h2 { class: "text-2xl font-bold mb-3", "ALPR Camera Map" }
                p { class: "text-muted-foreground mb-6",
                    "Discover Automatic License Plate Reader (ALPR) surveillance cameras near you. Data sourced directly from OpenStreetMap."
                }

                div { class: "flex justify-center gap-8 mb-8",
                    div { class: "text-center",
                        div { class: "text-2xl font-bold text-red-400",
                            if let Some(count) = worldwide_count {
                                "{count}"
                            } else {
                                "—"
                            }
                        }
                        div { class: "text-xs text-muted-foreground", "Mapped Worldwide" }
                    }
                    div { class: "text-center",
                        div { class: "text-2xl font-bold text-blue-400", "{loaded_count}" }
                        div { class: "text-xs text-muted-foreground", "Loaded Locally" }
                    }
                }

                Link {
                    to: Route::DeflockMap {},
                    class: "inline-flex items-center gap-2 px-8 py-3 bg-red-500 hover:bg-red-600 text-white rounded-xl font-medium transition text-lg",
                    "Open Camera Map"
                }
            }

            div { class: "px-4 py-8 max-w-2xl mx-auto space-y-4",
                div { class: "bg-card border border-border rounded-xl p-4",
                    h3 { class: "font-semibold mb-2", "What are ALPR cameras?" }
                    p { class: "text-sm text-muted-foreground",
                        "Automated License Plate Readers are cameras that capture and store license plate data, including location, date, and time. They are deployed by law enforcement, HOAs, and private entities."
                    }
                }

                div { class: "bg-card border border-border rounded-xl p-4",
                    h3 { class: "font-semibold mb-2", "Data Source" }
                    p { class: "text-sm text-muted-foreground",
                        "All camera locations are sourced from "
                    }
                    a {
                        href: "https://www.openstreetmap.org",
                        target: "_blank",
                        rel: "noopener",
                        class: "text-blue-500 hover:text-blue-400 text-sm",
                        "OpenStreetMap"
                    }
                    p { class: "text-sm text-muted-foreground mt-1",
                        "using the tag: man_made=surveillance, surveillance:type=ALPR"
                    }
                }

                div { class: "bg-card border border-border rounded-xl p-4",
                    h3 { class: "font-semibold mb-2", "Report a Camera" }
                    p { class: "text-sm text-muted-foreground mb-2",
                        "Help improve the map by adding cameras to OpenStreetMap:"
                    }
                    a {
                        href: "https://deflock.org/report",
                        target: "_blank",
                        rel: "noopener",
                        class: "text-blue-500 hover:text-blue-400 text-sm",
                        "Submit via DeFlock →"
                    }
                }
            }

            div { class: "px-4 py-6 text-center",
                a {
                    href: "https://github.com/FoggedLens/deflock",
                    target: "_blank",
                    rel: "noopener",
                    class: "text-xs text-muted-foreground hover:text-foreground",
                    "DeFlock on GitHub"
                }
            }
        }
    }
}
