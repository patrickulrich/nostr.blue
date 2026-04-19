use dioxus::prelude::*;
use nostr_sdk::Tags;

use crate::components::blobbi::core::builders::build_record_event;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::stores::nostr_client;
use crate::utils::nip_bb::*;

#[derive(Clone, Debug)]
struct RecordEntry {
    record_type: String,
    generation: u32,
    content: String,
    created_at: u64,
}

fn tag_str(tags: &Tags, name: &str) -> Option<String> {
    for tag in tags.iter() {
        if tag.kind().to_string() == name {
            if let Some(content) = tag.content() {
                return Some(content.to_string());
            }
        }
    }
    None
}

#[component]
pub fn RecordsModal(blobbi: BlobbiCompanion, on_close: EventHandler<()>) -> Element {
    let mut records = use_signal(Vec::<RecordEntry>::new);
    let mut loading = use_signal(|| true);

    let blobbi_id = blobbi.d.clone();
    use_future(move || {
        let blobbi_id = blobbi_id.clone();
        async move {
            if let Some(client) = nostr_client::NOSTR_CLIENT.read().as_ref() {
                let pk = crate::stores::auth_store::get_pubkey();
                let mut filter = nostr_sdk::Filter::new()
                    .kind(blobbi_record_kind())
                    .limit(50);
                if let Some(pubkey) = pk {
                    if let Ok(author) = nostr_sdk::PublicKey::from_hex(&pubkey) {
                        filter = filter.author(author);
                    }
                }
                if let Ok(events) = client.database().query(filter).await {
                    let mut parsed = Vec::new();
                    for event in events.into_iter() {
                        let rid = tag_str(&event.tags, TAG_BLOBBI_ID).unwrap_or_default();
                        if rid != blobbi_id {
                            continue;
                        }
                        let record_type = tag_str(&event.tags, TAG_RECORD_TYPE).unwrap_or_else(|| "unknown".to_string());
                        let generation = tag_str(&event.tags, TAG_GENERATION)
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                        parsed.push(RecordEntry {
                            record_type,
                            generation,
                            content: event.content.clone(),
                            created_at: event.created_at.as_secs(),
                        });
                    }
                    parsed.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                    records.set(parsed);
                }
            }
            loading.set(false);
        }
    });

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-card border border-border rounded-2xl p-6 w-[90vw] max-w-md shadow-xl max-h-[80vh] overflow-y-auto",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-bold", "📜 Records" }
                    button {
                        class: "p-1 hover:bg-accent rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                p { class: "text-xs text-muted-foreground mb-3",
                    "{blobbi.display_name()}'s life records"
                }

                div { class: "space-y-2",
                    RecordButton {
                        blobbi: blobbi.clone(),
                        label: "Birth Record".to_string(),
                        record_type: "birth".to_string(),
                        icon: "🐣".to_string(),
                    }
                    RecordButton {
                        blobbi: blobbi.clone(),
                        label: "Hatch Record".to_string(),
                        record_type: "hatched".to_string(),
                        icon: "🥚".to_string(),
                    }
                    RecordButton {
                        blobbi: blobbi.clone(),
                        label: "Evolution Record".to_string(),
                        record_type: "evolution".to_string(),
                        icon: "⬆️".to_string(),
                    }
                }

                div { class: "mt-4 space-y-2",
                    div { class: "text-xs text-muted-foreground mb-1", "History" }

                    if loading() {
                        div { class: "text-xs text-muted-foreground text-center py-4",
                            "Loading records..."
                        }
                    } else if records().is_empty() {
                        div { class: "text-xs text-muted-foreground text-center py-4",
                            "No records yet"
                        }
                    } else {
                        for record in records() {
                            div { class: "p-2 rounded-lg bg-muted/50",
                                div { class: "flex items-center gap-2",
                                    span { class: "text-xs font-medium capitalize",
                                        "{record.record_type}"
                                    }
                                    span { class: "text-[10px] text-muted-foreground",
                                        "Gen {record.generation}"
                                    }
                                }
                                if !record.content.is_empty() {
                                    p { class: "text-[10px] text-muted-foreground mt-1",
                                        "{record.content}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RecordButton(blobbi: BlobbiCompanion, label: String, record_type: String, icon: String) -> Element {
    rsx! {
        button {
            class: "w-full flex items-center gap-3 p-3 rounded-lg bg-muted/30 hover:bg-accent transition text-left",
            onclick: {
                let blobbi = blobbi.clone();
                let record_type = record_type.clone();
                move |_| {
                    let b = blobbi.clone();
                    let rt = record_type.clone();
                    spawn(async move {
                        let content = format!("{} record for {}", rt, b.display_name());
                        let extra = vec![
                            (TAG_ORIGIN, "wild".to_string()),
                            (TAG_BASE_COLOR, b.visual_traits.base_color.clone()),
                        ];
                        let event = build_record_event(&b.d, &rt, b.generation, extra, content);
                        if let Some(client) = nostr_client::NOSTR_CLIENT.read().as_ref() {
                            let _ = client.send_event_builder(event).await;
                        }
                    });
                }
            },
            span { class: "text-lg", "{icon}" }
            span { class: "text-xs font-medium", "{label}" }
        }
    }
}
