use std::time::Duration;

use dioxus::prelude::*;
use nostr_sdk::Filter;

use crate::components::blobbi::core::parsers::parse_blobbi_from_event;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::stores::blobbi_store;
use crate::stores::nostr_client;
use crate::utils::nip_bb::*;

pub fn use_blobbi_collection() {
    let mut pubkey_signal: Signal<Option<String>> = use_signal(|| None);
    let mut fetch_started: Signal<bool> = use_signal(|| false);

    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        let pubkey = crate::stores::auth_store::get_pubkey();
        if pubkey.is_none() {
            blobbi_store::set_loading(false);
            return;
        }
        let pk = pubkey.unwrap();
        if pubkey_signal() == Some(pk.clone()) {
            return;
        }
        pubkey_signal.set(Some(pk.clone()));

        if fetch_started() {
            return;
        }
        fetch_started.set(true);

        blobbi_store::set_loading(true);

        spawn(async move {
            let author = match nostr_sdk::PublicKey::from_hex(&pk) {
                Ok(a) => a,
                Err(e) => {
                    blobbi_store::set_error(Some(format!("Invalid pubkey: {}", e)));
                    return;
                }
            };

            let filter = Filter::new()
                .kind(blobbi_state_kind())
                .author(author)
                .custom_tag(
                    nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::B),
                    BLOBBI_ECOSYSTEM_TAG,
                )
                .limit(50);

            let events = match nostr_client::fetch_events_from_connected_relays(
                filter,
                Duration::from_secs(10),
            )
            .await
            {
                Ok(e) => e,
                Err(e) => {
                    blobbi_store::set_error(Some(format!("Failed to fetch blobbis: {}", e)));
                    return;
                }
            };

            let mut collection: Vec<BlobbiCompanion> =
                events.iter().map(parse_blobbi_from_event).collect();

            collection.sort_by_key(|b| std::cmp::Reverse(b.experience));

            let selected_d = {
                let store = blobbi_store::BLOBBI_COLLECTION.read();
                store.selected_d.clone()
            };

            if selected_d.is_none() && !collection.is_empty() {
                if let Some(profile_companion) =
                    crate::stores::blobbi_profile_store::get_profile()
                        .and_then(|p| p.current_companion)
                {
                    blobbi_store::select_blobbi(profile_companion);
                } else {
                    blobbi_store::select_blobbi(collection[0].d.clone());
                }
            }

            blobbi_store::set_collection(collection);
        });
    });
}
