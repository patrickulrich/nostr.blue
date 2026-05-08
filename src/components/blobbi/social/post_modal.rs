use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;

#[component]
pub fn BlobbiPostModal(blobbi: BlobbiCompanion, on_close: EventHandler<()>) -> Element {
    let mut text = use_signal(String::new);
    let mut publishing = use_signal(|| false);
    let mut published = use_signal(|| false);

    let name = blobbi.display_name();
    let default_hashtag = if blobbi.is_egg() || matches!(blobbi.stage, crate::utils::nip_bb::BlobbiStage::Baby) {
        format!("#blobbi #Evolving{}", name.replace(' ', ""))
    } else {
        "#blobbi".to_string()
    };

    let char_count = text().len();
    let max_chars = 500usize;

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-card border border-border rounded-2xl w-full max-w-md mx-4 shadow-2xl",
                onclick: move |e| e.stop_propagation(),

                div { class: "flex items-center justify-between p-4 border-b border-border",
                    div { class: "flex items-center gap-2",
                        span { class: "text-xl", "\u{1F4DD}" }
                        h3 { class: "text-lg font-bold", "Post about {name}" }
                    }
                    button {
                        class: "p-1.5 hover:bg-accent rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "\u{2715}"
                    }
                }

                if published() {
                    div { class: "p-6 text-center",
                        div { class: "text-4xl mb-3", "\u{2705}" }
                        p { class: "text-sm font-medium", "Posted!" }
                    }
                } else {
                    div { class: "p-4 space-y-3",
                        textarea {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-sm resize-none focus:outline-none focus:ring-1 focus:ring-blue-500",
                            rows: "4",
                            maxlength: "{max_chars}",
                            placeholder: "What's {name} up to?",
                            value: "{text}",
                            oninput: move |e| text.set(e.value()),
                        }

                        div { class: "flex items-center justify-between",
                            span { class: "text-[10px] text-muted-foreground",
                                "{char_count}/{max_chars}"
                            }
                            button {
                                class: "text-xs px-2 py-1 rounded-md bg-muted hover:bg-accent transition",
                                onclick: {
                                    let default_hashtag = default_hashtag.clone();
                                    move |_| {
                                        let current = text();
                                        if !current.contains("#blobbi") {
                                            text.set(format!("{} {}", current.trim(), default_hashtag));
                                        }
                                    }
                                },
                                "#blobbi"
                            }
                        }

                        button {
                            class: if publishing() || text().trim().is_empty() {
                                "w-full py-2.5 bg-muted text-muted-foreground text-sm font-medium rounded-lg cursor-not-allowed"
                            } else {
                                "w-full py-2.5 bg-blue-500 hover:bg-blue-600 text-white text-sm font-medium rounded-lg transition"
                            },
                            disabled: publishing() || text().trim().is_empty(),
                            onclick: {
                                let blobbi_d = blobbi.d.clone();
                                move |_| {
                                    let content = text().clone();
                                    let blobbi_d = blobbi_d.clone();
                                    publishing.set(true);
                                    spawn(async move {
                                        let mut builder = nostr_sdk::EventBuilder::new(
                                            nostr_sdk::Kind::TextNote,
                                            &content,
                                        );

                                        let hashtags: Vec<&str> = content
                                            .split_whitespace()
                                            .filter(|w| w.starts_with('#'))
                                            .collect();
                                        for tag in &hashtags {
                                            builder = builder.tag(
                                                nostr_sdk::Tag::hashtag(tag.trim_start_matches('#')),
                                            );
                                        }

                                        builder = builder.tag(
                                            nostr_sdk::Tag::custom(
                                                nostr_sdk::TagKind::Custom("blobbi".into()),
                                                vec![blobbi_d],
                                            ),
                                        );

                                        if let Ok(signed) = crate::stores::publish_queue::signing::sign_event_builder(builder).await {
                                            crate::stores::publish_queue::enqueue(
                                                signed,
                                                crate::stores::publish_queue::types::QueueEventType::Other("blobbi".to_string()),
                                                None,
                                                std::collections::HashMap::new(),
                                            ).await;
                                        }

                                        publishing.set(false);
                                        published.set(true);
                                        crate::platform::timer::sleep_ms(1200).await;
                                        on_close.call(());
                                    });
                                }
                            },
                            if publishing() { "Publishing..." } else { "Post" }
                        }
                    }
                }
            }
        }
    }
}
