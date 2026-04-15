use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;

#[derive(Clone, Debug, Default)]
pub struct BlobbiStateStore {
    pub collection: Vec<BlobbiCompanion>,
    pub selected_d: Option<String>,
    pub loading: bool,
    pub error: Option<String>,
}

pub static BLOBBI_COLLECTION: GlobalSignal<BlobbiStateStore> =
    Signal::global(BlobbiStateStore::default);

pub fn get_selected_blobbi() -> Option<BlobbiCompanion> {
    let store = BLOBBI_COLLECTION.read();
    let selected_d = store.selected_d.as_ref()?;
    store
        .collection
        .iter()
        .find(|b| &b.d == selected_d)
        .cloned()
}

pub fn select_blobbi(d: String) {
    BLOBBI_COLLECTION.write().selected_d = Some(d);
}

pub fn set_collection(collection: Vec<BlobbiCompanion>) {
    let mut store = BLOBBI_COLLECTION.write();
    store.collection = collection;
    store.loading = false;
    store.error = None;
}

pub fn set_loading(loading: bool) {
    BLOBBI_COLLECTION.write().loading = loading;
}

pub fn set_error(error: Option<String>) {
    let mut store = BLOBBI_COLLECTION.write();
    store.error = error;
    store.loading = false;
}

pub fn update_blobbi_in_collection(blobbi: &BlobbiCompanion) {
    let mut store = BLOBBI_COLLECTION.write();
    if let Some(existing) = store.collection.iter_mut().find(|b| b.d == blobbi.d) {
        *existing = blobbi.clone();
    }
}
