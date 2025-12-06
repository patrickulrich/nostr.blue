use dioxus::prelude::*;
use crate::routes::Route;
use nostr_sdk::{PublicKey, EventId, FromBech32};

#[component]
pub fn Nip19Handler(identifier: String) -> Element {
    let mut error_msg = use_signal(|| None::<String>);
    let mut processing = use_signal(|| true);

    // Clone identifier for use in different contexts
    let identifier_for_effect = identifier.clone();
    let identifier_for_display = identifier.clone();

    // Decode and redirect on mount
    use_effect(move || {
        let id = identifier_for_effect.clone();

        spawn(async move {
            match decode_and_redirect(&id).await {
                Ok(route) => {
                    // Navigate to the appropriate route
                    navigator().push(route);
                }
                Err(e) => {
                    error_msg.set(Some(e));
                    processing.set(false);
                }
            }
        });
    });

    rsx! {
        div {
            class: "min-h-screen flex items-center justify-center p-4",

            if *processing.read() {
                div {
                    class: "text-center",
                    div {
                        class: "text-4xl mb-4 animate-spin",
                        "🔄"
                    }
                    h2 {
                        class: "text-xl font-semibold mb-2",
                        "Processing identifier..."
                    }
                    p {
                        class: "text-muted-foreground text-sm font-mono break-all",
                        "{identifier_for_display}"
                    }
                }
            } else if let Some(err) = error_msg.read().as_ref() {
                div {
                    class: "text-center max-w-md",
                    div {
                        class: "text-6xl mb-4",
                        "❌"
                    }
                    h2 {
                        class: "text-2xl font-bold mb-4",
                        "Invalid Identifier"
                    }
                    p {
                        class: "text-muted-foreground mb-4",
                        "{err}"
                    }
                    div {
                        class: "p-3 bg-muted rounded-lg mb-6",
                        p {
                            class: "text-xs font-mono break-all",
                            "{identifier_for_display}"
                        }
                    }
                    Link {
                        to: Route::Home {},
                        class: "inline-block px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                        "← Go Home"
                    }
                }
            }
        }
    }
}

// Decode NIP-19 identifier and determine redirect route
async fn decode_and_redirect(identifier: &str) -> Result<Route, String> {
    log::info!("Decoding NIP-19 identifier: {}", identifier);

    // Check prefix to determine type
    if identifier.starts_with("npub") {
        // Public key
        match PublicKey::from_bech32(identifier) {
            Ok(pubkey) => {
                log::info!("Decoded npub: {}", pubkey);
                Ok(Route::Profile {
                    pubkey: pubkey.to_hex()
                })
            }
            Err(e) => Err(format!("Invalid npub: {}", e))
        }
    } else if identifier.starts_with("note") {
        // Event ID
        match EventId::from_bech32(identifier) {
            Ok(event_id) => {
                log::info!("Decoded note: {}", event_id);
                Ok(Route::Note {
                    note_id: event_id.to_hex(),
                    from_voice: None,
                })
            }
            Err(e) => Err(format!("Invalid note ID: {}", e))
        }
    } else if identifier.starts_with("nprofile") {
        // Profile with relay hints - not yet supported but we can extract the pubkey
        Err("nprofile decoding not yet supported. Please use npub instead.".to_string())
    } else if identifier.starts_with("nevent") {
        // Event with relay hints - not yet supported but we can extract the event ID
        Err("nevent decoding not yet supported. Please use note instead.".to_string())
    } else if identifier.starts_with("nsec") {
        // Secret key - security warning
        Err("🔒 This is a private key (nsec)! Never share your private key with anyone or paste it into websites. Keep it safe!".to_string())
    } else if identifier.starts_with("naddr") {
        // Addressable event - not yet supported
        Err("Addressable events (naddr) are not yet supported. Coming soon!".to_string())
    } else if identifier.starts_with("nrelay") {
        // Relay URL
        Err("Relay URLs (nrelay) are not yet supported. Relay management coming soon.".to_string())
    } else {
        Err(format!(
            "Unrecognized identifier type. Supported types: npub, note, nprofile, nevent. Got: {}",
            identifier.chars().take(6).collect::<String>()
        ))
    }
}
