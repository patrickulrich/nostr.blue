use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey};
use crate::stores::{auth_store, nostr_client};
use std::time::Duration;
use std::collections::HashMap;

/// Represents a DM conversation with another user
#[derive(Clone, Debug, PartialEq)]
pub struct Conversation {
    pub pubkey: String,
    pub messages: Vec<Event>,
    pub unread_count: usize,
}

/// Global signal to track DM conversations
pub static CONVERSATIONS: GlobalSignal<HashMap<String, Conversation>> =
    Signal::global(|| HashMap::new());

/// Initialize DMs by fetching conversations from relays
pub async fn init_dms() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Loading DMs for {}", pubkey_str);

    let mut all_messages = Vec::new();

    // Fetch NIP-04 DMs (Kind 4) - Legacy encrypted direct messages
    let received_nip04 = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(pubkey)
        .limit(200);

    let sent_nip04 = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .author(pubkey)
        .limit(200);

    // Fetch NIP-17 DMs (Kind 1059) - Gift wrapped private messages
    let received_nip17 = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(pubkey)
        .limit(200);

    // Note: NIP-17 sent messages are wrapped with ephemeral keys,
    // so we can't easily query our sent messages by author.
    // We'll rely on receiving the rumor from relays.

    // Fetch received NIP-04 messages
    match client.fetch_events(received_nip04, Duration::from_secs(10)).await {
        Ok(events) => {
            let count = events.len();
            all_messages.extend(events.into_iter());
            log::info!("Fetched {} received NIP-04 DMs", count);
        }
        Err(e) => {
            log::error!("Failed to fetch received NIP-04 DMs: {}", e);
        }
    }

    // Fetch sent NIP-04 messages
    match client.fetch_events(sent_nip04, Duration::from_secs(10)).await {
        Ok(events) => {
            let count = events.len();
            all_messages.extend(events.into_iter());
            log::info!("Fetched {} sent NIP-04 DMs", count);
        }
        Err(e) => {
            log::error!("Failed to fetch sent NIP-04 DMs: {}", e);
        }
    }

    // Fetch received NIP-17 messages (gift wraps)
    match client.fetch_events(received_nip17, Duration::from_secs(10)).await {
        Ok(events) => {
            let count = events.len();
            all_messages.extend(events.into_iter());
            log::info!("Fetched {} received NIP-17 DMs (gift wraps)", count);
        }
        Err(e) => {
            log::error!("Failed to fetch received NIP-17 DMs: {}", e);
        }
    }

    // Group messages by conversation partner
    let mut conversations: HashMap<String, Conversation> = HashMap::new();

    for msg in all_messages {
        // Handle NIP-17 (GiftWrap) vs NIP-04 (EncryptedDirectMessage)
        if msg.kind == Kind::GiftWrap {
            // NIP-17: Unwrap the gift wrap to get the actual sender
            match client.unwrap_gift_wrap(&msg).await {
                Ok(unwrapped) => {
                    // The rumor contains the actual message (Kind 14)
                    if unwrapped.rumor.kind == Kind::PrivateDirectMessage {
                        let sender_pubkey = unwrapped.sender.to_string();
                        let other_pubkey = sender_pubkey;

                        // Create a pseudo-event for the conversation (use the rumor content)
                        // We'll store the original gift wrap event but note it's NIP-17
                        conversations.entry(other_pubkey.clone())
                            .or_insert_with(|| Conversation {
                                pubkey: other_pubkey.clone(),
                                messages: Vec::new(),
                                unread_count: 0,
                            })
                            .messages.push(msg);
                    }
                }
                Err(e) => {
                    log::error!("Failed to unwrap gift wrap: {}", e);
                    continue;
                }
            }
        } else {
            // NIP-04: Standard encrypted direct message
            let other_pubkey = if msg.pubkey.to_string() == pubkey_str {
                // We sent this message, get recipient from p-tag
                msg.tags.iter()
                    .find(|tag| tag.kind() == nostr_sdk::TagKind::p())
                    .and_then(|tag| tag.content())
                    .unwrap_or_default()
                    .to_string()
            } else {
                // We received this message, sender is the other party
                msg.pubkey.to_string()
            };

            if other_pubkey.is_empty() {
                continue;
            }

            // Add message to conversation
            conversations.entry(other_pubkey.clone())
                .or_insert_with(|| Conversation {
                    pubkey: other_pubkey.clone(),
                    messages: Vec::new(),
                    unread_count: 0,
                })
                .messages.push(msg);
        }
    }

    // Sort messages in each conversation by timestamp
    for conversation in conversations.values_mut() {
        conversation.messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    }

    log::info!("Organized into {} conversations", conversations.len());
    *CONVERSATIONS.write() = conversations;

    Ok(())
}

