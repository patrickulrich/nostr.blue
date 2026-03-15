#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use indexed_db_futures::prelude::*;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use std::future::IntoFuture;

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub const DB_NAME: &str = "nostr_blue_ai_providers";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub const DB_VERSION: u32 = 2;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub const STORE_SETTINGS: &str = "settings";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub const STORE_CHAT_HISTORY: &str = "chat_history";

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub async fn open_ai_db_with_schema(context: &str) -> Result<IdbDatabase, String> {
    let mut db_req: OpenDbRequest = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
        .map_err(|e| format!("Failed to open {} database: {:?}", context, e))?;
    db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
        let db = evt.db();
        if !db.object_store_names().any(|name| name == STORE_SETTINGS) {
            db.create_object_store(STORE_SETTINGS)?;
        }
        if !db
            .object_store_names()
            .any(|name| name == STORE_CHAT_HISTORY)
        {
            db.create_object_store(STORE_CHAT_HISTORY)?;
        }
        Ok(())
    }));

    db_req
        .into_future()
        .await
        .map_err(|e| format!("Failed to open {} database: {:?}", context, e))
}
