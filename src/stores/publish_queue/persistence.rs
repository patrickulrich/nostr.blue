use super::types::QueuedEvent;

#[allow(dead_code)]
const STORE_PUBLISH_QUEUE: &str = "publish_queue";

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use dioxus::prelude::ReadableExt;

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub async fn add_queued_event(event: &QueuedEvent) -> Result<(), String> {
    let localstore = crate::stores::cashu::signals::SHARED_LOCALSTORE.read();
    if let Some(ref db) = *localstore {
        db.add_queued_event(event).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub async fn get_all_queued_events() -> Result<Vec<QueuedEvent>, String> {
    let localstore = crate::stores::cashu::signals::SHARED_LOCALSTORE.read();
    if let Some(ref db) = *localstore {
        Ok(db.get_all_queued_events().await.map_err(|e| e.to_string())?)
    } else {
        Ok(vec![])
    }
}

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub async fn remove_queued_event(event_id: &str) -> Result<(), String> {
    let localstore = crate::stores::cashu::signals::SHARED_LOCALSTORE.read();
    if let Some(ref db) = *localstore {
        db.remove_queued_event(event_id).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub async fn update_queued_event(event: &QueuedEvent) -> Result<(), String> {
    let localstore = crate::stores::cashu::signals::SHARED_LOCALSTORE.read();
    if let Some(ref db) = *localstore {
        db.update_queued_event(event).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
#[allow(dead_code)]
pub async fn persist_all(events: &[QueuedEvent]) -> Result<(), String> {
    for event in events {
        add_queued_event(event).await?;
    }
    Ok(())
}

#[cfg(feature = "native")]
mod native {
    use super::QueuedEvent;
    use std::sync::Mutex;

    static FILE_LOCK: Mutex<()> = Mutex::new(());

    fn queue_path() -> std::path::PathBuf {
        crate::platform::storage::data_dir().join("nostr-blue").join("publish_queue.json")
    }

    fn load_queue() -> Vec<QueuedEvent> {
        let path = queue_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
                log::warn!("Failed to parse publish_queue.json, starting fresh: {}", e);
                vec![]
            }),
            Err(_) => vec![],
        }
    }

    fn save_queue(events: &[QueuedEvent]) -> Result<(), String> {
        let path = queue_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string(events).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn add_queued_event(event: &QueuedEvent) -> Result<(), String> {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut queue = load_queue();
        queue.retain(|e| e.id != event.id);
        queue.push(event.clone());
        save_queue(&queue)
    }

    pub async fn get_all_queued_events() -> Result<Vec<QueuedEvent>, String> {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        Ok(load_queue())
    }

    pub async fn remove_queued_event(event_id: &str) -> Result<(), String> {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut queue = load_queue();
        queue.retain(|e| e.id != event_id);
        save_queue(&queue)
    }

    pub async fn update_queued_event(event: &QueuedEvent) -> Result<(), String> {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut queue = load_queue();
        if let Some(existing) = queue.iter_mut().find(|e| e.id == event.id) {
            *existing = event.clone();
        }
        save_queue(&queue)
    }

    #[allow(dead_code)]
    pub async fn persist_all(events: &[QueuedEvent]) -> Result<(), String> {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        save_queue(events)
    }
}

#[cfg(feature = "native")]
pub use native::*;

#[cfg(not(any(
    all(target_arch = "wasm32", feature = "web", not(feature = "native")),
    feature = "native"
)))]
mod fallback {
    use super::QueuedEvent;

    pub async fn add_queued_event(_event: &QueuedEvent) -> Result<(), String> {
        Ok(())
    }

    pub async fn get_all_queued_events() -> Result<Vec<QueuedEvent>, String> {
        Ok(vec![])
    }

    pub async fn remove_queued_event(_event_id: &str) -> Result<(), String> {
        Ok(())
    }

    pub async fn update_queued_event(_event: &QueuedEvent) -> Result<(), String> {
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn persist_all(_events: &[QueuedEvent]) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(not(any(
    all(target_arch = "wasm32", feature = "web", not(feature = "native")),
    feature = "native"
)))]
pub use fallback::*;
