//! Admin/solver dispute list page.
//!
//! `/p2p/admin/disputes` — lists all disputes (kind 38386) from the
//! configured daemon. Requires admin keys to be loaded.
//!
//! Subscribes to kind 38386 events and renders dispute cards with status
//! badges. Clicking a dispute navigates to the detail page.

use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::mostro::{
    admin_keys, dispute_store, try_get_node_config, ensure_node_relays_connected,
};
use dioxus::prelude::*;
use nostr::prelude::*;

/// Admin dispute list. Shows all disputes from the configured daemon.
#[component]
pub fn MostroAdminDisputes() -> Element {
    let admin_keys_loaded = admin_keys::try_get().is_some();
    let nav = navigator();

    if !*crate::stores::nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! {
            div { class: "min-h-screen p-4 max-w-3xl mx-auto",
                ClientInitializing {}
            }
        };
    }

    if !admin_keys_loaded {
        return rsx! {
            div { class: "min-h-screen p-4 max-w-3xl mx-auto flex items-center justify-center",
                div { class: "text-center space-y-4",
                    div { class: "text-4xl", "🔐" }
                    h3 { class: "text-lg font-medium", "Admin Keys Required" }
                    p { class: "text-sm text-muted-foreground",
                        "Configure your solver nsec in Settings → P2P to access the dispute resolution interface."
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm",
                        onclick: move |_| { let _ = nav.push(Route::SettingsMostro {}); },
                        "Go to Settings"
                    }
                }
            }
        };
    }

    // Subscribe to kind 38386 events from the daemon
    let daemon_pk_hex = try_get_node_config().map(|n| n.pubkey.clone()).unwrap_or_default();
    let daemon_pk = daemon_pk_hex
        .as_str()
        .parse::<PublicKey>()
        .ok()
        .or_else(|| PublicKey::from_hex(&daemon_pk_hex).ok());

    {
        let dpk = daemon_pk;
        use_effect(move || {
            if let Some(pk) = dpk {
                spawn(async move {
                    ensure_node_relays_connected().await;
                    let client = match crate::stores::nostr_client::get_client() {
                        Some(c) => c,
                        None => return,
                    };
                    let filter = Filter::new()
                        .kind(Kind::Custom(38386))
                        .author(pk)
                        .limit(100);
                    match client.fetch_events(filter, std::time::Duration::from_secs(10)).await {
                        Ok(events) => {
                            for event in events.into_iter() {
                                if let Some(d) = dispute_store::parse_dispute_event(&event) {
                                    dispute_store::upsert(d);
                                }
                            }
                        }
                        Err(e) => log::warn!("Failed to fetch disputes: {e}"),
                    }
                });
            }
        });
    }

    // Live subscription for new dispute events
    {
        let live_f = daemon_pk.map(|pk| {
            Filter::new()
                .kind(Kind::Custom(38386))
                .author(pk)
                .limit(0)
        });
        crate::hooks::use_relay_subscription(
            live_f,
            move |event: &nostr_sdk::Event| {
                if let Some(d) = dispute_store::parse_dispute_event(event) {
                    dispute_store::upsert(d);
                }
            },
        );
    }

    let disputes = dispute_store::filter_for_daemon(&daemon_pk_hex);
    let sorted: Vec<_> = {
        let mut v: Vec<_> = disputes.iter().collect();
        v.sort_by_key(|d| std::cmp::Reverse(d.created_at));
        v
    };

    rsx! {
        div { class: "min-h-screen p-4 max-w-3xl mx-auto",
            div { class: "flex items-center gap-3 mb-4",
                button {
                    class: "p-2 hover:bg-accent rounded-lg",
                    title: "Back",
                    onclick: move |_| { let _ = nav.push(Route::MostroHome {}); },
                    crate::components::icons::ArrowLeftIcon { class: "w-5 h-5".to_string() }
                }
                h1 { class: "text-xl font-bold flex-1", "Disputes" }
                button {
                    class: "px-3 py-1.5 bg-primary text-primary-foreground rounded-lg text-sm font-medium",
                    title: "Add Solver",
                    onclick: move |_| { let _ = nav.push(Route::MostroAdminSolvers {}); },
                    "Add Solver"
                }
            }

            if sorted.is_empty() {
                div { class: "p-8 text-center",
                    div { class: "text-4xl mb-4", "⚖️" }
                    h3 { class: "text-lg font-medium mb-2", "No active disputes" }
                    p { class: "text-muted-foreground",
                        "Disputes from the configured daemon will appear here."
                    }
                }
            } else {
                div { class: "space-y-2",
                    for dispute in sorted {
                        {
                            let d = dispute.clone();
                            let did = d.dispute_id.clone();
                            let status_cls = match d.status {
                                dispute_store::DisputeStatus::Initiated =>
                                    "bg-red-500/10 text-red-500 border-red-500/20",
                                dispute_store::DisputeStatus::InProgress =>
                                    "bg-amber-500/10 text-amber-500 border-amber-500/20",
                                dispute_store::DisputeStatus::SellerRefunded =>
                                    "bg-blue-500/10 text-blue-500 border-blue-500/20",
                                dispute_store::DisputeStatus::Settled =>
                                    "bg-green-500/10 text-green-500 border-green-500/20",
                                dispute_store::DisputeStatus::Released =>
                                    "bg-green-500/10 text-green-500 border-green-500/20",
                            };
                            let short_id = if d.dispute_id.len() > 12 {
                                format!("{}…{}", &d.dispute_id[..8], &d.dispute_id[d.dispute_id.len()-4..])
                            } else {
                                d.dispute_id.clone()
                            };
                            rsx! {
                                button {
                                    key: "{d.dispute_id}",
                                    class: "w-full p-4 bg-card border border-border rounded-lg hover:bg-accent transition text-left",
                                    onclick: move |_| {
                                        let _ = nav.push(Route::MostroAdminDisputeDetail {
                                            dispute_id: did.clone(),
                                        });
                                    },
                                    div { class: "flex items-center justify-between",
                                        span { class: "text-sm font-medium font-mono", "{short_id}" }
                                        span { class: "text-xs px-2 py-0.5 rounded-full border {status_cls}",
                                            {d.status.label()}
                                        }
                                    }
                                    div { class: "mt-1 text-xs text-muted-foreground",
                                        "Initiator: {d.initiator_label()}"
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

/// Convenience trait for rendering dispute initiator label.
trait DisputeInitiatorExt {
    fn initiator_label(&self) -> &'static str;
}

impl DisputeInitiatorExt for dispute_store::Dispute {
    fn initiator_label(&self) -> &'static str {
        match self.initiator {
            dispute_store::DisputeInitiator::Buyer => "Buyer",
            dispute_store::DisputeInitiator::Seller => "Seller",
            dispute_store::DisputeInitiator::Unknown => "Unknown",
        }
    }
}
