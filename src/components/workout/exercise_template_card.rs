//! Kind-33401 exercise template card. Rendered when a template appears
//! in feeds or is opened
//! directly.
use super::exercise_type_icon::ExerciseTypeIcon;
use crate::components::icons::DumbbellIcon;
use crate::components::{RichContent, SensitiveContent};
use crate::stores::nostr_client;
use crate::utils::format::format_relative_time_or;
use crate::utils::nip36;
use crate::utils::nips::nip101e::{self};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::Event as NostrEvent;

#[component]
pub fn ExerciseTemplateCard(event: NostrEvent) -> Element {
    let template = match nip101e::parse_exercise_template(&event) {
        Ok(t) => t,
        Err(_) => return rsx! {},
    };
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_metadata = author_pubkey.clone();
    let author_pubkey_for_display = author_pubkey.clone();
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let _metadata_task = use_future(move || {
        let pubkey_str = author_pubkey_for_metadata.clone();
        async move {
            match nostr_sdk::PublicKey::parse(&pubkey_str) {
                Ok(pk) => {
                    if let Some(client) = nostr_client::get_client() {
                        if let Ok(Some(metadata)) =
                            client.fetch_metadata(pk, std::time::Duration::from_secs(5)).await
                        {
                            author_metadata.set(Some(metadata));
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to parse pubkey: {}", e);
                }
            }
        }
    });
    let title = template
        .title
        .clone()
        .or_else(|| {
            if template.d_tag.is_empty() {
                None
            } else {
                Some(nip101e::slug_to_title(&template.d_tag))
            }
        })
        .unwrap_or_else(|| "Exercise Template".to_string());
    let subtitle: Vec<String> = [&template.equipment, &template.difficulty]
        .into_iter()
        .flatten()
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    let subtitle = subtitle.join(" \u{b7} ");
    let author_name = author_metadata
        .read()
        .as_ref()
        .and_then(crate::stores::profiles::display_name_or_name)
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey_for_display));
    let time_ago = format_relative_time_or(event.created_at.as_secs(), "now");
    let is_cardio = template
        .equipment
        .as_deref()
        .map(|e| e.eq_ignore_ascii_case("cardio"))
        .unwrap_or(false);
    let content_warning = nip36::get_content_warning(&event.tags);
    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition border-b border-border",
            div { class: "flex items-center gap-2 mb-3",
                span { class: "font-semibold", "{author_name}" }
                span { class: "text-muted-foreground text-sm", "\u{b7} {time_ago}" }
                span { class: "px-2 py-0.5 rounded bg-accent text-foreground text-xs ml-auto", "Exercise" }
            }
            {
                let inner = rsx! {
                    div { class: "flex items-center gap-3",
                        div { class: "w-10 h-10 rounded-full bg-primary/10 flex items-center justify-center shrink-0",
                            if is_cardio {
                                ExerciseTypeIcon { exercise_type: Some(crate::utils::nips::nip101e::ExerciseType::Running), class: "w-6 h-6 text-primary".to_string() }
                            } else {
                                DumbbellIcon { class: "w-6 h-6 text-primary".to_string() }
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "font-semibold truncate", "{title}" }
                            if !subtitle.is_empty() {
                                div { class: "text-xs text-muted-foreground truncate", "{subtitle}" }
                            }
                        }
                    }
                    if !template.content.trim().is_empty() {
                        div { class: "mt-2 text-sm break-words",
                            RichContent {
                                content: template.content.clone(),
                                tags: event.tags.iter().cloned().collect(),
                            }
                        }
                    }
                };
                if let Some(reason) = content_warning {
                    rsx! { SensitiveContent { reason, {inner} } }
                } else {
                    inner
                }
            }
        }
    }
}
