use std::time::Duration;

use dioxus::prelude::*;
use nostr_sdk::Filter;

use crate::components::blobbi::core::parsers::parse_profile_from_event;
use crate::components::blobbi::core::types::BlobbonautProfile;
use crate::stores::blobbi_profile_store;
use crate::stores::nostr_client;
use crate::utils::nip_bb::*;

pub fn use_blobbi_profile() {
    let mut pubkey_signal: Signal<Option<String>> = use_signal(|| None);
    let mut fetch_started: Signal<bool> = use_signal(|| false);

    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        let pubkey = crate::stores::auth_store::get_pubkey();
        if pubkey.is_none() {
            blobbi_profile_store::set_profile_loading(false);
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

        blobbi_profile_store::set_profile_loading(true);

        spawn(async move {
            let author = match nostr_sdk::PublicKey::from_hex(&pk) {
                Ok(a) => a,
                Err(e) => {
                    blobbi_profile_store::set_profile_error(Some(format!(
                        "Invalid pubkey: {}",
                        e
                    )));
                    return;
                }
            };

            let d_tag = profile_d_tag(&pk);

            let filter = Filter::new()
                .kinds([
                    nostr_sdk::Kind::Custom(KIND_BLOBBONAUT_PROFILE),
                    nostr_sdk::Kind::Custom(31125),
                ])
                .author(author)
                .identifier(&d_tag)
                .limit(1);

            let events = match nostr_client::fetch_events_from_connected_relays(
                filter,
                Duration::from_secs(10),
            )
            .await
            {
                Ok(e) => e,
                Err(e) => {
                    blobbi_profile_store::set_profile_error(Some(format!(
                        "Failed to fetch profile: {}",
                        e
                    )));
                    return;
                }
            };

            if let Some(event) = events.first() {
                let profile = parse_profile_from_event(event);
                blobbi_profile_store::set_profile(profile);
            } else {
                let default = BlobbonautProfile {
                    d: profile_d_tag(&pk),
                    coins: INITIAL_BLOBBONAUT_COINS,
                    ..Default::default()
                };
                blobbi_profile_store::set_profile(default);
            }
        });
    });
}
