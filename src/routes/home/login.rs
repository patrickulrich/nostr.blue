use crate::routes::Route;
use crate::stores::auth_store;
use crate::utils::nip49;
use dioxus::prelude::*;
use nostr::ToBech32;

use super::types::Nip55State;

#[component]
pub fn HelpModal(on_close: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-40 bg-black/50 backdrop-blur-sm flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card rounded-lg shadow-xl max-w-2xl w-full max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                div { class: "sticky top-0 bg-card border-b border-border px-6 py-4 flex items-center justify-between",
                    h3 { class: "text-xl font-bold text-foreground",
                        "About Nostr Sign-In Methods"
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground text-2xl",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }
                div { class: "px-6 py-4 space-y-6",
                    div {
                        h4 { class: "font-semibold text-foreground mb-2",
                            "What is Nostr?"
                        }
                        p { class: "text-sm text-muted-foreground",
                            "Nostr is a decentralized social protocol where you own your identity and data. Instead of relying on a company, your identity is based on cryptographic keys that only you control."
                        }
                    }
                    div {
                        h4 { class: "font-semibold text-foreground mb-2 flex items-center gap-2",
                            "🔌 Browser Extension (NIP-07)"
                            span { class: "px-2 py-0.5 text-xs bg-primary text-primary-foreground rounded-full",
                                "RECOMMENDED"
                            }
                        }
                        p { class: "text-sm text-muted-foreground mb-2",
                            "Browser extensions like Alby, nos2x, and Flamingo store your keys securely and sign events on your behalf. Your private key never leaves the extension."
                        }
                        ul { class: "text-sm text-muted-foreground list-disc list-inside space-y-1",
                            li { "Keys stored securely in the extension" }
                            li { "Websites can't access your private key" }
                            li { "Works across all Nostr apps" }
                            li { "You control which actions to approve" }
                        }
                    }
                    div {
                        h4 { class: "font-semibold text-foreground mb-2 flex items-center gap-2",
                            "🔐 Remote Signer (NIP-46)"
                            span { class: "px-2 py-0.5 text-xs bg-primary text-primary-foreground rounded-full",
                                "RECOMMENDED"
                            }
                        }
                        p { class: "text-sm text-muted-foreground mb-2",
                            "Remote signers let you keep your keys on a separate device (like your phone with Amber) or a dedicated service (like nsecBunker). This app connects to your signer and requests signatures remotely."
                        }
                        ul { class: "text-sm text-muted-foreground list-disc list-inside space-y-1",
                            li { "Keys stay on your signing device" }
                            li { "Approve each action on your phone" }
                            li { "Compatible signers: Amber (Android), nsecBunker" }
                            li { "Most secure for untrusted devices" }
                        }
                        p { class: "text-xs text-primary mt-2",
                            "To use: Get a bunker:// URI from your signing app and paste it above."
                        }
                    }
                    div {
                        h4 { class: "font-semibold text-foreground mb-2 flex items-center gap-2",
                            "🔑 Private Key (nsec)"
                            span { class: "px-2 py-0.5 text-xs bg-accent text-accent-foreground rounded-full",
                                "USE WITH CAUTION"
                            }
                        }
                        p { class: "text-sm text-muted-foreground mb-2",
                            "Entering your private key (nsec) directly gives this app full access to your account. Your key is stored in browser localStorage."
                        }
                        ul { class: "text-sm text-muted-foreground list-disc list-inside space-y-1",
                            li { "⚠️ Only use on devices you fully trust" }
                            li { "⚠️ Never share your nsec with anyone" }
                            li { "⚠️ Stored in browser (cleared if you clear data)" }
                            li { "Can be compromised if device is compromised" }
                        }
                    }
                    div {
                        h4 { class: "font-semibold text-foreground mb-2",
                            "👁️ Public Key (npub) - Read Only"
                        }
                        p { class: "text-sm text-muted-foreground",
                            "Using just your public key (npub) lets you browse and view content, but you cannot post, react, or send messages. Perfect for exploring Nostr without committing."
                        }
                    }
                    div {
                        h4 { class: "font-semibold text-foreground mb-2",
                            "🛡️ Security Best Practices"
                        }
                        ul { class: "text-sm text-muted-foreground list-disc list-inside space-y-1",
                            li { "Always prefer browser extensions or remote signers" }
                            li { "Never enter your nsec on untrusted websites" }
                            li { "Backup your keys securely (offline)" }
                            li { "Use different keys for testing and main account" }
                        }
                    }
                }
                div { class: "sticky bottom-0 bg-muted border-t border-border px-6 py-4",
                    button {
                        class: "w-full px-4 py-2 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition",
                        onclick: move |_| on_close.call(()),
                        "Got it!"
                    }
                }
            }
        }
    }
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
fn google_sign_in_ui() -> Element {
    rsx! { GoogleSignInCard {} }
}

