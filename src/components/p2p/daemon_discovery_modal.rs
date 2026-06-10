use crate::stores::social::mostro::{
    discover_daemons, switch_to_daemon, DiscoveredDaemon, MOSTRO_NODE_CONFIG,
};
use crate::utils::format::truncate_pubkey;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::time::Duration;

#[component]
pub fn DaemonDiscoveryModal(
    on_close: EventHandler<()>,
    on_daemon_selected: EventHandler<()>,
) -> Element {
    let mut is_loading = use_signal(|| true);
    let mut discovered_daemons = use_signal(Vec::<DiscoveredDaemon>::new);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut selected_pubkey = use_signal(|| Option::<String>::None);
    let mut is_switching = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            match discover_daemons().await {
                Ok(daemons) => {
                    discovered_daemons.set(daemons);
                    is_loading.set(false);
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to discover daemons: {}", e)));
                    is_loading.set(false);
                }
            }
        });
    });

    let current_pubkey = use_memo(move || {
        MOSTRO_NODE_CONFIG
            .read()
            .as_ref()
            .map(|c| c.pubkey.clone())
            .unwrap_or_default()
    });

    let mut on_switch = move |pk: String| {
        is_switching.set(true);
        error_message.set(None);
        let pk_clone = pk.clone();
        spawn(async move {
            let daemons = discovered_daemons.read();
            let daemon = match daemons.iter().find(|d| d.pubkey == pk_clone) {
                Some(d) => d,
                None => {
                    error_message.set(Some("Daemon not found in results".to_string()));
                    is_switching.set(false);
                    return;
                }
            };
            match switch_to_daemon(daemon).await {
                Ok(()) => {
                    let toast = consume_toast();
                    toast.info(
                        "Daemon switched".to_string(),
                        ToastOptions::new().duration(Duration::from_secs(2)),
                    );
                    is_switching.set(false);
                    on_daemon_selected.call(());
                    on_close.call(());
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to switch: {}", e)));
                    is_switching.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| {
                if !*is_loading.read() && !*is_switching.read() {
                    on_close.call(());
                }
            },
            div {
                class: "bg-card border border-border rounded-lg max-w-lg w-full shadow-xl max-h-[80vh] flex flex-col",
                onclick: move |e| e.stop_propagation(),
                div { class: "px-6 py-4 border-b border-border flex items-center justify-between shrink-0",
                    h3 { class: "text-xl font-bold",
                        "Discover Daemons"
                    }
                    if !*is_loading.read() && !*is_switching.read() {
                        button {
                            class: "text-2xl text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| on_close.call(()),
                            "\u{00d7}"
                        }
                    }
                }
                div { class: "p-6 overflow-y-auto flex-1",
                    if let Some(msg) = error_message.read().as_ref() {
                        div { class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4 mb-4",
                            p { class: "text-sm text-red-800 dark:text-red-200", "{msg}" }
                        }
                    }
                    if *is_loading.read() {
                        div { class: "flex flex-col items-center justify-center py-12",
                            p { class: "text-muted-foreground", "Discovering Mostro daemons on the network..." }
                        }
                    } else if discovered_daemons.read().is_empty() {
                        div { class: "text-center py-12",
                            h4 { class: "text-lg font-semibold mb-2", "No daemons discovered" }
                            p { class: "text-muted-foreground text-sm",
                                "No Mostro daemon info events found on the network. Try configuring one manually."
                            }
                        }
                    } else {
                        div { class: "space-y-3",
                            p { class: "text-sm text-muted-foreground mb-4",
                                "Daemons discovered via kind 38385 info events. Trusted daemons shown first."
                            }
                            for daemon in discovered_daemons.read().iter() {
                                {
                                    let pk = daemon.pubkey.clone();
                                    let is_active = *current_pubkey.read() == daemon.pubkey;
                                    let is_selected = selected_pubkey.read().as_ref() == Some(&daemon.pubkey);
                                    let display_name = daemon.community_label
                                        .map(|s| s.to_string())
                                        .or_else(|| daemon.info.lnd_node_alias.clone())
                                        .unwrap_or_else(|| truncate_pubkey(&daemon.pubkey));

                                    let fee_str = daemon.info.fee
                                        .map(|f| format!("{:.1}% fee", f * 100.0))
                                        .unwrap_or_else(|| "Unknown fee".to_string());

                                    let currencies_str = if daemon.info.fiat_currencies_accepted.is_empty() {
                                        "Unknown currencies".to_string()
                                    } else {
                                        daemon.info.fiat_currencies_accepted.join(", ")
                                    };

                                    let bond_str = if daemon.info.bond_enabled {
                                        let mut parts = vec!["Bond required".to_string()];
                                        if let Some(pct) = daemon.info.bond_amount_pct {
                                            parts.push(format!("{:.1}%", pct));
                                        }
                                        if let Some(base) = daemon.info.bond_base_amount_sats {
                                            parts.push(format!("+ {} sats", base));
                                        }
                                        parts.join(" ")
                                    } else {
                                        "No bond".to_string()
                                    };

                                    let order_count = daemon.order_count;
                                    let order_suffix = if order_count == 1 { "" } else { "s" };

                                    rsx! {
                                        div {
                                            key: "{pk}",
                                            class: if is_active {
                                                "bg-accent/30 border border-border rounded-lg p-4 opacity-70"
                                            } else if is_selected {
                                                "bg-accent border-2 border-blue-500 rounded-lg p-4 cursor-pointer transition"
                                            } else {
                                                "bg-accent/50 border border-border rounded-lg p-4 cursor-pointer hover:border-blue-400 transition"
                                            },
                                            onclick: {
                                                let pk = pk.clone();
                                                move |_| {
                                                    if !is_active {
                                                        selected_pubkey.set(Some(pk.clone()));
                                                    }
                                                }
                                            },
                                            div { class: "flex items-start justify-between gap-2",
                                                div { class: "flex-1 min-w-0",
                                                    h4 { class: "font-semibold truncate", "{display_name}" }
                                                    p {
                                                        class: "text-xs text-muted-foreground truncate font-mono",
                                                        title: "{daemon.pubkey}",
                                                        "{daemon.pubkey}"
                                                    }
                                                }
                                                div { class: "flex items-center gap-2 shrink-0 flex-wrap justify-end",
                                                    if daemon.is_trusted {
                                                        span { class: "px-2 py-0.5 text-xs bg-green-500/20 text-green-600 dark:text-green-400 rounded",
                                                            "Trusted"
                                                        }
                                                    }
                                                    if is_active {
                                                        span { class: "px-2 py-0.5 text-xs bg-blue-500/20 text-blue-600 dark:text-blue-400 rounded",
                                                            "Active"
                                                        }
                                                    }
                                                    if let Some(ver) = &daemon.info.mostro_version {
                                                        span { class: "px-2 py-0.5 text-xs bg-muted text-muted-foreground rounded",
                                                            "v{ver}"
                                                        }
                                                    }
                                                }
                                            }
                                            div { class: "flex flex-wrap gap-x-4 gap-y-1 mt-2 text-xs text-muted-foreground",
                                                span { "{fee_str}" }
                                                span { "{currencies_str}" }
                                                span { "{bond_str}" }
                                            }
                                            if let (Some(min), Some(max)) = (daemon.info.min_order_amount, daemon.info.max_order_amount) {
                                                div { class: "text-xs text-muted-foreground mt-1",
                                                    "Orders: {min} - {max} sats"
                                                }
                                            }
                                            div { class: "flex items-center justify-between mt-2 pt-2 border-t border-border/50",
                                                span { class: "text-xs text-muted-foreground",
                                                    "{order_count} pending order{order_suffix}"
                                                }
                                                if let Some(alias) = &daemon.info.lnd_node_alias {
                                                    span {
                                                        class: "text-xs text-muted-foreground",
                                                        title: "{alias}",
                                                        "LND: {alias}"
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
                div { class: "px-6 py-4 border-t border-border flex gap-3 shrink-0",
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        disabled: *is_switching.read(),
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    if let Some(pk) = selected_pubkey.read().clone() {
                        button {
                            class: if *is_switching.read() {
                                "flex-1 px-4 py-3 bg-blue-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                            } else {
                                "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition"
                            },
                            disabled: *is_switching.read(),
                            onclick: move |_| on_switch(pk.clone()),
                            if *is_switching.read() {
                                "Switching..."
                            } else {
                                "Switch to this Daemon"
                            }
                        }
                    }
                }
            }
        }
    }
}
