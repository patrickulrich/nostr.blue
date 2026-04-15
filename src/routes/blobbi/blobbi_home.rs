use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::onboarding::onboarding_flow::OnboardingFlow;
use crate::components::blobbi::rooms::room_shell::RoomShell;
use crate::hooks::blobbi::{use_blobbi_collection, use_blobbi_decay, use_blobbi_profile};
use crate::stores::{auth_store, blobbi_store};

static CEREMONY_IN_PROGRESS: GlobalSignal<bool> = Signal::global(|| false);

pub fn ceremony_active() -> bool {
    *CEREMONY_IN_PROGRESS.read()
}

fn set_ceremony_active(val: bool) {
    *CEREMONY_IN_PROGRESS.write() = val;
}

#[component]
pub fn BlobbiHome() -> Element {
    let auth = &auth_store::AUTH_STATE;
    let is_authenticated = auth.read().is_authenticated;

    if !is_authenticated {
        return rsx! {
            div { class: "flex flex-col items-center justify-center min-h-[60vh] p-8",
                div { class: "text-6xl mb-4", "🥚" }
                h2 { class: "text-xl font-bold text-foreground mb-2",
                    "Blobbi"
                }
                p { class: "text-muted-foreground text-sm text-center",
                    "Sign in to adopt and care for your virtual Blobbi pet"
                }
            }
        };
    }

    use_blobbi_collection();
    use_blobbi_profile();
    use_blobbi_decay();

    let collection = {
        let store = blobbi_store::BLOBBI_COLLECTION.read();
        store.collection.clone()
    };
    let loading = {
        let store = blobbi_store::BLOBBI_COLLECTION.read();
        store.loading
    };
    let error = {
        let store = blobbi_store::BLOBBI_COLLECTION.read();
        store.error.clone()
    };
    let selected_d = {
        let store = blobbi_store::BLOBBI_COLLECTION.read();
        store.selected_d.clone()
    };

    // ── CASE A: Ceremony in progress ──
    // Stay mounted until ceremony calls on_complete, regardless of data changes.
    if ceremony_active() {
        return rsx! {
            div { class: "max-w-[600px] mx-auto w-full",
                OnboardingFlow {
                    on_complete: move |_| {
                        set_ceremony_active(false);
                    },
                }
            }
        };
    }

    // ── CASE B: Loading + no data ──
    if loading && collection.is_empty() {
        return rsx! {
            div { class: "flex flex-col items-center justify-center min-h-[60vh]",
                span { class: "inline-block w-8 h-8 border-2 border-current border-t-transparent rounded-full animate-spin text-muted-foreground" }
                span { class: "text-sm text-muted-foreground mt-3",
                    "Loading your Blobbis..."
                }
            }
        };
    }

    // ── CASE C: Error + no data ──
    if !loading && collection.is_empty() && error.is_some() {
        let err_msg = error.unwrap_or_default();
        return rsx! {
            div { class: "flex flex-col items-center justify-center min-h-[60vh] p-8 text-center",
                div { class: "text-4xl mb-4", "⚠️" }
                h2 { class: "text-lg font-bold text-foreground mb-2",
                    "Failed to load Blobbis"
                }
                p { class: "text-muted-foreground text-sm mb-4",
                    "{err_msg}"
                }
                button {
                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-80 transition",
                    onclick: move |_| blobbi_store::BLOBBI_COLLECTION.write().error = None,
                    "Retry"
                }
            }
        };
    }

    // ── CASE D: No pets, start ceremony ──
    if !loading && collection.is_empty() {
        set_ceremony_active(true);
        return rsx! {
            div { class: "max-w-[600px] mx-auto w-full",
                OnboardingFlow {
                    on_complete: move |_| {
                        set_ceremony_active(false);
                    },
                }
            }
        };
    }

    // ── CASE E: Has pets, show dashboard ──
    let selected_blobbi = if let Some(d) = &selected_d {
        collection.iter().find(|b| &b.d == d).cloned()
    } else {
        collection.first().cloned()
    };

    if selected_blobbi.is_some() {
        crate::components::blobbi::companion::set_companion_visible(true);
    }

    rsx! {
        div { class: "flex flex-col min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/95 backdrop-blur-sm border-b border-border",
                div { class: "max-w-[600px] mx-auto px-4 py-3 flex items-center justify-between",
                    h1 { class: "text-xl font-bold",
                        "Blobbi"
                    }
                    if collection.len() > 1 {
                        PetSelector { collection: collection.clone(), selected_d: selected_d.clone() }
                    }
                }
            }

            div { class: "flex-1 max-w-[600px] mx-auto w-full",
                if let Some(blobbi) = selected_blobbi {
                    RoomShell { blobbi }
                } else {
                    div { class: "text-center text-muted-foreground p-8",
                        "Select a Blobbi to interact with"
                    }
                }
            }
        }
    }
}

#[component]
fn PetSelector(collection: Vec<BlobbiCompanion>, selected_d: Option<String>) -> Element {
    let mut open = use_signal(|| false);

    rsx! {
        div { class: "relative",
            button {
                class: "flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-accent transition text-sm",
                onclick: move |_| open.set(!open()),
                span { class: "text-lg", "🥚" }
                span { class: "text-xs text-muted-foreground",
                    "{collection.len()}"
                }
            }
            if open() {
                div {
                    class: "absolute right-0 top-full mt-1 bg-card border border-border rounded-lg shadow-lg z-50 min-w-[180px] overflow-hidden",
                    onclick: move |e| e.stop_propagation(),
                    for blobbi in &collection {
                        button {
                            class: if selected_d.as_deref() == Some(&blobbi.d) {
                                "w-full flex items-center gap-2 px-3 py-2 bg-accent transition text-left"
                            } else {
                                "w-full flex items-center gap-2 px-3 py-2 hover:bg-accent transition text-left"
                            },
                            onclick: {
                                let d = blobbi.d.clone();
                                move |_| {
                                    blobbi_store::select_blobbi(d.clone());
                                    open.set(false);
                                }
                            },
                            span { class: "text-sm", "{blobbi.stage.label()}" }
                            span { class: "text-sm truncate",
                                "{blobbi.display_name()}"
                            }
                        }
                    }
                }
            }
        }
    }
}