#[cfg(not(any(target_family = "wasm", feature = "mobile_platform")))]
fn google_sign_in_ui() -> Element {
    rsx! {}
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
#[component]
fn GoogleSignInCard() -> Element {
    use crate::services::cloud_backup::GoogleBackupState;
    use crate::stores::auth_store::GOOGLE_BACKUP_STATE;

    let mut import_nsec = use_signal(String::new);
    let mut mnemonic_ack = use_signal(|| false);
    let state = GOOGLE_BACKUP_STATE.read().clone();

    let reset = move |_| {
        import_nsec.set(String::new());
        mnemonic_ack.set(false);
        crate::stores::auth_store::reset_google_backup_state();
    };

    rsx! {
        div { class: "mb-6 p-4 bg-accent/50 rounded-lg border-2 border-primary/50",
            match state {
                GoogleBackupState::Idle => rsx! {
                    div { class: "flex items-start gap-3 mb-3",
                        div { class: "text-2xl", "☁️" }
                        div { class: "flex-1",
                            span { class: "font-semibold text-foreground block mb-1",
                                "Sign in with Google"
                            }
                            p { class: "text-sm text-muted-foreground",
                                "Back up and restore your Nostr keys with Google Drive"
                            }
                        }
                    }
                    button {
                        class: "w-full px-4 py-2.5 bg-white hover:bg-gray-50 text-gray-800 rounded-lg font-medium transition shadow-xs border border-gray-300 flex items-center justify-center gap-2",
                        onclick: move |_| {
                            spawn(async move {
                                crate::stores::auth_store::start_google_sign_in().await;
                            });
                        },
                        svg {
                            class: "w-5 h-5",
                            view_box: "0 0 24 24",
                            path { d: "M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 0 1-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z", fill: "#4285F4" }
                            path { d: "M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z", fill: "#34A853" }
                            path { d: "M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z", fill: "#FBBC05" }
                            path { d: "M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z", fill: "#EA4335" }
                        }
                        "Sign in with Google"
                    }
                },

                GoogleBackupState::SigningIn => rsx! {
                    div { class: "flex items-center gap-3",
                        span { class: "inline-block w-5 h-5 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        span { class: "text-foreground", "Signing in with Google..." }
                    }
                    button {
                        class: "mt-3 text-sm text-muted-foreground hover:text-foreground transition",
                        onclick: reset,
                        "Cancel"
                    }
                },

                GoogleBackupState::CheckingDrive => rsx! {
                    div { class: "flex items-center gap-3",
                        span { class: "inline-block w-5 h-5 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        span { class: "text-foreground", "Checking for backups..." }
                    }
                    button {
                        class: "mt-3 text-sm text-muted-foreground hover:text-foreground transition",
                        onclick: reset,
                        "Cancel"
                    }
                },

                GoogleBackupState::Choose { entries, .. } => rsx! {
                    div { class: "flex items-start gap-3 mb-3",
                        div { class: "text-2xl", "☁️" }
                        div { class: "flex-1",
                            span { class: "font-semibold text-foreground block mb-1",
                                "Choose Account"
                            }
                            p { class: "text-sm text-muted-foreground",
                                "Found {entries.len()} account(s) in your Google Drive"
                            }
                        }
                    }
                    div { class: "space-y-2 mb-3",
                        {
                            let items: Vec<(String, String, Option<String>, String)> = entries
                                .into_iter()
                                .map(|e| {
                                    let name = e.display_name.clone().unwrap_or_else(|| {
                                        let n = &e.npub;
                                        if n.len() > 16 {
                                            format!("{}...", &n[..16])
                                        } else {
                                            n.clone()
                                        }
                                    });
                                    (e.file_id.clone(), name, e.picture.clone(), e.npub.clone())
                                })
                                .collect();
                            rsx! {
                                for (file_id, name, pic, npub) in items {
                                    {
                                        let fid = file_id.clone();
                                        let initial = name.chars().next().unwrap_or('?').to_string();
                                        rsx! {
                                            button {
                                                key: "{fid}",
                                                class: "w-full flex items-center gap-3 p-3 bg-card hover:bg-accent rounded-lg border border-border transition text-left",
                                                onclick: move |_| {
                                                    let id = fid.clone();
                                                    spawn(async move {
                                                        crate::stores::auth_store::restore_google_backup(&id).await;
                                                    });
                                                },
                                                if let Some(url) = &pic {
                                                    img { class: "w-10 h-10 rounded-full object-cover shrink-0", src: "{url}" }
                                                } else {
                                                    div { class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center text-muted-foreground font-medium text-sm shrink-0",
                                                        "{initial}"
                                                    }
                                                }
                                                div { class: "flex-1 min-w-0",
                                                    p { class: "text-sm text-foreground truncate", "{name}" }
                                                    p { class: "text-xs text-muted-foreground truncate", "{npub}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "space-y-2 mt-3 pt-3 border-t border-border",
                        button {
                            class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition shadow-xs",
                            onclick: move |_| {
                                spawn(async move {
                                    crate::stores::auth_store::create_key_with_google().await;
                                });
                            },
                            "Create New Account"
                        }
                        button {
                            class: "w-full px-4 py-2.5 bg-accent hover:bg-accent/80 text-accent-foreground rounded-lg font-medium transition",
                            onclick: move |_| {
                                let st = GOOGLE_BACKUP_STATE.read().clone();
                                if let GoogleBackupState::Choose { auth, .. } = st {
                                    *GOOGLE_BACKUP_STATE.write() = GoogleBackupState::ImportKey {
                                        auth,
                                        nsec_input: String::new(),
                                        error: None,
                                    };
                                }
                            },
                            "Import Existing Key"
                        }
                    }
                    button {
                        class: "mt-3 text-sm text-muted-foreground hover:text-foreground transition",
                        onclick: reset,
                        "← Back"
                    }
                },

                GoogleBackupState::NoBackup(_) => rsx! {
                    div { class: "flex items-start gap-3 mb-3",
                        div { class: "text-2xl", "☁️" }
                        div { class: "flex-1",
                            span { class: "font-semibold text-foreground block mb-1",
                                "No Backups Found"
                            }
                            p { class: "text-sm text-muted-foreground",
                                "No Nostr keys found in your Google Drive."
                            }
                        }
                    }
                    div { class: "space-y-2",
                        button {
                            class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition shadow-xs",
                            onclick: move |_| {
                                spawn(async move {
                                    crate::stores::auth_store::create_key_with_google().await;
                                });
                            },
                            "Create New Account"
                        }
                        button {
                            class: "w-full px-4 py-2.5 bg-accent hover:bg-accent/80 text-accent-foreground rounded-lg font-medium transition",
                            onclick: move |_| {
                                let st = GOOGLE_BACKUP_STATE.read().clone();
                                if let GoogleBackupState::NoBackup(auth) = st {
                                    *GOOGLE_BACKUP_STATE.write() = GoogleBackupState::ImportKey {
                                        auth,
                                        nsec_input: String::new(),
                                        error: None,
                                    };
                                }
                            },
                            "Import Existing Key"
                        }
                    }
                    button {
                        class: "mt-3 text-sm text-muted-foreground hover:text-foreground transition",
                        onclick: reset,
                        "← Back"
                    }
                },

                GoogleBackupState::ImportKey { .. } => rsx! {
                    div { class: "mb-3",
                        span { class: "font-semibold text-foreground block mb-1",
                            "Import Existing Key"
                        }
                        p { class: "text-sm text-muted-foreground",
                            "Enter your private key (nsec) to back it up and sign in."
                        }
                    }
                    input {
                        class: "w-full px-3 py-2 text-sm border border-border rounded-lg bg-card text-foreground mb-2",
                        r#type: "password",
                        placeholder: "nsec1...",
                        value: "{import_nsec}",
                        oninput: move |evt| import_nsec.set(evt.value()),
                    }
                    div { class: "flex gap-2",
                        button {
                            class: "flex-1 px-4 py-2 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50",
                            onclick: move |_| {
                                let nsec = import_nsec.read().clone();
                                spawn(async move {
                                    crate::stores::auth_store::import_key_to_google(&nsec).await;
                                });
                            },
                            disabled: import_nsec.read().is_empty(),
                            "Import & Sign In"
                        }
                        button {
                            class: "px-4 py-2 bg-accent hover:bg-accent/80 text-accent-foreground rounded-lg transition",
                            onclick: reset,
                            "Cancel"
                        }
                    }
                },

                GoogleBackupState::ShowMnemonic { words, .. } => rsx! {
                    div { class: "mb-3",
                        span { class: "font-semibold text-foreground block mb-1",
                            "Your Recovery Phrase"
                        }
                        p { class: "text-sm text-muted-foreground",
                            "Write down these 12 words in order and keep them safe."
                        }
                    }
                    div { class: "p-3 bg-card border border-border rounded-lg mb-3",
                        {
                            let word_pairs: Vec<(usize, String)> = words
                                .split(' ')
                                .enumerate()
                                .map(|(i, w)| (i + 1, w.to_string()))
                                .collect();
                            rsx! {
                                div { class: "grid grid-cols-3 gap-2",
                                    for (num, word) in word_pairs {
                                        div { key: "{word}",
                                            class: "flex items-center gap-2 text-sm",
                                            span { class: "text-muted-foreground text-xs w-5 text-right",
                                                "{num}"
                                            }
                                            span { class: "text-foreground font-medium", "{word}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    label { class: "flex items-center gap-2 mb-3 cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: *mnemonic_ack.read(),
                            onchange: move |_| {
                                let current = *mnemonic_ack.read();
                                mnemonic_ack.set(!current);
                            },
                        }
                        span { class: "text-sm text-muted-foreground",
                            "I have written down my recovery phrase"
                        }
                    }
                    div { class: "flex gap-2",
                        button {
                            class: "flex-1 px-4 py-2 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50",
                            onclick: move |_| {
                                spawn(async move {
                                    crate::stores::auth_store::confirm_mnemonic_and_create().await;
                                });
                            },
                            disabled: !*mnemonic_ack.read(),
                            "Create Account"
                        }
                        button {
                            class: "px-4 py-2 bg-accent hover:bg-accent/80 text-accent-foreground rounded-lg transition",
                            onclick: reset,
                            "Cancel"
                        }
                    }
                },

                GoogleBackupState::Working => rsx! {
                    div { class: "flex items-center gap-3",
                        span { class: "inline-block w-5 h-5 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        span { class: "text-foreground", "Working..." }
                    }
                },

                GoogleBackupState::Done { is_new_account } => rsx! {
                    div { class: "text-center py-2",
                        div { class: "text-3xl mb-2",
                            if is_new_account { "🎉" } else { "✓" }
                        }
                        p { class: "text-foreground font-medium",
                            if is_new_account { "Account Created!" } else { "Account Restored!" }
                        }
                        p { class: "text-sm text-muted-foreground mt-1", "Redirecting..." }
                    }
                },

                GoogleBackupState::Error(msg) => rsx! {
                    div { class: "p-3 bg-destructive/10 border border-destructive/30 rounded-lg mb-3",
                        p { class: "text-sm text-destructive", "{msg}" }
                    }
                    button {
                        class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition",
                        onclick: reset,
                        "Try Again"
                    }
                },
            }
        }
    }
}

#[component]
pub fn LoginSection() -> Element {
    let mut nsec_input = use_signal(String::new);
    let mut npub_input = use_signal(String::new);
    let mut bunker_uri_input = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut show_advanced = use_signal(|| false);
    let mut show_help_modal = use_signal(|| false);
    let mut connecting_bunker = use_signal(|| false);
    let mut nsec_password = use_signal(String::new);
    let mut nsec_confirm_password = use_signal(String::new);
    let mut show_nsec_password = use_signal(|| false);
    let login_with_nsec = move |_| {
        let nsec = nsec_input.read().clone();
        let password = nsec_password.read().clone();
        let confirm = nsec_confirm_password.read().clone();
        if password != confirm {
            error.set(Some("Passwords do not match".to_string()));
            return;
        }
        if let Some(err) = nip49::validate_password(&password) {
            error.set(Some(err));
            return;
        }
        spawn(async move {
            match auth_store::login_with_nsec(&nsec, &password).await {
                Ok(_) => error.set(None),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    let login_with_npub = move |_| {
        let npub = npub_input.read().clone();
        spawn(async move {
            match auth_store::login_with_npub(&npub).await {
                Ok(_) => error.set(None),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    let login_with_bunker = move |_| {
        let uri = bunker_uri_input.read().clone();
        connecting_bunker.set(true);
        error.set(None);
        spawn(async move {
            match auth_store::login_with_nostr_connect(&uri).await {
                Ok(_) => {
                    bunker_uri_input.set(String::new());
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            connecting_bunker.set(false);
        });
    };
    let generate_new = move |_| {
        let keys = auth_store::generate_keys();
        let nsec = keys.secret_key().to_bech32().unwrap();
        nsec_input.set(nsec);
    };
    let login_with_extension = move |_| {
        spawn(async move {
            match auth_store::login_with_browser_extension().await {
                Ok(_) => error.set(None),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    let has_extension = auth_store::is_browser_extension_available();
    let has_android_signer = auth_store::is_android_signer_available();
    let mut nip55_state = use_signal(|| Nip55State::Idle);
    let nip55_connect = move |_| {
        error.set(None);
        nip55_state.set(Nip55State::Checking);
        spawn(async move {
            match auth_store::login_with_android_signer_auto().await {
                Ok(result) => match result {
                    auth_store::AndroidSignerAutoResult::LoggedIn(package) => {
                        log::info!("NIP-55: auto-connected to signer: {}", package);
                        error.set(None);
                        nip55_state.set(Nip55State::Idle);
                    }
                    auth_store::AndroidSignerAutoResult::Error(e) => {
                        log::error!("NIP-55: auto-detect error: {}", e);
                        nip55_state.set(Nip55State::Error(e));
                    }
                },
                Err(e) => {
                    log::error!("NIP-55: auto-detect failed: {}", e);
                    nip55_state.set(Nip55State::Error(e));
                }
            }
        });
    };
    let nip55_retry = move |_| {
        error.set(None);
        nip55_state.set(Nip55State::Idle);
    };
    rsx! {
        div { class: "p-6 max-w-lg mx-auto",
            div { class: "flex items-center justify-between mb-6",
                h3 { class: "text-2xl font-bold text-foreground", "Welcome to Nostr" }
                button {
                    class: "px-3 py-1.5 text-sm bg-accent text-accent-foreground hover:bg-accent/80 rounded-lg transition",
                    onclick: move |_| show_help_modal.set(true),
                    "Learn More"
                }
            }
            p { class: "text-muted-foreground mb-6",
                "Choose a secure sign-in method to get started with the decentralized social network."
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "mb-4 p-3 bg-destructive/10 text-destructive rounded-lg text-sm",
                    "❌ {err}"
                }
            }
            div { class: "mb-6",
                h4 { class: "text-sm font-semibold text-muted-foreground uppercase tracking-wide mb-3",
                    "Recommended (Secure)"
                }
                div { class: "space-y-3",
                    if has_extension {
                        div { class: "p-4 bg-accent/50 rounded-lg border-2 border-primary/50",
                            div { class: "flex items-start gap-3 mb-3",
                                div { class: "text-2xl", "🔌" }
                                div { class: "flex-1",
                                    div { class: "flex items-center gap-2 mb-1",
                                        span { class: "font-semibold text-foreground",
                                            "Browser Extension"
                                        }
                                        span { class: "px-2 py-0.5 text-xs bg-primary text-primary-foreground rounded-full",
                                            "RECOMMENDED"
                                        }
                                    }
                                    p { class: "text-sm text-muted-foreground",
                                        "Your keys stay in the extension, never exposed to websites."
                                    }
                                }
                            }
                            button {
                                class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition shadow-xs",
                                onclick: login_with_extension,
                                "Connect Extension"
                            }
                        }
                    }
                    if has_android_signer {
                        div { class: "p-4 bg-accent/50 rounded-lg border-2 border-primary/50",
                            div { class: "flex items-start gap-3 mb-3",
                                div { class: "text-2xl", "📱" }
                                div { class: "flex-1",
                                    div { class: "flex items-center gap-2 mb-1",
                                        span { class: "font-semibold text-foreground",
                                            "Android Signer (NIP-55)"
                                        }
                                        span { class: "px-2 py-0.5 text-xs bg-primary text-primary-foreground rounded-full",
                                            "RECOMMENDED"
                                        }
                                    }
                                    p { class: "text-sm text-muted-foreground",
                                        "Use Amber or another NIP-55 signer app. Your keys never leave the signer."
                                    }
                                }
                            }
                            match &*nip55_state.read() {
                                Nip55State::Idle => rsx! {
                                    button {
                                        class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition shadow-xs",
                                        onclick: nip55_connect,
                                        "Connect Signer"
                                    }
                                },
                                Nip55State::Checking => rsx! {
                                    button {
                                        class: "w-full px-4 py-2.5 bg-primary/70 text-primary-foreground rounded-lg font-medium transition shadow-xs cursor-not-allowed",
                                        disabled: true,
                                        "Connecting..."
                                    }
                                },
                                Nip55State::Error(msg) => rsx! {
                                    div { class: "space-y-3",
                                        div { class: "p-3 bg-destructive/10 border border-destructive/30 rounded-lg",
                                            p { class: "text-sm text-destructive", "{msg}" }
                                        }
                                        button {
                                            class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition shadow-xs",
                                            onclick: nip55_retry,
                                            "Try Again"
                                        }
                                    }
                                },
                            }
                        }
                    }
                    div { class: "p-4 bg-accent/50 rounded-lg border-2 border-primary/50",
                        div { class: "flex items-start gap-3 mb-3",
                            div { class: "text-2xl", "🔐" }
                            div { class: "flex-1",
                                div { class: "flex items-center gap-2 mb-1",
                                    span { class: "font-semibold text-foreground",
                                        "Remote Signer"
                                    }
                                    span { class: "px-2 py-0.5 text-xs bg-primary text-primary-foreground rounded-full",
                                        "RECOMMENDED"
                                    }
                                }
                                p { class: "text-sm text-muted-foreground",
                                    "Use Amber, nsecBunker, or other NIP-46 signers. Keys never leave your device."
                                }
                            }
                        }
                        div { class: "space-y-2",
                            input {
                                class: "w-full px-3 py-2 text-sm border border-primary/50 rounded-lg bg-card text-foreground focus:ring-2 focus:ring-primary focus:border-transparent",
                                r#type: "text",
                                placeholder: "bunker://...",
                                value: "{bunker_uri_input}",
                                oninput: move |evt| bunker_uri_input.set(evt.value()),
                                disabled: *connecting_bunker.read(),
                            }
                            button {
                                class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition shadow-xs disabled:opacity-50 disabled:cursor-not-allowed",
                                onclick: login_with_bunker,
                                disabled: bunker_uri_input.read().is_empty() || *connecting_bunker.read(),
                                if *connecting_bunker.read() {
                                    "Connecting..."
                                } else {
                                    "Connect Remote Signer"
                                }
                            }
                            if *connecting_bunker.read() {
                                p { class: "text-xs text-primary text-center",
                                    "Waiting for approval on your signing device (up to 2 minutes)..."
                                }
                            }
                        }
                    }
                }
            }
            {google_sign_in_ui()}
            div { class: "border-t border-border pt-6",
                button {
                    class: "w-full flex items-center justify-between p-3 bg-muted hover:bg-accent rounded-lg transition",
                    onclick: move |_| {
                        let current = *show_advanced.read();
                        show_advanced.set(!current);
                    },
                    div { class: "flex items-center gap-2",
                        span { class: "text-muted-foreground", "⚠️" }
                        span { class: "font-medium text-foreground", "Advanced Options" }
                    }
                    span { class: "text-muted-foreground",
                        if *show_advanced.read() {
                            "▼"
                        } else {
                            "▶"
                        }
                    }
                }
                if *show_advanced.read() {
                    div { class: "mt-4 p-4 bg-accent/30 border border-border rounded-lg space-y-4",
                        div { class: "p-3 bg-accent/50 rounded-lg",
                            p { class: "text-sm text-foreground font-medium",
                                "⚠️ Security Warning"
                            }
                            p { class: "text-xs text-muted-foreground mt-1",
                                "These methods store keys in your browser. Only use on devices you fully trust."
                            }
                        }
                        div {
                            h5 { class: "font-medium text-foreground mb-2 text-sm",
                                "🔑 Private Key (nsec)"
                            }
                            div { class: "space-y-2",
                                input {
                                    class: "w-full px-3 py-2 text-sm border border-border rounded-lg bg-card text-foreground",
                                    r#type: "password",
                                    placeholder: "nsec1...",
                                    value: "{nsec_input}",
                                    oninput: move |evt| nsec_input.set(evt.value()),
                                }
                                div { class: "relative",
                                    input {
                                        class: "w-full px-3 py-2 text-sm border border-border rounded-lg bg-card text-foreground pr-10",
                                        r#type: if *show_nsec_password.read() { "text" } else { "password" },
                                        placeholder: "Set encryption password",
                                        value: "{nsec_password}",
                                        oninput: move |evt| nsec_password.set(evt.value()),
                                    }
                                    button {
                                        class: "absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground",
                                        r#type: "button",
                                        onclick: move |_| {
                                            let current = *show_nsec_password.read();
                                            show_nsec_password.set(!current);
                                        },
                                        if *show_nsec_password.read() {
                                            "🙈"
                                        } else {
                                            "👁️"
                                        }
                                    }
                                }
                                input {
                                    class: "w-full px-3 py-2 text-sm border border-border rounded-lg bg-card text-foreground",
                                    r#type: if *show_nsec_password.read() { "text" } else { "password" },
                                    placeholder: "Confirm password",
                                    value: "{nsec_confirm_password}",
                                    oninput: move |evt| nsec_confirm_password.set(evt.value()),
                                }
                                p { class: "text-xs text-muted-foreground",
                                    "🔐 Your key will be encrypted with this password"
                                }
                                div { class: "flex gap-2",
                                    button {
                                        class: "flex-1 px-3 py-2 text-sm bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg transition disabled:opacity-50 disabled:cursor-not-allowed",
                                        onclick: login_with_nsec,
                                        disabled: nsec_input.read().is_empty() || nsec_password.read().is_empty()
                                            || nsec_confirm_password.read().is_empty(),
                                        "Login"
                                    }
                                    button {
                                        class: "px-3 py-2 text-sm bg-accent hover:bg-accent/80 text-accent-foreground rounded-lg transition",
                                        onclick: generate_new,
                                        "Generate"
                                    }
                                }
                            }
                        }
                        div {
                            h5 { class: "font-medium text-foreground mb-2 text-sm",
                                "👁️ Public Key (npub) - Read Only"
                            }
                            div { class: "space-y-2",
                                input {
                                    class: "w-full px-3 py-2 text-sm border border-border rounded-lg bg-card text-foreground",
                                    r#type: "text",
                                    placeholder: "npub1...",
                                    value: "{npub_input}",
                                    oninput: move |evt| npub_input.set(evt.value()),
                                }
                                button {
                                    class: "w-full px-3 py-2 text-sm bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg transition",
                                    onclick: login_with_npub,
                                    "View Profile (Read-Only)"
                                }
                                p { class: "text-xs text-muted-foreground",
                                    "ℹ️ You can browse but cannot post or interact."
                                }
                            }
                        }
                    }
                }
            }
            if *show_help_modal.read() {
                HelpModal { on_close: move |_| show_help_modal.set(false) }
            }
        }
    }
}

#[component]
pub fn ProfileSection() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut logout_error = use_signal(|| None::<String>);
    rsx! {
        div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
            div { class: "flex justify-between items-start mb-4",
                h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                    "👤 Your Profile"
                }
                button {
                    class: "px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg font-medium transition",
                    onclick: move |_| {
                        let nav = navigator();
                        spawn(async move {
                            match auth_store::logout().await {
                                Ok(()) => {
                                    logout_error.set(None);
                                    nav.push(Route::Home { list: String::new() });
                                }
                                Err(e) => {
                                    log::error!("{}", e);
                                    logout_error.set(Some(e));
                                }
                            }
                        });
                    },
                    "Logout"
                }
            }
            if let Some(error) = logout_error.read().as_ref() {
                p { class: "mb-4 rounded-lg bg-red-100 p-3 text-sm text-red-700 dark:bg-red-900/30 dark:text-red-300",
                    "{error}"
                }
            }
            div { class: "space-y-3",
                div { class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                    p { class: "text-sm text-gray-600 dark:text-gray-400 mb-1", "Public Key" }
                    if let Some(pubkey) = &auth.pubkey {
                        Link {
                            to: Route::AddressViewer {
                                address: crate::utils::nip19_urls::profile_route_id(pubkey),
                            },
                            class: "font-mono text-sm text-blue-600 dark:text-blue-400 hover:underline break-all",
                            "{pubkey}"
                        }
                    }
                }
                div { class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                    p { class: "text-sm text-gray-600 dark:text-gray-400 mb-1", "Login Method" }
                    p { class: "text-gray-900 dark:text-white",
                        match auth.login_method {
                            Some(auth_store::LoginMethod::PrivateKey) => "🔑 Private Key",
                            Some(auth_store::LoginMethod::ReadOnly) => "👁️ Read-Only",
                            Some(auth_store::LoginMethod::BrowserExtension) => "🔌 Browser Extension",
                            Some(auth_store::LoginMethod::RemoteSigner) => "🔐 Remote Signer",
                            #[cfg(feature = "mobile_platform")]
                            Some(auth_store::LoginMethod::AndroidSigner) => "📱 Android Signer",
                            None => "Unknown",
                        }
                    }
                }
            }
        }
    }
}
