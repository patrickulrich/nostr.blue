use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbonautProfile;

#[derive(Clone, Debug, Default)]
pub struct BlobbiProfileStore {
    pub profile: Option<BlobbonautProfile>,
    pub loading: bool,
    pub error: Option<String>,
}

pub static BLOBBI_PROFILE: GlobalSignal<BlobbiProfileStore> =
    Signal::global(BlobbiProfileStore::default);

pub fn get_profile() -> Option<BlobbonautProfile> {
    BLOBBI_PROFILE.read().profile.clone()
}

pub fn set_profile(profile: BlobbonautProfile) {
    cache_profile_to_local_storage(&profile);
    let mut store = BLOBBI_PROFILE.write();
    store.profile = Some(profile);
    store.loading = false;
    store.error = None;
}

pub fn set_profile_loading(loading: bool) {
    BLOBBI_PROFILE.write().loading = loading;
}

pub fn set_profile_error(error: Option<String>) {
    let mut store = BLOBBI_PROFILE.write();
    store.error = error;
    store.loading = false;
}

#[allow(dead_code)]
pub fn get_coins() -> u64 {
    BLOBBI_PROFILE
        .read()
        .profile
        .as_ref()
        .map(|p| p.coins)
        .unwrap_or(0)
}

#[allow(dead_code)]
pub fn get_item_quantity(item_id: &str) -> u32 {
    BLOBBI_PROFILE
        .read()
        .profile
        .as_ref()
        .and_then(|p| p.storage.iter().find(|i| i.item_id == item_id))
        .map(|i| i.quantity)
        .unwrap_or(0)
}

#[allow(dead_code)]
pub fn is_onboarding_done() -> bool {
    BLOBBI_PROFILE
        .read()
        .profile
        .as_ref()
        .map(|p| p.onboarding_done)
        .unwrap_or(false)
}

#[allow(dead_code)]
const CACHE_KEY_PREFIX: &str = "blobbi:profile:cache:";

#[cfg(feature = "web")]
pub fn cache_profile_to_local_storage(profile: &BlobbonautProfile) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let key = format!("{}{}", CACHE_KEY_PREFIX, profile.d);
            let _ = storage.set_item(&key, &profile.content_json);
        }
    }
}

#[cfg(not(feature = "web"))]
pub fn cache_profile_to_local_storage(_profile: &BlobbonautProfile) {}

#[cfg(feature = "web")]
#[allow(dead_code)]
pub fn load_profile_cache(d: &str) -> Option<String> {
    let window = web_sys::window()?;
    let storage = window.local_storage().ok()??;
    let key = format!("{}{}", CACHE_KEY_PREFIX, d);
    storage.get_item(&key).ok().flatten()
}

#[cfg(not(feature = "web"))]
#[allow(dead_code)]
pub fn load_profile_cache(_d: &str) -> Option<String> {
    None
}
