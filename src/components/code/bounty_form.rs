//! Bounty Form Component
//!
//! Inline form for creating bounties on git issues.
use crate::services::git_hosting::bounties::publish_bounty;
use crate::utils::nip34::decode_naddr;
use dioxus::prelude::*;
use nostr_sdk::prelude::EventId;

#[allow(dead_code)]
const PRESET_AMOUNTS: &[u64] = &[1_000, 5_000, 10_000, 50_000];

#[component]
pub fn BountyForm(
    issue_event_id: String,
    repository_naddr: String,
    on_cancel: EventHandler<()>,
    on_created: EventHandler<u64>,
) -> Element {
    let mut amount = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let handle_submit = {
        let issue_event_id = issue_event_id.clone();
        let repository_naddr = repository_naddr.clone();
        move |_| {
            let amount_str = amount.read().clone();
            let amount_sats: u64 = match amount_str.trim().parse() {
                Ok(v) if v > 0 => v,
                _ => {
                    error.set(Some("Enter a valid amount".to_string()));
                    return;
                }
            };
            let issue_id = issue_event_id.clone();
            let naddr = repository_naddr.clone();
            spawn(async move {
                is_submitting.set(true);
                error.set(None);
                let eid = match EventId::from_hex(&issue_id) {
                    Ok(id) => id,
                    Err(e) => {
                        error.set(Some(format!("Invalid issue ID: {}", e)));
                        is_submitting.set(false);
                        return;
                    }
                };
                let coord = if !naddr.is_empty() {
                    match decode_naddr(&naddr) {
                        Ok(c) => Some(c),
                        Err(e) => {
                            error.set(Some(format!("Invalid repository address: {}", e)));
                            is_submitting.set(false);
                            return;
                        }
                    }
                } else {
                    None
                };
                match publish_bounty(eid, amount_sats, coord.as_ref()).await {
                    Ok(_) => {
                        on_created.call(amount_sats);
                    }
                    Err(e) => {
                        error.set(Some(e));
                    }
                }
                is_submitting.set(false);
            });
        }
    };

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 space-y-3",
            h4 { class: "text-sm font-medium", "Add Bounty" }
            div { class: "flex flex-wrap gap-2",
                for preset in PRESET_AMOUNTS.iter() {
                    button {
                        key: "{preset}",
                        class: "px-3 py-1.5 text-xs border border-border rounded-lg hover:bg-accent/50 transition",
                        onclick: {
                            let val = *preset;
                            move |_| {
                                amount.set(val.to_string());
                                error.set(None);
                            }
                        },
                        "{preset / 1_000}k sats"
                    }
                }
            }
            input {
                class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-sm",
                r#type: "number",
                placeholder: "Amount in sats",
                min: "1",
                value: "{amount}",
                oninput: move |e| {
                    amount.set(e.value());
                    error.set(None);
                },
            }
            if let Some(err) = error.read().as_ref() {
                p { class: "text-xs text-destructive", "{err}" }
            }
            div { class: "flex items-center gap-2 justify-end",
                button {
                    class: "px-4 py-2 text-sm border border-border rounded-lg hover:bg-accent transition",
                    disabled: *is_submitting.read(),
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
                button {
                    class: "px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50",
                    disabled: *is_submitting.read() || amount.read().trim().is_empty(),
                    onclick: handle_submit,
                    if *is_submitting.read() {
                        "Publishing..."
                    } else {
                        "Add Bounty"
                    }
                }
            }
        }
    }
}
