use nostr_sdk::nips::nip19::{Nip19, Nip19Event, Nip19Profile};
use nostr_sdk::{EventId, FromBech32, PublicKey, RelayUrl, ToBech32};

pub fn parse_profile_id(input: &str) -> Option<PublicKey> {
    let trimmed = input.trim();
    let normalized = trimmed
        .strip_prefix("nostr:")
        .or_else(|| trimmed.strip_prefix("NOSTR:"))
        .unwrap_or(trimmed);

    if let Ok(nip19) = Nip19::from_bech32(normalized) {
        match nip19 {
            Nip19::Pubkey(pk) => return Some(pk),
            Nip19::Profile(p) => return Some(p.public_key),
            _ => {}
        }
    }

    PublicKey::from_hex(normalized).ok()
}

pub fn profile_route_id(pubkey: &str) -> String {
    let pk = match parse_profile_id(pubkey) {
        Some(pk) => pk,
        None => return pubkey.to_string(),
    };

    if let Some(relays) =
        crate::stores::relay::coverage::get_known_user_relays(&pk.to_hex())
    {
        let relay_urls: Vec<RelayUrl> = relays
            .iter()
            .take(2)
            .filter_map(|s| RelayUrl::parse(s).ok())
            .collect();
        if !relay_urls.is_empty() {
            let nprofile = Nip19Profile::new(pk, relay_urls);
            if let Ok(bech32) = nprofile.to_bech32() {
                return bech32;
            }
        }
    }

    pk.to_bech32().unwrap_or_else(|_| pk.to_hex())
}

pub fn note_route_id(event_id: &str, author_pubkey: Option<&str>) -> String {
    let id = match crate::stores::nostr_client::parse_event_id(event_id) {
        Some(parsed) => parsed.event_id,
        None => match EventId::from_hex(event_id) {
            Ok(id) => id,
            Err(_) => return event_id.to_string(),
        }
    };

    if let Some(author_hex) = author_pubkey {
        if let Some(relays) =
            crate::stores::relay::coverage::get_known_user_relays(author_hex)
        {
            if let Ok(author) = PublicKey::from_hex(author_hex) {
                let relay_urls: Vec<RelayUrl> = relays
                    .iter()
                    .take(2)
                    .filter_map(|s| RelayUrl::parse(s).ok())
                    .collect();
                let nevent = Nip19Event::new(id).author(author).relays(relay_urls);
                if let Ok(bech32) = nevent.to_bech32() {
                    return bech32;
                }
            }
        }
    }

    id.to_bech32().unwrap_or_else(|_| id.to_hex())
}