/// Send an encrypted DM to a recipient
pub async fn send_dm(recipient_pubkey: String, content: String) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let recipient_pk = PublicKey::parse(&recipient_pubkey)
        .map_err(|e| format!("Invalid recipient pubkey: {}", e))?;

    log::info!("Sending DM to {}", recipient_pubkey);

    match client.send_private_msg(recipient_pk, content, None).await {
        Ok(event_id) => {
            log::info!("DM sent successfully: {:?}", event_id);

            // Refresh conversations to include new message
            let _ = init_dms().await;

            Ok(())
        }
        Err(e) => {
            log::error!("Failed to send DM: {}", e);
            Err(format!("Failed to send DM: {}", e))
        }
    }
}

/// Decrypt a DM event (supports both NIP-04 and NIP-17)
pub async fn decrypt_dm(event: &Event) -> Result<String, String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // NIP-17: Try to unwrap gift wrap first (Kind 1059)
    if event.kind == Kind::GiftWrap {
        match client.unwrap_gift_wrap(&event).await {
            Ok(unwrapped) => {
                log::debug!("Successfully unwrapped NIP-17 gift wrap from {}", unwrapped.sender);
                return Ok(unwrapped.rumor.content);
            }
            Err(e) => {
                log::error!("Failed to unwrap gift wrap: {}", e);
                return Err(format!("Failed to decrypt NIP-17 message: {}", e));
            }
        }
    }

    // NIP-04: For Kind 4 encrypted direct messages
    if event.kind == Kind::EncryptedDirectMessage {
        // The nostr-sdk client should handle NIP-04 decryption automatically
        // when fetching events. If the content is still encrypted, we need to
        // manually decrypt it.

        // Check if content looks encrypted (base64-like)
        if event.content.contains("?iv=") {
            // This is an encrypted NIP-04 message, need to decrypt
            let my_pubkey = auth_store::get_pubkey()
                .ok_or("Not authenticated")?;

            // Determine the other party's pubkey
            let other_pubkey = if event.pubkey.to_string() == my_pubkey {
                // We sent it, decrypt with recipient's pubkey
                event.tags.iter()
                    .find(|tag| tag.kind() == nostr_sdk::TagKind::p())
                    .and_then(|tag| tag.content())
                    .ok_or("No recipient found in sent message")?
                    .to_string()
            } else {
                // We received it, decrypt with sender's pubkey
                event.pubkey.to_string()
            };

            let other_pk = PublicKey::parse(&other_pubkey)
                .map_err(|e| format!("Invalid pubkey: {}", e))?;

            // Use signer to decrypt (NIP-04)
            let signer_result = nostr_client::NOSTR_CLIENT.read().as_ref()
                .ok_or("Client not initialized")?
                .signer().await;

            if let Ok(signer) = signer_result {
                match signer.nip04_decrypt(&other_pk, &event.content).await {
                    Ok(decrypted) => {
                        log::debug!("Successfully decrypted NIP-04 message");
                        return Ok(decrypted);
                    }
                    Err(e) => {
                        log::error!("Failed to decrypt NIP-04 message: {}", e);
                        return Err(format!("Failed to decrypt NIP-04 message: {}", e));
                    }
                }
            } else {
                return Err("No signer available for decryption".to_string());
            }
        } else {
            // Content is already decrypted or plain text
            return Ok(event.content.clone());
        }
    }

    // Unknown kind
    Err(format!("Unsupported message kind: {:?}", event.kind))
}

/// Get a specific conversation
pub fn get_conversation(pubkey: &str) -> Option<Conversation> {
    CONVERSATIONS.read().get(pubkey).cloned()
}

/// Get all conversations sorted by last message time
pub fn get_conversations_sorted() -> Vec<Conversation> {
    let mut convos: Vec<Conversation> = CONVERSATIONS.read().values().cloned().collect();

    // Sort by most recent message
    convos.sort_by(|a, b| {
        let last_a = a.messages.last().map(|m| m.created_at).unwrap_or_default();
        let last_b = b.messages.last().map(|m| m.created_at).unwrap_or_default();
        last_b.cmp(&last_a)
    });

    convos
}
