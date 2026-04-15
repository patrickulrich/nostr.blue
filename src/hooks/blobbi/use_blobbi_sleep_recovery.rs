use crate::components::blobbi::core::decay::apply_sleep_recovery;
use crate::components::blobbi::core::builders::publish_blobbi_state_with_source;
use crate::stores::blobbi_store;
use dioxus::prelude::*;

pub fn use_blobbi_sleep_recovery() {
    use_future(move || {
        async move {
            loop {
                crate::platform::timer::sleep(std::time::Duration::from_secs(30 * 60)).await;

                let now = nostr_sdk::Timestamp::now().as_secs();
                let collection = {
                    let store = blobbi_store::BLOBBI_COLLECTION.read();
                    store.collection.clone()
                };

                for blobbi in &collection {
                    if !blobbi.is_sleeping() {
                        continue;
                    }

                    let recovered = apply_sleep_recovery(blobbi, now);
                    if recovered.stats.energy != blobbi.stats.energy || recovered.is_sleeping != blobbi.is_sleeping {
                        blobbi_store::update_blobbi_in_collection(&recovered);
                        let _ = publish_blobbi_state_with_source(&recovered, "system").await;
                    }
                }
            }
        }
    });
}
