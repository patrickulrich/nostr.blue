use crate::stores::profiles;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

pub fn use_author_metadata(pubkey: String) -> Signal<Option<Metadata>> {
    let mut metadata = use_signal(|| None::<Metadata>);
    use_effect(use_reactive!(|pubkey| {
        let pubkey_str = pubkey.clone();
        if let Some(cached) = profiles::get_profile(&pubkey_str) {
            // A completely blank cache entry must never permanently suppress
            // fetching — treat it as a miss and refetch from relays
            // (bypassing the cache-hit path inside `fetch_profile`).
            if profiles::metadata_has_identity(&cached) {
                metadata.set(Some(cached));
                return;
            }
            let pk = pubkey_str.clone();
            spawn(async move {
                match profiles::fetch_profile_from_relays(&pk).await {
                    Ok(fresh) => {
                        metadata.set(Some(profiles::profile_to_metadata(&fresh)));
                    }
                    Err(e) => {
                        log::debug!("Failed to refetch blank cached profile {}: {}", pk, e);
                        // Fall back to what we have rather than nothing.
                        metadata.set(Some(cached));
                    }
                }
            });
            return;
        }
        spawn(async move {
            match profiles::fetch_profile(pubkey_str).await {
                Ok(profile) => {
                    metadata.set(Some(profiles::profile_to_metadata(&profile)));
                }
                Err(e) => {
                    log::debug!("Failed to fetch profile for author metadata: {}", e);
                }
            }
        });
    }));
    metadata
}
