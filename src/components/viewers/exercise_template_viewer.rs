//! Exercise-template detail viewer for kind-33401 addressable events.
use crate::components::{ClientInitializing, ExerciseTemplateCard};
use crate::stores::nostr_client;
use crate::utils::nips::nip101e::KIND_EXERCISE_TEMPLATE;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::time::Duration;

async fn fetch_template_by_naddr(naddr: &str) -> std::result::Result<Option<NostrEvent>, String> {
    let coordinate = Coordinate::from_bech32(naddr)
        .or_else(|_| Coordinate::parse(naddr))
        .map_err(|e| format!("Invalid exercise-template address: {}", e))?;
    let filter = Filter::new()
        .kind(coordinate.kind)
        .author(coordinate.public_key)
        .identifier(coordinate.identifier)
        .limit(1);
    let events = nostr_client::fetch_events_aggregated_outbox(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch exercise template: {}", e))?;
    Ok(events.into_iter().next())
}

#[component]
pub fn ExerciseTemplateViewer(naddr: String) -> Element {
    let mut template_event = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    use_effect(use_reactive!(|naddr| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            error.set(None);
            match fetch_template_by_naddr(&naddr).await {
                Ok(Some(event)) => {
                    if event.kind == Kind::from(KIND_EXERCISE_TEMPLATE) {
                        template_event.set(Some(event));
                    } else {
                        error.set(Some("Event is not an exercise template".to_string()));
                    }
                    loading.set(false);
                }
                Ok(None) => {
                    error.set(Some("Exercise template not found".to_string()));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    }));

    let current = template_event.read().clone();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: crate::routes::Route::Workouts {},
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition",
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7",
                            }
                        }
                        "Back to Workouts"
                    }
                    h1 { class: "text-xl font-bold", "Exercise Template" }
                }
            }
            div { class: "max-w-2xl mx-auto",
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if *loading.read() {
                    div { class: "flex items-center justify-center py-12",
                        div { class: "flex flex-col items-center gap-3 text-muted-foreground",
                            span { class: "inline-block w-8 h-8 border-4 border-current border-t-transparent rounded-full animate-spin" }
                            "Loading exercise template..."
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "\u{26A0}\u{FE0F}" }
                        h3 { class: "text-xl font-semibold mb-2", "Error" }
                        p { class: "text-muted-foreground mb-4", "{err}" }
                        Link {
                            to: crate::routes::Route::Workouts {},
                            class: "inline-block px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Workouts"
                        }
                    }
                } else if let Some(event) = current {
                    ExerciseTemplateCard { event }
                } else {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "\u{1F3CB}\u{FE0F}" }
                        h3 { class: "text-xl font-semibold mb-2", "Exercise template not found" }
                        Link {
                            to: crate::routes::Route::Workouts {},
                            class: "inline-block mt-4 px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Workouts"
                        }
                    }
                }
            }
        }
    }
}
