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

    use_future(move || {
        let pubkey = crate::stores::auth_store::get_pubkey();
        async move {
            if pubkey.is_none() {
                blobbi_profile_store::set_profile_loading(false);
                return;
            }
            let pk = pubkey.unwrap();
            if pubkey_signal() == Some(pk.clone()) {
                return;
            }
            pubkey_signal.set(Some(pk.clone()));

            blobbi_profile_store::set_profile_loading(true);

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

            let filter_canonical = Filter::new()
                .kind(blobbonaut_profile_kind())
                .author(author)
                .identifier(&d_tag)
                .limit(1);

            let events = match nostr_client::fetch_events_from_connected_relays(
                filter_canonical,
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
                let ditto_kind = nostr_sdk::Kind::Custom(11125);
                let filter_ditto = Filter::new()
                    .kind(ditto_kind)
                    .author(author)
                    .identifier(&d_tag)
                    .limit(1);

                if let Ok(ditto_events) = nostr_client::fetch_events_from_connected_relays(
                    filter_ditto,
                    Duration::from_secs(5),
                )
                .await
                {
                    if let Some(ditto_event) = ditto_events.first() {
                        let profile = parse_profile_from_event(ditto_event);
                        blobbi_profile_store::set_profile(profile);
                        return;
                    }
                }

                let default = BlobbonautProfile {
                    d: profile_d_tag(&pk),
                    coins: INITIAL_BLOBBONAUT_COINS,
                    ..Default::default()
                };
                blobbi_profile_store::set_profile(default);
            }
        }
    });
}
