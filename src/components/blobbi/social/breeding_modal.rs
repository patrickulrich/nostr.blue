use dioxus::prelude::*;

use crate::components::blobbi::core::builders::build_breeding_event;
use crate::components::blobbi::core::types::BlobbiCompanion;

#[derive(Clone, Debug)]
struct BreedingResult {
    success: bool,
    offspring_id: String,
}

#[component]
pub fn BreedingModal(blobbi: BlobbiCompanion, on_close: EventHandler<()>) -> Element {
    let mut partner_id = use_signal(String::new);
    let mut partner_owner = use_signal(String::new);
    let mut breeding = use_signal(|| false);
    let mut result = use_signal(|| None::<BreedingResult>);

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-card border border-border rounded-2xl p-6 w-[90vw] max-w-md shadow-xl",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-bold", "🧬 Breed {blobbi.display_name()}" }
                    button {
                        class: "p-1 hover:bg-accent rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                if let Some(res) = result() {
                    div { class: "text-center py-6",
                        if res.success {
                            span { class: "text-4xl block mb-2", "🥚" }
                            p { class: "text-sm font-medium", "A new egg has been created!" }
                            p { class: "text-xs text-muted-foreground mt-1",
                                "Offspring: {res.offspring_id}"
                            }
                        } else {
                            span { class: "text-4xl block mb-2", "💔" }
                            p { class: "text-sm font-medium", "Breeding was not successful" }
                            p { class: "text-xs text-muted-foreground mt-1",
                                "Try again with a different partner"
                            }
                        }
                        button {
                            class: "mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-80 transition",
                            onclick: move |_| on_close.call(()),
                            "Done"
                        }
                    }
                } else {
                    div { class: "space-y-3",
                        p { class: "text-xs text-muted-foreground",
                            "Enter the partner Blobbi's details to attempt breeding."
                        }

                        div { class: "p-3 rounded-lg bg-muted/50",
                            div { class: "text-xs text-muted-foreground mb-1", "Your Blobbi" }
                            div { class: "text-sm font-medium",
                                "{blobbi.display_name()}"
                            }
                            div { class: "text-xs text-muted-foreground",
                                "Gen {blobbi.generation} · {blobbi.adult_type.as_deref().unwrap_or(\"unknown\")}"
                            }
                        }

                        div {
                            label { class: "text-xs text-muted-foreground block mb-1", "Partner Blobbi ID" }
                            input {
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-sm",
                                r#type: "text",
                                placeholder: "blobbi-partner-name",
                                value: "{partner_id}",
                                oninput: move |e| partner_id.set(e.value()),
                            }
                        }

                        div {
                            label { class: "text-xs text-muted-foreground block mb-1", "Partner Owner (npub)" }
                            input {
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-sm",
                                r#type: "text",
                                placeholder: "npub1...",
                                value: "{partner_owner}",
                                oninput: move |e| partner_owner.set(e.value()),
                            }
                        }

                        button {
                            class: if breeding() {
                                "w-full py-2 bg-muted text-muted-foreground rounded-lg text-sm"
                            } else if partner_id().is_empty() || partner_owner().is_empty() {
                                "w-full py-2 bg-muted text-muted-foreground rounded-lg text-sm cursor-not-allowed"
                            } else {
                                "w-full py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-80 transition"
                            },
                            disabled: breeding() || partner_id().is_empty() || partner_owner().is_empty(),
                            onclick: move |_| {
                                let blobbi_d = blobbi.d.clone();
                                let blobbi_name = blobbi.display_name().to_string();
                                let pid = partner_id();
                                let powner = partner_owner();
                                spawn(async move {
                                    breeding.set(true);
                                    let success = true;
                                    let offspring_name = pid.trim_start_matches("blobbi-").to_string();
                                    let offspring_id = format!("blobbi-baby-{offspring_name}");
                                    let content = format!("New Blobbi born from {blobbi_name} and {pid}");

                                    let event = build_breeding_event(
                                        &blobbi_d,
                                        &pid,
                                        "self",
                                        &powner,
                                        success,
                                        Some(&offspring_id),
                                        content,
                                    );

                                    match crate::stores::publish_queue::signing::sign_event_builder(event).await {
                                        Ok(signed) => {
                                            crate::stores::publish_queue::enqueue(
                                                signed,
                                                crate::stores::publish_queue::types::QueueEventType::Other("blobbi".to_string()),
                                                None,
                                                std::collections::HashMap::new(),
                                            ).await;
                                        }
                                        Err(e) => {
                                            log::error!("Breeding sign failed: {}", e);
                                            result.set(Some(BreedingResult {
                                                success: false,
                                                offspring_id,
                                            }));
                                            breeding.set(false);
                                            return;
                                        }
                                    }

                                    result.set(Some(BreedingResult {
                                        success,
                                        offspring_id,
                                    }));
                                    breeding.set(false);
                                });
                            },
                            if breeding() { "Breeding..." } else { "Start Breeding" }
                        }
                    }
                }
            }
        }
    }
}
