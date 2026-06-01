use crate::stores::profiles;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
pub fn use_author_metadata(pubkey: String) -> Signal<Option<Metadata>> {
    let mut metadata = use_signal(|| None::<Metadata>);
    use_effect(use_reactive!(|pubkey| {
        let pubkey_str = pubkey.clone();
        if let Some(cached) = profiles::get_profile(&pubkey_str) {
            metadata.set(Some(cached));
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
