use serde::{Deserialize, Serialize};

const SEARCH_HISTORY_KEY: &str = "nostr_blue_search_history";
const MAX_HISTORY: usize = 10;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RecentSearchItem {
    Query(String),
    Profile {
        pubkey: String,
        display_name: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SearchHistory {
    items: Vec<RecentSearchItem>,
}

static HISTORY_CACHE: std::sync::OnceLock<std::sync::Mutex<SearchHistory>> =
    std::sync::OnceLock::new();

fn get_cache() -> &'static std::sync::Mutex<SearchHistory> {
    HISTORY_CACHE.get_or_init(|| {
        let history = load_from_storage();
        std::sync::Mutex::new(history)
    })
}

fn load_from_storage() -> SearchHistory {
    crate::platform::storage::get::<SearchHistory>(SEARCH_HISTORY_KEY).unwrap_or_default()
}

fn save_to_storage(history: &SearchHistory) {
    if let Err(e) = crate::platform::storage::set(SEARCH_HISTORY_KEY, history) {
        log::warn!("Failed to save search history: {}", e);
    }
}

pub fn add_query(query: String) {
    if query.trim().is_empty() {
        return;
    }
    let cache = get_cache();
    let mut history = cache.lock().unwrap();
    history.items.retain(|item| match item {
        RecentSearchItem::Query(q) => q != &query,
        _ => true,
    });
    history
        .items
        .insert(0, RecentSearchItem::Query(query));
    history.items.truncate(MAX_HISTORY);
    save_to_storage(&history);
}

pub fn add_profile(pubkey: String, display_name: String) {
    let cache = get_cache();
    let mut history = cache.lock().unwrap();
    history.items.retain(|item| match item {
        RecentSearchItem::Profile { pubkey: pk, .. } => pk != &pubkey,
        _ => true,
    });
    history.items.insert(
        0,
        RecentSearchItem::Profile {
            pubkey,
            display_name,
        },
    );
    history.items.truncate(MAX_HISTORY);
    save_to_storage(&history);
}

#[allow(dead_code)]
pub fn remove_item(index: usize) {
    let cache = get_cache();
    let mut history = cache.lock().unwrap();
    if index < history.items.len() {
        history.items.remove(index);
        save_to_storage(&history);
    }
}

pub fn clear_all() {
    let cache = get_cache();
    let mut history = cache.lock().unwrap();
    history.items.clear();
    save_to_storage(&history);
}

pub fn get_items() -> Vec<RecentSearchItem> {
    let cache = get_cache();
    cache.lock().unwrap().items.clone()
}
