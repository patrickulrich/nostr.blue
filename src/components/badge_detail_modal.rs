//! Badge Detail Modal Component
//!
//! Displays full badge information with accept/reject actions.
use crate::routes::Route;
use crate::stores::nostr_client::get_client;
use crate::stores::profiles;
use crate::utils::nip58::{BadgeAward, BadgeDefinition};
use crate::utils::time::format_relative_time;
use crate::utils::truncate_pubkey;
use crate::utils::validation::is_valid_http_url;
use dioxus::dioxus_core::Task;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Processing state for accept/decline buttons
#[derive(Clone, Copy, PartialEq, Default)]
enum ProcessingState {
    #[default]
    Idle,
    Accepting,
    Declining,
    Removing,
}
/// Badge detail modal component
#[component]
pub fn BadgeDetailModal(
    badge: BadgeDefinition,
    award: Option<BadgeAward>,
    is_own_badge: bool,
    is_accepted: bool,
    on_close: EventHandler<()>,
    on_accept: EventHandler<()>,
    on_reject: EventHandler<()>,
) -> Element {
    let mut processing_state = use_signal(ProcessingState::default);
    let mut timeout_task: Signal<Option<Task>> = use_signal(|| None);
    use_effect(use_reactive!(|is_accepted| {
        processing_state.set(ProcessingState::Idle);
        let _ = is_accepted;
    }));
    use_effect(move || {
        let current_state = *processing_state.read();

        // Only spawn timer when entering non-Idle state (avoids unnecessary work)
        if current_state != ProcessingState::Idle {
            // Cancel any existing timer
            if let Some(task) = timeout_task.write().take() {
                task.cancel();
            }
            // Spawn new timer
            let task = spawn(async move {
                crate::platform::timer::sleep_ms(10_000).await;
                // Use peek() in async to avoid subscription
                if *processing_state.peek() != ProcessingState::Idle {
                    log::warn!("Processing state timed out, resetting to Idle");
                    processing_state.set(ProcessingState::Idle);
                }
                timeout_task.set(None);
            });
            timeout_task.set(Some(task));
        }
    });
    let mut issuer_profile = use_signal(|| None::<nostr_sdk::Metadata>);
    let badge_pubkey = badge.pubkey.clone();
    let mut fetch_task: Signal<Option<Task>> = use_signal(|| None);
    let mut target_pubkey: Signal<String> = use_signal(|| badge_pubkey.clone());
    let mut gen_counter = use_signal(|| 0u32);
    use_effect(use_reactive!(|badge_pubkey| {
        if let Some(existing_task) = fetch_task.write().take() {
            existing_task.cancel();
        }
        let current_gen = gen_counter.peek().wrapping_add(1);
        gen_counter.set(current_gen);
        target_pubkey.set(badge_pubkey.clone());
        issuer_profile.set(None);
        if let Some(profile) = profiles::get_profile(&badge_pubkey) {
            issuer_profile.set(Some(profile));
        } else {
            let pubkey_str = badge_pubkey.clone();
            let new_task = spawn(async move {
                let pubkey = match PublicKey::from_hex(&pubkey_str)
                    .or_else(|_| PublicKey::from_bech32(&pubkey_str))
                {
                    Ok(pk) => pk,
                    Err(e) => {
                        log::warn!("Invalid issuer pubkey: {}", e);
                        return;
                    }
                };
                let client = match get_client() {
                    Some(c) => c,
                    None => {
                        log::error!("Client not initialized, cannot fetch issuer metadata");
                        return;
                    }
                };
                match client.database().metadata(pubkey).await {
                    Ok(Some(metadata)) => {
                        // Use peek() to avoid subscription in async context
                        if *target_pubkey.peek() == pubkey_str && *gen_counter.peek() == current_gen
                        {
                            issuer_profile.set(Some(metadata));
                        }
                        return;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::warn!("Database fetch failed for {}: {}", pubkey_str, e);
                    }
                }
                // Staleness check before network fetch
                if *target_pubkey.peek() != pubkey_str {
                    log::debug!("Pubkey changed during DB lookup, skipping network fetch");
                    return;
                }
                match client.fetch_metadata(pubkey, Duration::from_secs(5)).await {
                    Ok(Some(metadata)) => {
                        // Use peek() to avoid subscription in async context
                        if *target_pubkey.peek() == pubkey_str && *gen_counter.peek() == current_gen
                        {
                            issuer_profile.set(Some(metadata));
                        }
                    }
                    Ok(None) => {
                        log::debug!("No metadata found for {}", pubkey_str);
                    }
                    Err(e) => {
                        log::warn!("Network fetch failed for {}: {}", pubkey_str, e);
                    }
                }
            });
            fetch_task.set(Some(new_task));
        }
    }));
    let issuer_name = issuer_profile
        .read()
        .as_ref()
        .and_then(crate::stores::profiles::display_name_or_name)
        .unwrap_or_else(|| truncate_pubkey(&badge.pubkey));
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card border border-border rounded-xl max-w-md w-full shadow-xl overflow-hidden",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "badge-modal-title",
                onclick: move |e| e.stop_propagation(),
                div { class: "relative bg-gradient-to-br from-primary/20 to-accent/20 p-8 flex items-center justify-center",
                    button {
                        class: "absolute top-2 right-2 p-2 rounded-full hover:bg-black/20 transition",
                        aria_label: "Close",
                        onclick: move |_| on_close.call(()),
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M6 18L18 6M6 6l12 12",
                            }
                        }
                    }
                    {
                        let valid_image = badge.get_image().filter(|url| is_valid_http_url(url));
                        if let Some(image) = valid_image {
                            rsx! {
                                img {
                                    src: "{image}",
                                    alt: "{badge.get_display_name()}",
                                    class: "w-32 h-32 rounded-lg object-contain",
                                }
                            }
                        } else {
                            rsx! {
                                div { class: "w-32 h-32 rounded-lg bg-primary/30 flex items-center justify-center",
                                    span { class: "text-4xl font-bold text-primary",
                                        "{badge.id.chars().next().unwrap_or('?').to_ascii_uppercase()}"
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "p-6 space-y-4",
                    h2 {
                        id: "badge-modal-title",
                        class: "text-xl font-bold text-center",
                        "{badge.get_display_name()}"
                    }
                    if let Some(description) = &badge.description {
                        p { class: "text-muted-foreground text-center text-sm", "{description}" }
                    }
                    div { class: "border-t border-border my-4" }
                    div { class: "flex items-center justify-between text-sm",
                        span { class: "text-muted-foreground", "Issued by" }
                        Link {
                            to: Route::AddressViewer {
                                address: crate::utils::nip19_urls::profile_route_id(&badge.pubkey),
                            },
                            class: "text-primary hover:underline font-medium",
                            onclick: move |_| on_close.call(()),
                            "@{issuer_name}"
                        }
                    }
                    if let Some(award) = &award {
                        div { class: "flex items-center justify-between text-sm",
                            span { class: "text-muted-foreground", "Awarded" }
                            span { class: "text-foreground",
                                "{format_relative_time(Timestamp::from(award.created_at))}"
                            }
                        }
                    }
                    div { class: "flex items-center justify-between text-sm",
                        span { class: "text-muted-foreground", "Badge ID" }
                        span { class: "text-foreground font-mono text-xs", "{badge.id}" }
                    }
                    if is_own_badge {
                        div { class: "flex gap-3 mt-6",
                            {
                                let mut start_processing = move |
                                    state: ProcessingState,
                                    handler: EventHandler<()>|
                                {
                                    processing_state.set(state);
                                    handler.call(());
                                };
                                let is_processing = *processing_state.read() != ProcessingState::Idle;
                                if is_accepted {
                                    rsx! {
                                        button {
                                            class: "flex-1 px-4 py-2 rounded-lg border border-destructive text-destructive hover:bg-destructive/10 transition disabled:opacity-50",
                                            disabled: is_processing,
                                            onclick: move |_| start_processing(ProcessingState::Removing, on_reject),
                                            if *processing_state.read() == ProcessingState::Removing {
                                                "Removing..."
                                            } else {
                                                "Remove from Profile"
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {
                                        button {
                                            class: "flex-1 px-4 py-2 rounded-lg border border-border hover:bg-accent transition disabled:opacity-50",
                                            disabled: is_processing,
                                            onclick: move |_| start_processing(ProcessingState::Declining, on_reject),
                                            if *processing_state.read() == ProcessingState::Declining {
                                                "Declining..."
                                            } else {
                                                "Decline"
                                            }
                                        }
                                        button {
                                            class: "flex-1 px-4 py-2 rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50",
                                            disabled: is_processing,
                                            onclick: move |_| start_processing(ProcessingState::Accepting, on_accept),
                                            if *processing_state.read() == ProcessingState::Accepting {
                                                "Accepting..."
                                            } else {
                                                "Accept Badge"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "mt-4 text-center",
                        Link {
                            to: Route::BadgeDetail {
                                naddr: badge.naddr.clone(),
                            },
                            class: "text-sm text-muted-foreground hover:text-primary transition",
                            onclick: move |_| on_close.call(()),
                            "View full details →"
                        }
                    }
                }
            }
        }
    }
}
