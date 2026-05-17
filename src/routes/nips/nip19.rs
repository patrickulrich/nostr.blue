use crate::routes::Route;
use crate::utils::route_for_kind::route_for_naddr;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
#[component]
pub fn Nip19Handler(identifier: String) -> Element {
    let mut error_msg = use_signal(|| None::<String>);
    let mut processing = use_signal(|| true);
    let identifier_for_effect = identifier.clone();
    let identifier_for_display = identifier.clone();
    use_effect(move || {
        let id = identifier_for_effect.clone();
        spawn(async move {
            match decode_and_redirect(&id).await {
                Ok(route) => {
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
        div { class: "min-h-screen flex items-center justify-center p-4",
            if *processing.read() {
                div { class: "text-center",
                    div { class: "text-4xl mb-4 animate-spin", "🔄" }
                    h2 { class: "text-xl font-semibold mb-2", "Processing identifier..." }
                    p { class: "text-muted-foreground text-sm font-mono break-all",
                        "{identifier_for_display}"
                    }
                }
            } else if let Some(err) = error_msg.read().as_ref() {
                div { class: "text-center max-w-md",
                    div { class: "text-6xl mb-4", "❌" }
                    h2 { class: "text-2xl font-bold mb-4", "Invalid Identifier" }
                    p { class: "text-muted-foreground mb-4", "{err}" }
                    div { class: "p-3 bg-muted rounded-lg mb-6",
                        p { class: "text-xs font-mono break-all", "{identifier_for_display}" }
                    }
                    Link {
                        to: Route::Home { list: String::new() },
                        class: "inline-block px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                        "← Go Home"
                    }
                }
            }
        }
    }
}
async fn decode_and_redirect(identifier: &str) -> std::result::Result<Route, String> {
    log::info!("Decoding NIP-19 identifier: {}", identifier);
    if identifier.starts_with("nsec") {
        return Err(
            "🔒 This is a private key (nsec)! Never share your private key with anyone or paste it into websites. Keep it safe!"
                .to_string(),
        );
    }
    if identifier.starts_with("nrelay") {
        return Err(
            "Relay URLs (nrelay) are not yet supported. Relay management coming soon.".to_string(),
        );
    }
    match Nip19::from_bech32(identifier) {
        Ok(nip19) => {
            match nip19 {
                Nip19::Pubkey(pubkey) => {
                    log::info!("Decoded npub: {}", pubkey);
                    Ok(Route::Profile {
                        pubkey: pubkey.to_bech32().unwrap_or_else(|_| pubkey.to_hex()),
                    })
                }
                Nip19::Profile(profile) => {
                    log::info!(
                        "Decoded nprofile: {} with {} relay hints", profile.public_key,
                        profile.relays.len()
                    );
                    if !profile.relays.is_empty() {
                        let urls: Vec<String> = profile.relays.iter().map(|r| r.to_string()).collect();
                        crate::stores::relay::coverage::record_user_relays(
                            &profile.public_key.to_hex(), &urls,
                        );
                    }
                    Ok(Route::Profile {
                        pubkey: profile.to_bech32().unwrap_or_else(|_| profile.public_key.to_hex()),
                    })
                }
                Nip19::EventId(event_id) => {
                    log::info!("Decoded note: {}", event_id);
                    Ok(Route::Note {
                        note_id: event_id.to_bech32().unwrap_or_else(|_| event_id.to_hex()),
                        from_voice: None,
                    })
                }
                Nip19::Event(nevent) => {
                    log::info!(
                        "Decoded nevent: {} with {} relay hints", nevent.event_id, nevent
                        .relays.len()
                    );
                    if !nevent.relays.is_empty() {
                        let urls: Vec<String> = nevent.relays.iter().map(|r| r.to_string()).collect();
                        if let Some(author) = &nevent.author {
                            crate::stores::relay::coverage::record_user_relays(
                                &author.to_hex(), &urls,
                            );
                        }
                    }
                    Ok(Route::Note {
                        note_id: nevent.to_bech32().unwrap_or_else(|_| nevent.event_id.to_hex()),
                        from_voice: None,
                    })
                }
                Nip19::Coordinate(coord) => {
                    log::info!(
                        "Decoded naddr: kind={} pubkey={} id={}", coord.coordinate.kind
                        .as_u16(), coord.coordinate.public_key, coord.coordinate
                        .identifier
                    );
                    let kind = coord.coordinate.kind.as_u16();
                    let naddr = identifier.to_string();
                    let pubkey = coord.coordinate.public_key;
                    let identifier_str = coord.coordinate.identifier.clone();
                    route_for_naddr(kind, naddr, pubkey, identifier_str)
                        .ok_or_else(|| {
                            format!(
                                "Addressable event kind {} is not yet supported. naddr: {}",
                                kind,
                                identifier,
                            )
                        })
                }
                Nip19::Secret(_) => {
                    Err(
                        "🔒 This is a private key (nsec)! Never share your private key with anyone."
                            .to_string(),
                    )
                }
                Nip19::EncryptedSecret(_) => {
                    Err(
                        "🔐 This is an encrypted private key (ncryptsec). While encrypted, avoid pasting it into untrusted websites. Import it safely via Settings."
                            .to_string(),
                    )
                }
            }
        }
        Err(e) => Err(format!(
            "Failed to decode NIP-19 identifier '{}...': {}",
            identifier.chars().take(20).collect::<String>(),
            e,
        )),
    }
}
