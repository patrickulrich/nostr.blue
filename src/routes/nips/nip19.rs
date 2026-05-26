use crate::routes::address_viewer::AddressViewer;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

#[component]
pub fn Nip19Handler(identifier: String) -> Element {
    let identifier_for_effect = identifier.clone();
    let identifier_for_display = identifier.clone();

    use_effect(move || {
        let id = identifier_for_effect.clone();
        spawn(async move {
            if let Err(e) = decode_and_redirect(&id).await {
                log::warn!("Nip19Handler error: {}", e);
                let _ = e;
            }
        });
    });

    rsx! {
        AddressViewer { address: identifier_for_display.clone() }
    }
}

async fn decode_and_redirect(identifier: &str) -> std::result::Result<(), String> {
    if identifier.starts_with("nsec") {
        return Err(
            "🔒 This is a private key (nsec)! Never share your private key with anyone.".to_string(),
        );
    }
    if identifier.starts_with("nrelay") {
        return Err("Relay URLs (nrelay) are not yet supported.".to_string());
    }

    let nip19 = Nip19::from_bech32(identifier).map_err(|e| {
        format!(
            "Failed to decode '{}': {}",
            &identifier[..identifier.len().min(20)],
            e
        )
    })?;

    match nip19 {
        Nip19::Profile(profile) if !profile.relays.is_empty() => {
            let urls: Vec<String> = profile.relays.iter().map(|r| r.to_string()).collect();
            crate::stores::relay::coverage::record_user_relays(
                &profile.public_key.to_hex(),
                &urls,
            );
        }
        Nip19::Event(nevent) if !nevent.relays.is_empty() => {
            let urls: Vec<String> = nevent.relays.iter().map(|r| r.to_string()).collect();
            if let Some(author) = &nevent.author {
                crate::stores::relay::coverage::record_user_relays(
                    &author.to_hex(),
                    &urls,
                );
            }
        }
        Nip19::Secret(_) | Nip19::EncryptedSecret(_) => {
            return Err("🔒 Private key detected. Keep it safe!".to_string());
        }
        _ => {}
    }
    Ok(())
}
