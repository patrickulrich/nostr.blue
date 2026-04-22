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

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
pub async fn add_queued_event(_event: &QueuedEvent) -> Result<(), String> {
    Ok(())
}

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
pub async fn get_all_queued_events() -> Result<Vec<QueuedEvent>, String> {
    Ok(vec![])
}

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
pub async fn remove_queued_event(_event_id: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
pub async fn update_queued_event(_event: &QueuedEvent) -> Result<(), String> {
    Ok(())
}

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
#[allow(dead_code)]
pub async fn persist_all(_events: &[QueuedEvent]) -> Result<(), String> {
    Ok(())
}
