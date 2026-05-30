use crate::platform::storage;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingOfflineToken {
    pub id: String,
    pub token_string: String,
    pub mint_url: String,
    pub value: u64,
    pub stored_at: u64,
}

pub static PENDING_OFFLINE_TOKENS: GlobalSignal<Vec<PendingOfflineToken>> =
    Signal::global(Vec::new);

const STORAGE_KEY: &str = "pending_offline_tokens";

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn persist_tokens(tokens: &[PendingOfflineToken]) -> Result<(), String> {
    storage::set(STORAGE_KEY, tokens)
}

pub fn load_pending_offline_tokens() {
    if let Ok(tokens) = storage::get::<Vec<PendingOfflineToken>>(STORAGE_KEY) {
        *PENDING_OFFLINE_TOKENS.write() = tokens;
    }
}

pub async fn store_offline_token(
    token_string: String,
    mint_url: String,
    value: u64,
) -> Result<(), String> {
    let id = uuid::Uuid::new_v4().to_string();
    let pending = PendingOfflineToken {
        id,
        token_string,
        mint_url,
        value,
        stored_at: now_millis(),
    };
    let mut tokens = PENDING_OFFLINE_TOKENS.write();
    tokens.push(pending);
    persist_tokens(&tokens)?;
    log::info!(
        "Stored offline token, {} pending total",
        tokens.len()
    );
    Ok(())
}

async fn remove_offline_token(id: &str) {
    let mut tokens = PENDING_OFFLINE_TOKENS.write();
    tokens.retain(|t| t.id != id);
    if let Err(e) = persist_tokens(&tokens) {
        log::warn!("Failed to persist offline token removal: {}", e);
    }
}

pub async fn redeem_offline_tokens() {
    let tokens_to_redeem = PENDING_OFFLINE_TOKENS.read().clone();
    if tokens_to_redeem.is_empty() {
        return;
    }
    log::info!(
        "Starting offline token redemption for {} tokens",
        tokens_to_redeem.len()
    );
    for token in tokens_to_redeem {
        match super::receive::receive_tokens(token.token_string.clone()).await {
            Ok(received) => {
                log::info!(
                    "Redeemed offline token: {} sats from {}",
                    received,
                    token.mint_url
                );
                remove_offline_token(&token.id).await;
            }
            Err(e) => {
                log::warn!(
                    "Failed to redeem offline token ({} sats from {}): {}",
                    token.value,
                    token.mint_url,
                    e
                );
            }
        }
    }
}

pub fn start_offline_redemption_watcher() {
    spawn(async move {
        let mut was_online = *crate::stores::ui::online_status::ONLINE_STATUS.read();
        loop {
            crate::platform::timer::sleep_ms(5000).await;
            let is_online = *crate::stores::ui::online_status::ONLINE_STATUS.read();
            if !was_online && is_online {
                log::info!("Online status changed: offline -> online, redeeming offline tokens");
                redeem_offline_tokens().await;
            }
            was_online = is_online;
        }
    });
}
