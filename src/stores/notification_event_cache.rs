use dioxus::prelude::*;
use nostr_sdk::Event;
use std::collections::HashMap;

static NOTIFICATION_EVENT_CACHE: GlobalSignal<HashMap<String, Event>> =
    Signal::global(HashMap::new);

pub fn get_cached_referenced_event(event_id: &str) -> Option<Event> {
    NOTIFICATION_EVENT_CACHE.read().get(event_id).cloned()
}

pub fn cache_referenced_events(events: Vec<Event>) {
    if events.is_empty() {
        return;
    }
    NOTIFICATION_EVENT_CACHE
        .write()
        .extend(events.into_iter().map(|e| (e.id.to_hex(), e)));
}

pub fn clear_notification_event_cache() {
    NOTIFICATION_EVENT_CACHE.write().clear();
}
