//! Admin/solver management page.
//!
//! `/p2p/admin/solvers` — register new dispute solvers with the Mostro
//! daemon. Requires admin keys to be loaded.
//!
//! Sends `Action::AdminAddSolver` with `Payload::TextMessage("npub[:permission]")`
//! per the daemon's `admin_add_solver.rs:12-44`.

use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::mostro::{
    admin_keys, admin_add_solver, ensure_node_relays_connected, resolve_effective_pow,
    send_mostro_message, try_get_node_config, SolverPermission,
};
use dioxus::prelude::*;
use nostr::prelude::*;

/// Register a new dispute solver on the Mostro daemon.
#[component]
pub fn MostroAdminSolvers() -> Element {
    let admin_keys_loaded = admin_keys::try_get().is_some();
    let nav = navigator();
    let mut npub_input = use_signal(String::new);
    let mut read_only = use_signal(|| false);
    let mut sending = use_signal(|| false);
    let mut error_msg = use_signal::<Option<String>>(|| None);
    let mut success_msg = use_signal::<Option<String>>(|| None);

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
                        "Configure your solver nsec in Settings → P2P to access the admin interface."
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

    let on_submit = move |_| {
        let npub = npub_input.read().trim().to_string();
        if npub.is_empty() {
            error_msg.set(Some("Enter a solver npub".to_string()));
            return;
        }
        let permission = if *read_only.read() {
            SolverPermission::ReadOnly
        } else {
            SolverPermission::ReadWrite
        };

        let admin_keys = match admin_keys::try_get() {
            Some(k) => k,
            None => {
                error_msg.set(Some("Admin keys not loaded".to_string()));
                return;
            }
        };
        let node = match try_get_node_config() {
            Some(n) => n,
            None => {
                error_msg.set(Some("No daemon configured".to_string()));
                return;
            }
        };
        let node_pk = match PublicKey::from_hex(&node.pubkey)
            .or_else(|_| PublicKey::from_bech32(&node.pubkey))
        {
            Ok(pk) => pk,
            Err(e) => {
                error_msg.set(Some(format!("Invalid daemon pubkey: {e}")));
                return;
            }
        };
        let keys = admin_keys.keys.clone();

        sending.set(true);
        error_msg.set(None);
        success_msg.set(None);

        spawn(async move {
            ensure_node_relays_connected().await;

            let message = admin_add_solver(npub, permission);
            let pow = resolve_effective_pow(&node, node_pk).await;

            if let Err(e) = send_mostro_message(
                &message, &keys, &keys, node_pk, &node.relays, pow,
            )
            .await
            {
                error_msg.set(Some(format!("Failed to add solver: {e}")));
            } else {
                success_msg.set(Some("Solver registration sent. The daemon will confirm.".to_string()));
                npub_input.set(String::new());
            }
            sending.set(false);
        });
    };

    rsx! {
        div { class: "min-h-screen p-4 max-w-3xl mx-auto space-y-4",
            // Header with back button
            div { class: "flex items-center gap-3",
                button {
                    class: "p-2 hover:bg-accent rounded-lg",
                    onclick: move |_| { let _ = nav.push(Route::MostroAdminDisputes {}); },
                    crate::components::icons::ArrowLeftIcon { class: "w-5 h-5".to_string() }
                }
                h1 { class: "text-xl font-bold", "Add Dispute Solver" }
            }

            p { class: "text-sm text-muted-foreground",
                "Register a new dispute solver. The solver will be able to take and resolve \
                 disputes on this Mostro daemon."
            }

            // Success message
            if let Some(msg) = success_msg.read().as_ref() {
                div { class: "p-3 bg-green-500/10 border border-green-500/30 rounded-lg text-sm text-green-500",
                    "{msg}"
                }
            }

            // Error message
            if let Some(msg) = error_msg.read().as_ref() {
                div { class: "p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-500",
                    "{msg}"
                }
            }

            // Form
            div { class: "space-y-3 bg-card border border-border rounded-lg p-4",
                div { class: "space-y-1",
                    label { class: "text-sm font-medium", "Solver npub" }
                    input {
                        class: "w-full px-3 py-2 bg-background border border-border rounded-lg \
                               text-sm focus:outline-none focus:ring-1 focus:ring-primary",
                        r#type: "text",
                        placeholder: "npub1...",
                        value: "{npub_input}",
                        oninput: move |e| npub_input.set(e.value()),
                        disabled: *sending.read(),
                    }
                }

                div { class: "space-y-1",
                    label { class: "text-sm font-medium", "Permission level" }
                    div { class: "flex gap-2",
                        button {
                            class: if !*read_only.read() {
                                "flex-1 px-3 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium"
                            } else {
                                "flex-1 px-3 py-2 border border-border rounded-lg text-sm text-muted-foreground"
                            },
                            onclick: move |_| read_only.set(false),
                            disabled: *sending.read(),
                            "Read-Write"
                        }
                        button {
                            class: if *read_only.read() {
                                "flex-1 px-3 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium"
                            } else {
                                "flex-1 px-3 py-2 border border-border rounded-lg text-sm text-muted-foreground"
                            },
                            onclick: move |_| read_only.set(true),
                            disabled: *sending.read(),
                            "Read-Only"
                        }
                    }
                    p { class: "text-xs text-muted-foreground",
                        if *read_only.read() {
                            "Read-only solvers can view disputes and chat with parties but cannot settle or cancel."
                        } else {
                            "Read-write solvers can take disputes, settle, and cancel trades."
                        }
                    }
                }

                button {
                    class: "w-full px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm \
                           font-medium disabled:opacity-50",
                    onclick: on_submit,
                    disabled: *sending.read() || npub_input.read().trim().is_empty(),
                    if *sending.read() { "Adding..." } else { "Add Solver" }
                }
            }
        }
    }
}
