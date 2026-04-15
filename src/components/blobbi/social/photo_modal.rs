use dioxus::prelude::*;

use crate::components::blobbi::core::builders::build_record_event;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::blobbi_visual::BlobbiVisual;

#[component]
pub fn PhotoModal(blobbi: BlobbiCompanion, on_close: EventHandler<()>) -> Element {
    let mut captured = use_signal(|| false);
    let mut caption = use_signal(String::new);

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-card border border-border rounded-2xl p-6 w-[90vw] max-w-md shadow-xl",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-bold", "📸 Photo" }
                    button {
                        class: "p-1 hover:bg-accent rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                if !captured() {
                    div { class: "text-center space-y-4",
                        div { class: "flex justify-center",
                            div { class: "relative",
                                BlobbiVisual { blobbi: blobbi.clone(), size: Some("200".to_string()) }
                                div { class: "absolute inset-0 border-4 border-dashed border-muted-foreground/30 rounded-lg pointer-events-none" }
                            }
                        }

                        p { class: "text-xs text-muted-foreground",
                            "Frame your Blobbi for the perfect shot"
                        }

                        button {
                            class: "px-6 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-80 transition",
                            onclick: move |_| captured.set(true),
                            "📸 Take Photo"
                        }
                    }
                } else {
                    div { class: "text-center space-y-4",
                        div { class: "relative inline-block",
                            BlobbiVisual { blobbi: blobbi.clone(), size: Some("200".to_string()) }
                            div { class: "absolute -bottom-1 -right-1 w-8 h-8 bg-card rounded-full border-2 border-border flex items-center justify-center",
                                span { class: "text-sm", "✓" }
                            }
                        }

                        div {
                            label { class: "text-xs text-muted-foreground block mb-1", "Caption" }
                            input {
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-sm",
                                r#type: "text",
                                placeholder: "Write a caption...",
                                value: "{caption}",
                                oninput: move |e| caption.set(e.value()),
                            }
                        }

                        div { class: "flex gap-2",
                            button {
                                class: "flex-1 py-2 bg-muted rounded-lg text-sm hover:bg-accent transition",
                                onclick: move |_| captured.set(false),
                                "Retake"
                            }
                            button {
                                class: "flex-1 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-80 transition",
                                onclick: {
                                    let blobbi = blobbi.clone();
                                    move |_| {
                                        let b = blobbi.clone();
                                        let cap = caption();
                                        spawn(async move {
                                            let content = if cap.is_empty() {
                                                format!("Photo of {}", b.display_name())
                                            } else {
                                                cap
                                            };
                                            let event = build_record_event(
                                                &b.d,
                                                "memory",
                                                b.generation,
                                                vec![],
                                                content,
                                            );
                                            if let Some(client) = crate::stores::nostr_client::NOSTR_CLIENT.read().as_ref() {
                                                let _ = client.send_event_builder(event).await;
                                            }
                                        });
                                        on_close.call(());
                                    }
                                },
                                "Share"
                            }
                        }
                    }
                }
            }
        }
    }
}
