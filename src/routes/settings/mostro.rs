//! Settings page for Mostro P2P exchange integration
//!
//! Lets the user:
//! - See Mostro key status (initialized, trade index, identity pubkey)
//! - Toggle privacy mode (identity == trade key, no reputation)
//! - Export/import the Mostro mnemonic
//! - Configure the Mostro daemon node (pubkey + relays)
//! - Restore session / request trade index from daemon
//! - Reset all Mostro state

use crate::components::{ClientInitializing, DaemonDiscoveryModal};
use crate::stores::mostro::{
    self, MostroKeyState, MOSTRO_KEYS, MOSTRO_NODE_CONFIG, MOSTRO_PRIVACY_MODE,
    parse_node_pubkey,
};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr::prelude::*;
use std::time::Duration;

#[component]
pub fn SettingsMostro() -> Element {
    let keys_signal = MOSTRO_KEYS.read();
    let privacy = *MOSTRO_PRIVACY_MODE.read();
    let node_cfg = MOSTRO_NODE_CONFIG.read().clone();

    let mut privacy_saving = use_signal(|| false);
    let mut privacy_error = use_signal(|| Option::<String>::None);
    let mut import_text = use_signal(String::new);
    let mut import_error = use_signal(|| Option::<String>::None);
    let mut importing = use_signal(|| false);
    let mut reset_confirm = use_signal(|| false);
    let mut reset_error = use_signal(|| Option::<String>::None);

    let mut node_pubkey = use_signal(|| {
        node_cfg.as_ref().map(|n| n.pubkey.clone()).unwrap_or_default()
    });
    let mut node_relays = use_signal(|| {
        node_cfg
            .as_ref()
            .map(|n| n.relays.join("\n"))
            .unwrap_or_default()
    });
    let mut node_label = use_signal(|| {
        node_cfg.as_ref().and_then(|n| n.label.clone()).unwrap_or_default()
    });
    let mut node_saving = use_signal(|| false);
    let mut node_error = use_signal(|| Option::<String>::None);

    let mut restore_busy = use_signal(|| false);
    let mut restore_error = use_signal(|| Option::<String>::None);
    let mut show_discover_modal = use_signal(|| false);
    let mut clear_confirm = use_signal(|| false);
    let mut clear_busy = use_signal(|| false);

    use_effect(move || {
        let cfg = MOSTRO_NODE_CONFIG.read();
        if let Some(n) = cfg.as_ref() {
            if *node_pubkey.peek() != n.pubkey {
                node_pubkey.set(n.pubkey.clone());
            }
            if *node_relays.peek() != n.relays.join("\n") {
                node_relays.set(n.relays.join("\n"));
            }
            let label = n.label.clone().unwrap_or_default();
            if *node_label.peek() != label {
                node_label.set(label);
            }
        }
    });

    let on_privacy_toggle = move |_| {
        privacy_saving.set(true);
        privacy_error.set(None);
        let target = !*MOSTRO_PRIVACY_MODE.peek();
        spawn(async move {
            if let Err(e) = mostro::set_privacy_mode(target) {
                privacy_error.set(Some(e));
            }
            privacy_saving.set(false);
        });
    };

    let on_import = move |_| {
        let text = import_text.read().clone();
        if text.trim().is_empty() {
            import_error.set(Some("Please paste a mnemonic first.".to_string()));
            return;
        }
        importing.set(true);
        import_error.set(None);
        spawn(async move {
            match mostro::import_mnemonic(&text) {
                Ok(()) => {
                    import_text.set(String::new());
                }
                Err(e) => import_error.set(Some(e)),
            }
            importing.set(false);
        });
    };

    let on_reset = move || {
        spawn(async move {
            if let Err(e) = mostro::reset_all_with_publish().await {
                reset_error.set(Some(e));
            } else {
                reset_error.set(None);
            }
        });
    };

    let on_save_node = move |_| {
        node_saving.set(true);
        node_error.set(None);
        let pk = node_pubkey.read().clone();
        let relays_text = node_relays.read().clone();
        let label = node_label.read().clone();
        spawn(async move {
            let relays: Vec<String> = relays_text
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            let label_opt = if label.trim().is_empty() {
                None
            } else {
                Some(label.trim().to_string())
            };
            match mostro::MostroNodeConfig::new(pk, relays, label_opt) {
                Ok(cfg) => {
                    if let Err(e) = mostro::save_node_config(cfg).await {
                        node_error.set(Some(e));
                    } else {
                        let toast = consume_toast();
                        toast.info(
                            "Node saved".to_string(),
                            ToastOptions::new().duration(Duration::from_secs(2)),
                        );
                    }
                }
                Err(e) => node_error.set(Some(e)),
            }
            node_saving.set(false);
        });
    };

    let on_restore_session = move |_| {
        restore_busy.set(true);
        restore_error.set(None);
        spawn(async move {
            let keys = match mostro::try_get() {
                Some(k) => k,
                None => {
                    restore_error.set(Some("Mostro keys not initialized".to_string()));
                    restore_busy.set(false);
                    return;
                }
            };
            let node = match mostro::try_get_node_config() {
                Some(n) => n,
                None => {
                    restore_error.set(Some("Node not configured".to_string()));
                    restore_busy.set(false);
                    return;
                }
            };
            let node_pk = match parse_node_pubkey(&node.pubkey) {
                Ok(p) => p,
                Err(e) => {
                    restore_error.set(Some(format!("Bad node pubkey: {e}")));
                    restore_busy.set(false);
                    return;
                }
            };
            let identity_keys = keys.identity_keys.clone();
            let message = mostro::restore_session();
            let pow = mostro::resolve_effective_pow(&node, node_pk).await;
            if let Err(e) = mostro::send_mostro_message(
                &message,
                &identity_keys,
                &identity_keys,
                node_pk,
                &node.relays,
                pow,
            )
            .await
            {
                restore_error.set(Some(format!("Send failed: {e}")));
                restore_busy.set(false);
                return;
            }
            let toast = consume_toast();
            toast.info(
                "Session restore requested".to_string(),
                ToastOptions::new()
                    .description("Waiting for daemon response…".to_string())
                    .duration(Duration::from_secs(3)),
            );
            restore_busy.set(false);
        });
    };

    let on_request_trade_index = move |_| {
        spawn(async move {
            let keys = match mostro::try_get() {
                Some(k) => k,
                None => return,
            };
            let node = match mostro::try_get_node_config() {
                Some(n) => n,
                None => return,
            };
            let node_pk = match parse_node_pubkey(&node.pubkey) {
                Ok(p) => p,
                Err(_) => return,
            };
            let identity_keys = keys.identity_keys.clone();
            let message = mostro::last_trade_index();
            let pow = mostro::resolve_effective_pow(&node, node_pk).await;
            let _ = mostro::send_mostro_message(
                &message,
                &identity_keys,
                &identity_keys,
                node_pk,
                &node.relays,
                pow,
            )
            .await;
            let toast = consume_toast();
            toast.info(
                "Trade index requested".to_string(),
                ToastOptions::new().duration(Duration::from_secs(2)),
            );
        });
    };

    let on_clear_daemon = move |_| {
        clear_busy.set(true);
        node_error.set(None);
        spawn(async move {
            if let Err(e) = mostro::clear_node_config().await {
                node_error.set(Some(e));
            } else {
                node_pubkey.set(String::new());
                node_relays.set(String::new());
                node_label.set(String::new());
                let toast = consume_toast();
                toast.info(
                    "Daemon cleared".to_string(),
                    ToastOptions::new().duration(Duration::from_secs(2)),
                );
            }
            clear_busy.set(false);
            clear_confirm.set(false);
        });
    };

    rsx! {
        div { class: "min-h-screen p-4 max-w-3xl mx-auto",
            if *show_discover_modal.read() {
                DaemonDiscoveryModal {
                    on_close: move |_| show_discover_modal.set(false),
                    on_daemon_selected: move |_| {
                        let cfg = MOSTRO_NODE_CONFIG.read().clone();
                        if let Some(n) = cfg {
                            node_pubkey.set(n.pubkey.clone());
                            node_relays.set(n.relays.join("\n"));
                            node_label.set(n.label.unwrap_or_default());
                        }
                    },
                }
            }
            if !*crate::stores::nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else {
                div { class: "space-y-6",
                    // Page header
                    div {
                        h1 { class: "text-2xl font-bold mb-1", "P2P Trading Settings" }
                        p { class: "text-sm text-muted-foreground",
                            "Manage your Mostro P2P exchange keys and preferences."
                        }
                    }

                    // Key status card
                    div { class: "p-4 bg-card border border-border rounded-lg",
                        h2 { class: "text-lg font-semibold mb-3", "Mostro Keys" }
                        match &*keys_signal {
                            MostroKeyState::NotInitialized => rsx! {
                                div { class: "text-sm text-muted-foreground",
                                    "Keys not yet initialized. They will be generated on first use."
                                }
                            },
                            MostroKeyState::Loading => rsx! {
                                div { class: "text-sm text-muted-foreground", "Loading..." }
                            },
                            MostroKeyState::Error(e) => rsx! {
                                div { class: "text-sm text-red-500", "Error: {e}" }
                            },
                            MostroKeyState::Ready(keys) => rsx! {
                                div { class: "space-y-2 text-sm",
                                    div {
                                        span { class: "text-muted-foreground", "Identity pubkey: " }
                                        span { class: "font-mono text-xs",
                                            "{keys.identity_keys.public_key().to_bech32().unwrap_or_default()}"
                                        }
                                    }
                                    div {
                                        span { class: "text-muted-foreground", "Next trade index: " }
                                        span { class: "font-medium", "{keys.trade_index}" }
                                    }
                                }
                            },
                        }
                    }

                    // Privacy mode toggle
                    div { class: "p-4 bg-card border border-border rounded-lg",
                        div { class: "flex items-center justify-between",
                            div { class: "flex-1 pr-4",
                                h2 { class: "text-lg font-semibold", "Privacy Mode" }
                                p { class: "text-sm text-muted-foreground mt-1",
                                    "When enabled, your identity and trade keys are the same. "
                                    "You won't build reputation on the order book, but your trades are unlinkable."
                                }
                            }
                            button {
                                class: if privacy {
                                    "px-4 py-2 bg-primary text-primary-foreground rounded-lg"
                                } else {
                                    "px-4 py-2 border border-border rounded-lg text-muted-foreground hover:text-foreground"
                                },
                                disabled: *privacy_saving.read(),
                                onclick: on_privacy_toggle,
                                if privacy { "Enabled" } else { "Disabled" }
                            }
                        }
                        if let Some(err) = privacy_error.read().as_ref() {
                            p { class: "mt-2 text-sm text-red-500", "{err}" }
                        }
                    }

                    // Node configuration
                    div { class: "p-4 bg-card border border-border rounded-lg",
                        h2 { class: "text-lg font-semibold mb-2", "Node Configuration" }
                        p { class: "text-sm text-muted-foreground mb-3",
                            "The Mostro daemon you trade with. Discover daemons on the network or configure one manually."
                        }
                        div { class: "flex gap-2 mb-3",
                            button {
                                class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg text-sm font-medium",
                                onclick: move |_| show_discover_modal.set(true),
                                "Discover Daemons"
                            }
                            if MOSTRO_NODE_CONFIG.read().is_some() {
                                if *clear_confirm.read() {
                                    button {
                                        class: "px-4 py-2 bg-destructive text-destructive-foreground rounded-lg text-sm font-medium disabled:opacity-50",
                                        disabled: *clear_busy.read(),
                                        onclick: on_clear_daemon,
                                        if *clear_busy.read() { "Clearing..." } else { "Confirm Clear" }
                                    }
                                    button {
                                        class: "px-4 py-2 border border-border rounded-lg text-sm",
                                        disabled: *clear_busy.read(),
                                        onclick: move |_| clear_confirm.set(false),
                                        "Cancel"
                                    }
                                } else {
                                    button {
                                        class: "px-4 py-2 border border-border rounded-lg text-sm text-muted-foreground hover:text-foreground",
                                        onclick: move |_| clear_confirm.set(true),
                                        "Clear Daemon"
                                    }
                                }
                            }
                        }
                        div { class: "space-y-3",
                            div {
                                label { class: "text-xs text-muted-foreground", "Daemon pubkey (hex or npub)" }
                                input {
                                    class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm font-mono",
                                    r#type: "text",
                                    placeholder: "npub1... or hex",
                                    value: "{node_pubkey}",
                                    oninput: move |e| node_pubkey.set(e.value()),
                                }
                            }
                            div {
                                label { class: "text-xs text-muted-foreground", "Relay URLs (one per line)" }
                                textarea {
                                    class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm font-mono min-h-20",
                                    placeholder: "wss://relay.damus.io\nwss://relay.primal.net",
                                    value: "{node_relays}",
                                    oninput: move |e| node_relays.set(e.value()),
                                }
                            }
                            div {
                                label { class: "text-xs text-muted-foreground", "Label (optional)" }
                                input {
                                    class: "w-full mt-1 p-2 border border-border rounded-lg bg-background text-sm",
                                    r#type: "text",
                                    placeholder: "Mostro Mainnet",
                                    value: "{node_label}",
                                    oninput: move |e| node_label.set(e.value()),
                                }
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg disabled:opacity-50",
                                disabled: *node_saving.read(),
                                onclick: on_save_node,
                                if *node_saving.read() { "Saving..." } else { "Save Node Config" }
                            }
                            if let Some(err) = node_error.read().as_ref() {
                                p { class: "text-sm text-red-500", "{err}" }
                            }
                        }
                    }

                    // Daemon Capabilities card — Phase 3e (mobile About-screen
                    // port). Surfaces the parsed kind-38385 info-event fields
                    // so users can see the daemon's transport, bond policy,
                    // and trade limits before committing. Most CLI/TUI clients
                    // hide this; surfacing it is a genuine UX improvement.
                    if let Some(cfg) = &*MOSTRO_NODE_CONFIG.read() {
                        div { class: "p-4 bg-card border border-border rounded-lg",
                            h2 { class: "text-lg font-semibold mb-3", "Daemon Capabilities" }
                            div { class: "space-y-2 text-sm",
                                // Wire transport (protocol_version).
                                div { class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Transport" }
                                    span {
                                        class: "font-medium",
                                        match cfg.protocol_version {
                                            2 => "NIP-44 direct (v2)",
                                            _ => "Gift-wrap (v1)",
                                        }
                                    }
                                }
                                // Bond policy section.
                                if cfg.bond_enabled {
                                    div { class: "pt-2 mt-2 border-t border-border/50",
                                        p { class: "text-xs font-semibold text-muted-foreground uppercase mb-2",
                                            "Anti-Abuse Bond"
                                        }
                                        div { class: "space-y-1.5",
                                            if let Some(apply) = &cfg.bond_apply_to {
                                                div { class: "flex justify-between",
                                                    span { class: "text-muted-foreground", "Applies to" }
                                                    span { "{apply}" }
                                                }
                                            }
                                            if let Some(pct) = cfg.bond_amount_pct {
                                                div { class: "flex justify-between",
                                                    span { class: "text-muted-foreground", "Amount" }
                                                    span { "{pct}% of order" }
                                                }
                                            }
                                            if let Some(base) = cfg.bond_base_amount_sats {
                                                div { class: "flex justify-between",
                                                    span { class: "text-muted-foreground", "Minimum" }
                                                    span { "{base} sats" }
                                                }
                                            }
                                            if let Some(slash_timeout) = cfg.bond_slash_on_waiting_timeout {
                                                div { class: "flex justify-between",
                                                    span { class: "text-muted-foreground", "Slash on timeout" }
                                                    span {
                                                        if slash_timeout { "Yes" } else { "No" }
                                                    }
                                                }
                                            }
                                            if let Some(share) = cfg.bond_slash_node_share_pct {
                                                div { class: "flex justify-between",
                                                    span { class: "text-muted-foreground", "Node share" }
                                                    span { {format!("{:.0}%", share * 100.0)} }
                                                }
                                            }
                                            div { class: "flex justify-between",
                                                span { class: "text-muted-foreground", "Claim window" }
                                                span { {format!("{} days", cfg.bond_payout_claim_window_days)} }
                                            }
                                        }
                                    }
                                } else {
                                    div { class: "flex justify-between text-muted-foreground",
                                        span { "Anti-abuse bonds" }
                                        span { "Disabled" }
                                    }
                                }
                                // Trade limits.
                                if let (Some(min), Some(max)) = (cfg.min_order_amount, cfg.max_order_amount) {
                                    div { class: "flex justify-between pt-2 mt-2 border-t border-border/50",
                                        span { class: "text-muted-foreground", "Order range" }
                                        span { "{min}–{max} sats" }
                                    }
                                }
                                if !cfg.fiat_currencies_accepted.is_empty() {
                                    div { class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Currencies" }
                                        span { {cfg.fiat_currencies_accepted.join(", ")} }
                                    }
                                }
                                if let Some(fee) = cfg.fee {
                                    div { class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Fee" }
                                        span { {format!("{:.0}%", fee * 100.0)} }
                                    }
                                }
                                if cfg.pow > 0 {
                                    div { class: "flex justify-between",
                                        span { class: "text-muted-foreground", "Required PoW" }
                                        span { {format!("{} bits", cfg.pow)} }
                                    }
                                }
                            }
                        }
                    }

                    // Session management
                    div { class: "p-4 bg-card border border-border rounded-lg",
                        h2 { class: "text-lg font-semibold mb-2", "Session Management" }
                        p { class: "text-sm text-muted-foreground mb-3",
                            "Restore your session state or request the last trade index from the daemon."
                        }
                        div { class: "flex gap-2",
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg disabled:opacity-50",
                                disabled: *restore_busy.read(),
                                onclick: on_restore_session,
                                if *restore_busy.read() { "Restoring..." } else { "Restore Session" }
                            }
                            button {
                                class: "px-4 py-2 border border-border rounded-lg",
                                onclick: on_request_trade_index,
                                "Request Trade Index"
                            }
                        }
                        if let Some(err) = restore_error.read().as_ref() {
                            p { class: "mt-2 text-sm text-red-500", "{err}" }
                        }
                    }

                    // Mnemonic import
                    div { class: "p-4 bg-card border border-border rounded-lg",
                        h2 { class: "text-lg font-semibold mb-2", "Import Mnemonic" }
                        p { class: "text-sm text-muted-foreground mb-3",
                            "Paste a 12 or 24-word BIP-39 mnemonic from another device to sync. "
                            "This will replace your current Mostro mnemonic and reset the trade index."
                        }
                        textarea {
                            class: "w-full p-2 border border-border rounded-lg bg-background text-foreground text-sm font-mono min-h-20",
                            placeholder: "word1 word2 word3 ...",
                            value: "{import_text}",
                            oninput: move |e| import_text.set(e.value()),
                        }
                        div { class: "mt-3 flex items-center gap-3",
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg disabled:opacity-50",
                                disabled: *importing.read(),
                                onclick: on_import,
                                if *importing.read() { "Importing..." } else { "Import" }
                            }
                            if let Some(err) = import_error.read().as_ref() {
                                span { class: "text-sm text-red-500", "{err}" }
                            }
                        }
                    }

                    // Export (read-only display of the current mnemonic)
                    if let MostroKeyState::Ready(_) = &*keys_signal {
                        if let Some(mnemonic) = mostro::export_mnemonic() {
                            div { class: "p-4 bg-card border border-border rounded-lg",
                                h2 { class: "text-lg font-semibold mb-2", "Export Mnemonic" }
                                p { class: "text-sm text-muted-foreground mb-3",
                                    "Back up your Mostro mnemonic. Anyone with this phrase can trade as you."
                                }
                                div { class: "p-3 bg-muted rounded-lg font-mono text-xs break-all",
                                    "{mnemonic}"
                                }
                            }
                        }
                    }

                    // Phase 7.6: Mostro preferences card
                    div { class: "p-4 bg-card border border-border rounded-lg",
                        h2 { class: "text-lg font-semibold mb-3", "Preferences" }

                        div { class: "space-y-4",
                            // Default fiat currency
                            div {
                                label { class: "text-sm font-medium", "Default Fiat Currency" }
                                input {
                                    class: "mt-1 w-full p-2 border border-border rounded-lg bg-background text-sm",
                                    r#type: "text",
                                    placeholder: "USD",
                                    value: "{crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().default_fiat_code.clone().unwrap_or_default()}",
                                    oninput: move |e| {
                                        let val = if e.value().trim().is_empty() { None } else { Some(e.value().trim().to_uppercase()) };
                                        crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.write().default_fiat_code = val;
                                    },
                                }
                            }

                            // Default Lightning address
                            div {
                                label { class: "text-sm font-medium", "Default Lightning Address" }
                                input {
                                    class: "mt-1 w-full p-2 border border-border rounded-lg bg-background text-sm",
                                    r#type: "text",
                                    placeholder: "you@walletofsatoshi.com",
                                    value: "{crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().default_ln_address.clone().unwrap_or_default()}",
                                    oninput: move |e| {
                                        let val = if e.value().trim().is_empty() { None } else { Some(e.value().trim().to_string()) };
                                        crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.write().default_ln_address = val;
                                    },
                                }
                            }

                            // Notification toggles
                            div {
                                label { class: "text-sm font-medium", "Notifications" }
                                div { class: "mt-2 space-y-2",
                                    {render_toggle("Trade updates", || crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().notify_trade_updates, |v| crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.write().notify_trade_updates = v)}
                                    {render_toggle("Chat messages", || crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().notify_chat_messages, |v| crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.write().notify_chat_messages = v)}
                                    {render_toggle("Dispute updates", || crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().notify_dispute_updates, |v| crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.write().notify_dispute_updates = v)}
                                    {render_toggle("Sound", || crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().notify_sound, |v| crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.write().notify_sound = v)}
                                    {render_toggle("Vibration", || crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().notify_vibration, |v| crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.write().notify_vibration = v)}
                                }
                            }

                            // Trade history expiration
                            div {
                                label { class: "text-sm font-medium", "Trade History Expiration" }
                                select {
                                    class: "mt-1 w-full p-2 border border-border rounded-lg bg-background text-sm",
                                    onchange: move |e| {
                                        let days = match e.value().as_str() {
                                            "7" => 7,
                                            "30" => 30,
                                            "90" => 90,
                                            "0" => 0,
                                            _ => 30,
                                        };
                                        crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.write().trade_history_expiration_days = days;
                                    },
                                    option {
                                        value: "7",
                                        selected: crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().trade_history_expiration_days == 7,
                                        "7 days"
                                    }
                                    option {
                                        value: "30",
                                        selected: crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().trade_history_expiration_days == 30,
                                        "30 days"
                                    }
                                    option {
                                        value: "90",
                                        selected: crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().trade_history_expiration_days == 90,
                                        "90 days"
                                    }
                                    option {
                                        value: "0",
                                        selected: crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().trade_history_expiration_days == 0,
                                        "Never"
                                    }
                                }
                            }

                            // Save button
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium",
                                onclick: move |_| {
                                    let toast = consume_toast();
                                    spawn(async move {
                                        match crate::stores::ui::p2p_settings::publish().await {
                                            Ok(()) => {
                                                toast.info(
                                                    "Saved".to_string(),
                                                    ToastOptions::new()
                                                        .description("P2P preferences saved.")
                                                        .duration(Duration::from_secs(3)),
                                                );
                                            }
                                            Err(e) => {
                                                toast.error(
                                                    "Save failed".to_string(),
                                                    ToastOptions::new()
                                                        .description(e)
                                                        .duration(Duration::from_secs(5)),
                                                );
                                            }
                                        }
                                    });
                                },
                                "Save Preferences"
                            }

                            // Phase 9.7: notification permission request.
                            div { class: "pt-2 border-t border-border",
                                if crate::stores::mostro::notifications::has_notification_permission() {
                                    p { class: "text-xs text-green-600",
                                        "✓ Notifications are enabled"
                                    }
                                } else {
                                    button {
                                        class: "px-4 py-2 border border-border rounded-lg text-sm hover:bg-accent transition",
                                        onclick: move |_| {
                                            crate::stores::mostro::notifications::request_permission();
                                        },
                                        "Enable Notifications"
                                    }
                                    p { class: "mt-1 text-xs text-muted-foreground",
                                        "Allow browser notifications to get trade updates even when this tab is in the background."
                                    }
                                }
                            }
                        }
                    }

                    // Reset
                    div { class: "p-4 bg-card border border-destructive/30 rounded-lg",
                        h2 { class: "text-lg font-semibold text-destructive mb-2", "Reset Mostro" }
                        p { class: "text-sm text-muted-foreground mb-3",
                            "Delete all Mostro keys, trade index, and terms agreement. "
                            "Your published orders and ratings on Nostr will remain visible (they're public)."
                        }
                        if *reset_confirm.read() {
                            div { class: "flex items-center gap-3",
                                button {
                                    class: "px-4 py-2 bg-destructive text-destructive-foreground rounded-lg",
                                    onclick: move |_| {
                                        on_reset();
                                        reset_confirm.set(false);
                                    },
                                    "Yes, reset everything"
                                }
                                button {
                                    class: "px-4 py-2 border border-border rounded-lg",
                                    onclick: move |_| reset_confirm.set(false),
                                    "Cancel"
                                }
                            }
                        } else {
                            button {
                                class: "px-4 py-2 border border-destructive/30 text-destructive rounded-lg hover:bg-destructive/10",
                                onclick: move |_| reset_confirm.set(true),
                                "Reset Mostro"
                            }
                        }
                        if let Some(err) = reset_error.read().as_ref() {
                            p { class: "mt-2 text-sm text-red-500", "{err}" }
                        }
                    }
                }
            }
        }
    }
}

/// Phase 7.6: render a labeled toggle checkbox for a boolean setting.
fn render_toggle(
    label: &str,
    read: impl Fn() -> bool,
    write: impl Fn(bool) + Clone + 'static,
) -> Element {
    let is_checked = read();
    let write_clone = write.clone();
    rsx! {
        label { class: "flex items-center gap-2 cursor-pointer text-sm",
            input {
                r#type: "checkbox",
                class: "w-4 h-4",
                checked: "{is_checked}",
                onchange: move |e| write_clone(e.value() == "true"),
            }
            "{label}"
        }
    }
}
