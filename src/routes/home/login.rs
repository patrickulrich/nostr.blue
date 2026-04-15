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
                    auth_store::AndroidSignerAutoResult::IntentLaunched => {
                        log::info!("NIP-55: Intent launched, waiting for user approval");
                        error.set(None);
                        nip55_state.set(Nip55State::WaitingForApproval);
                    }
                    auth_store::AndroidSignerAutoResult::IntentInFlight => {
                        log::info!("NIP-55: Intent already in flight");
                        error.set(None);
                        nip55_state.set(Nip55State::WaitingForApproval);
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
    let nip55_poll_and_connect = move |_| {
        error.set(None);
        nip55_state.set(Nip55State::Checking);
        spawn(async move {
            match auth_store::login_with_android_signer_auto().await {
                Ok(result) => match result {
                    auth_store::AndroidSignerAutoResult::LoggedIn(package) => {
                        log::info!("NIP-55: connected after approval: {}", package);
                        error.set(None);
                        nip55_state.set(Nip55State::Idle);
                    }
                    auth_store::AndroidSignerAutoResult::IntentInFlight => {
                        log::info!("NIP-55: still waiting for approval");
                        error.set(None);
                        nip55_state.set(Nip55State::WaitingForApproval);
                    }
                    auth_store::AndroidSignerAutoResult::IntentLaunched => {
                        error.set(None);
                        nip55_state.set(Nip55State::WaitingForApproval);
                    }
                    auth_store::AndroidSignerAutoResult::Error(e) => {
                        nip55_state.set(Nip55State::Error(e));
                    }
                },
                Err(e) => {
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
                                        "Checking..."
                                    }
                                },
                                Nip55State::WaitingForApproval => rsx! {
                                    div { class: "space-y-3",
                                        div { class: "p-3 bg-blue-500/10 border border-blue-500/30 rounded-lg",
                                            p { class: "text-sm text-foreground font-medium mb-1",
                                                "Approve in your signer app"
                                            }
                                            p { class: "text-xs text-muted-foreground",
                                                "Open Amber and approve the connection request, then come back and tap below."
                                            }
                                        }
                                        button {
                                            class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition shadow-xs",
                                            onclick: nip55_poll_and_connect,
                                            "I've Approved — Connect"
                                        }
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
                            to: Route::Profile {
                                pubkey: pubkey.clone(),
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
                            #[cfg(feature = "mobile")]
                            Some(auth_store::LoginMethod::AndroidSigner) => "📱 Android Signer",
                            None => "Unknown",
                        }
                    }
                }
            }
        }
    }
}
