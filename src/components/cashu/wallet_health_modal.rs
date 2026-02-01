//! Wallet Health Modal Component
//!
//! Shows detailed wallet health info and recovery options for stuck proofs.
use std::collections::HashMap;
use dioxus::prelude::*;
use crate::components::modal::{Modal, ModalBody, ModalFooter, ModalHeader};
use crate::stores::cashu::proof_recovery::{
    self, StuckProofInfo, UrgencyLevel, WalletHealthStats, PENDING_SPENT_TIMEOUT_DEFAULT,
    RESERVED_TIMEOUT_SECS,
};
use crate::stores::cashu::types::ProofState;
use crate::utils::format_sats_with_separator;
/// A group of stuck proofs from the same mint and transaction
struct ProofGroup {
    mint_url: String,
    transaction_id: Option<u64>,
    proofs: Vec<StuckProofInfo>,
}
/// Format a duration in seconds as a human-readable string
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if mins > 0 { format!("{}h {}m", hours, mins) } else { format!("{}h", hours) }
    } else {
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        if hours > 0 { format!("{}d {}h", days, hours) } else { format!("{}d", days) }
    }
}
/// Group stuck proofs by mint URL and transaction ID
///
/// Groups by (mint_url, transaction_id) to prevent proofs from different mints
/// with the same transaction_id from being incorrectly merged.
fn group_by_transaction(proofs: &[StuckProofInfo]) -> Vec<ProofGroup> {
    let mut groups: HashMap<(String, Option<u64>), Vec<StuckProofInfo>> = HashMap::new();
    for proof in proofs {
        groups
            .entry((proof.mint_url.clone(), proof.transaction_id))
            .or_default()
            .push(proof.clone());
    }
    let mut result: Vec<_> = groups
        .into_iter()
        .map(|((mint_url, transaction_id), proofs)| ProofGroup {
            mint_url,
            transaction_id,
            proofs,
        })
        .collect();
    result
        .sort_by(|a, b| {
            let a_max_urgency = a
                .proofs
                .iter()
                .map(|p| p.urgency)
                .max()
                .unwrap_or(UrgencyLevel::Normal);
            let b_max_urgency = b
                .proofs
                .iter()
                .map(|p| p.urgency)
                .max()
                .unwrap_or(UrgencyLevel::Normal);
            b_max_urgency.cmp(&a_max_urgency)
        });
    result
}
/// Get urgency icon
fn urgency_icon(urgency: UrgencyLevel) -> &'static str {
    match urgency {
        UrgencyLevel::Critical => "!",
        UrgencyLevel::High => "!",
        UrgencyLevel::Warning => "?",
        UrgencyLevel::Normal => "...",
    }
}
/// Get urgency color class
fn urgency_color_class(urgency: UrgencyLevel) -> &'static str {
    match urgency {
        UrgencyLevel::Critical => "text-red-500",
        UrgencyLevel::High => "text-orange-500",
        UrgencyLevel::Warning => "text-yellow-500",
        UrgencyLevel::Normal => "text-green-500",
    }
}
/// Calculate the remaining seconds until a group becomes eligible for recovery
/// Returns the EARLIEST (minimum) time when any proof in the group can be recovered
fn remaining_until_recovery(proofs: &[StuckProofInfo]) -> Option<u64> {
    proofs
        .iter()
        .filter_map(|p| {
            let timeout = match p.state {
                ProofState::Reserved => RESERVED_TIMEOUT_SECS,
                ProofState::PendingSpent => PENDING_SPENT_TIMEOUT_DEFAULT,
                _ => return None,
            };
            timeout.checked_sub(p.stuck_duration_secs)
        })
        .min()
}
/// Transaction group within the modal
#[component]
fn TransactionGroup(
    transaction_id: Option<u64>,
    proofs: Vec<StuckProofInfo>,
) -> Element {
    let total: u64 = proofs.iter().map(|p| p.amount).sum();
    let urgency = proofs.iter().map(|p| p.urgency).max().unwrap_or(UrgencyLevel::Normal);
    let can_recover = proofs.iter().any(|p| p.can_recover);
    let icon = urgency_icon(urgency);
    let icon_color = urgency_color_class(urgency);
    rsx! {
        div { class: "border border-border rounded-lg p-3",
            div { class: "flex items-center justify-between mb-2",
                div { class: "flex items-center gap-2",
                    span { class: "{icon_color} font-bold", "{icon}" }
                    span { class: "font-medium",
                        if let Some(id) = transaction_id {
                            "Transaction #{id}"
                        } else {
                            "Unknown Transaction"
                        }
                    }
                }
                span { class: "font-mono text-sm", "{format_sats_with_separator(total)} sats" }
            }
            div { class: "space-y-1 text-sm text-muted-foreground",
                for proof in proofs.iter() {
                    div {
                        key: "{proof.hashed_id}",
                        class: "flex justify-between pl-4",
                        span { "{format_sats_with_separator(proof.amount)} - {proof.state:?}" }
                        span { "{format_duration(proof.stuck_duration_secs)}" }
                    }
                }
            }
            div { class: "mt-2 text-xs",
                if can_recover {
                    span { class: "text-green-500", "Eligible for recovery" }
                } else {
                    span { class: "text-muted-foreground",
                        if let Some(remaining) = remaining_until_recovery(&proofs) {
                            "Recovery available in {format_duration(remaining)}"
                        } else {
                            "Recovery pending"
                        }
                    }
                }
            }
        }
    }
}
/// Detailed wallet health modal
#[component]
pub fn WalletHealthModal(open: Signal<bool>, on_close: EventHandler<()>) -> Element {
    let mut refresh_counter = use_signal(|| 0u32);
    let stats = use_memo(move || {
        let _ = refresh_counter();
        proof_recovery::get_wallet_health_stats()
    });
    let mut is_recovering = use_signal(|| false);
    let mut recovery_result = use_signal(|| Option::<String>::None);
    let health: WalletHealthStats = stats.read().clone();
    let recovering = *is_recovering.read();
    let result_msg = recovery_result.read().clone();
    let grouped = group_by_transaction(&health.stuck_proofs);
    let handle_recover_all = move |_| {
        is_recovering.set(true);
        recovery_result.set(None);
        spawn(async move {
            let result = proof_recovery::run_full_recovery().await;
            let msg = if result.recovered_count > 0 {
                format!(
                    "Recovered {} proofs ({} sats)",
                    result.recovered_count,
                    result.recovered_value,
                )
            } else if result.spent_count > 0 && result.errors.is_empty() {
                format!(
                    "Recovered 0 proofs (spent {} proofs / {} sats)",
                    result.spent_count,
                    result.spent_value,
                )
            } else if result.spent_count > 0 && !result.errors.is_empty() {
                let display = if result.errors.len() <= 3 {
                    result.errors.join(", ")
                } else {
                    format!(
                        "{} + {} more",
                        result.errors[..3].join(", "),
                        result.errors.len() - 3,
                    )
                };
                format!(
                    "{} spent, recovery completed with errors: {}",
                    result.spent_count,
                    display,
                )
            } else if !result.errors.is_empty() {
                let display = if result.errors.len() <= 3 {
                    result.errors.join(", ")
                } else {
                    format!(
                        "{} + {} more",
                        result.errors[..3].join(", "),
                        result.errors.len() - 3,
                    )
                };
                format!("Recovery completed with errors: {}", display)
            } else {
                "No proofs eligible for recovery yet".to_string()
            };
            recovery_result.set(Some(msg));
            refresh_counter.set(refresh_counter() + 1);
            is_recovering.set(false);
        });
    };
    rsx! {
        Modal { open,
            ModalHeader {
                title: "Wallet Health".to_string(),
                on_close: move |_| on_close.call(()),
            }
            ModalBody {
                div { class: "min-w-[350px] max-w-md",
                    div { class: "space-y-2 mb-6",
                        div { class: "flex justify-between",
                            span { class: "text-muted-foreground", "Spendable" }
                            span { class: "font-mono",
                                "{format_sats_with_separator(health.spendable_balance)} sats"
                            }
                        }
                        if health.pending_count > 0 {
                            div { class: "flex justify-between text-yellow-500",
                                span { "Pending ({health.pending_count} proofs)" }
                                span { class: "font-mono",
                                    "{format_sats_with_separator(health.pending_balance)} sats"
                                }
                            }
                        }
                        if health.stuck_count > 0 {
                            div { class: "flex justify-between text-red-500",
                                span { "Stuck ({health.stuck_count} proofs)" }
                                span { class: "font-mono",
                                    "{format_sats_with_separator(health.stuck_balance)} sats"
                                }
                            }
                        }
                    }
                    if health.stuck_count > 0 {
                        div { class: "space-y-4",
                            h3 { class: "font-semibold", "Stuck Proofs" }
                            for (idx , group) in grouped.iter().enumerate() {
                                {
                                    let group_key = match group.transaction_id {
                                        Some(tx_id) => format!("{}_tx_{}", group.mint_url, tx_id),
                                        None => format!("{}_idx_{}", group.mint_url, idx),
                                    };
                                    rsx! {
                                        TransactionGroup {
                                            key: "{group_key}",
                                            transaction_id: group.transaction_id,
                                            proofs: group.proofs.clone(),
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "text-center text-muted-foreground py-4", "No stuck proofs" }
                    }
                    if let Some(ref msg) = result_msg {
                        div { class: "mt-4 p-3 bg-muted rounded-lg text-sm", "{msg}" }
                    }
                }
            }
            ModalFooter {
                div { class: "flex justify-between w-full",
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 disabled:opacity-50",
                        disabled: recovering || health.stuck_count == 0,
                        onclick: handle_recover_all,
                        if recovering {
                            "Recovering..."
                        } else {
                            "Recover All Eligible"
                        }
                    }
                    button {
                        class: "px-4 py-2 rounded-lg hover:bg-accent",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }
            }
        }
    }
}
