//! Ephemeral clients: single-purpose, connected, thrown away.
//!
//! Used for throwaway scopes (NUT-18 payment transports, profile warmup)
//! where the main pool must not be touched. Every hand-rolled
//! `Client::builder().authenticator(...)` + add_relay + connect sequence
//! belongs here instead — 0.45 attaches signers at build time, and these
//! constructions are exactly what the migration broke.
use super::error::Error;
use nostr::key::Keys;
use nostr::types::url::RelayUrl;
use nostr_sdk::authenticator::SignerAuthenticator;
use nostr_sdk::client::Client;
use nostr_sdk::relay::RelayCapabilities;

/// Build a connected single-purpose client.
///
/// - signs with `keys` (generate throwaway keys with [`Keys::generate`] —
///   these are NOT the user identity)
/// - adds every parseable relay URL with the same capability set
///   (READ for listening scopes, WRITE for send-only scopes)
/// - connects before returning
///
/// Fails only when no relay URL parses. The client disconnects when its last
/// reference drops.
pub async fn ephemeral_client(
    keys: Keys,
    relay_urls: &[String],
    caps: RelayCapabilities,
) -> Result<Client, Error> {
    let client = Client::builder()
        .authenticator(SignerAuthenticator::new(keys))
        .build();
    let mut added = 0usize;
    for url in relay_urls {
        let Ok(relay_url) = RelayUrl::parse(url.as_str()) else {
            log::warn!("ephemeral_client: skipping invalid relay URL '{url}'");
            continue;
        };
        match client.add_relay(relay_url).capabilities(caps).await {
            Ok(_) => added += 1,
            Err(e) => log::warn!("ephemeral_client: failed to add relay '{url}': {e}"),
        }
    }
    if added == 0 {
        return Err(Error::Relay(
            "no valid relays for ephemeral client".to_string(),
        ));
    }
    client.connect().await;
    Ok(client)
}
