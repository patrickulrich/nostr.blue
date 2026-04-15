use crate::components::blobbi::core::decay::{apply_decay, apply_sleep_recovery};
use crate::stores::blobbi_store;
use dioxus::prelude::*;

pub fn use_blobbi_decay() {
    use_future(move || {
        async move {
            loop {
                crate::platform::timer::sleep(std::time::Duration::from_secs(60)).await;

                let now = nostr_sdk::Timestamp::now().as_secs();
                let collection = {
                    let store = blobbi_store::BLOBBI_COLLECTION.read();
                    store.collection.clone()
                };

                for blobbi in &collection {
                    let updated = apply_decay(blobbi, now);
                    let recovered = apply_sleep_recovery(&updated, now);
                    blobbi_store::update_blobbi_in_collection(&recovered);
                }
            }
        }
    });
}
