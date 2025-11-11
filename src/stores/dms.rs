use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::{Event, Filter, Kind, PublicKey, Timestamp, UnsignedEvent};
use crate::stores::{auth_store, nostr_client};
use std::time::Duration;
use std::collections::HashMap;

/// Represents a message in a conversation, handling both NIP-04 and NIP-17
#[derive(Clone, Debug, PartialEq)]
pub enum ConversationMessage {
    /// NIP-04 legacy encrypted direct message
    Nip04 {
        event: Event,
    },
    /// NIP-17 gift wrap message with unwrapped data
    Nip17 {
        gift_wrap: Event,
        rumor: UnsignedEvent,
        sender: PublicKey,
    },
}

impl ConversationMessage {
    /// Get the actual message timestamp (uses rumor timestamp for NIP-17)
    pub fn created_at(&self) -> Timestamp {
        match self {
            Self::Nip04 { event } => event.created_at,
            Self::Nip17 { rumor, .. } => rumor.created_at,
        }
    }

    /// Get the sender's public key
    pub fn sender(&self) -> PublicKey {
        match self {
            Self::Nip04 { event } => event.pubkey,
            Self::Nip17 { sender, .. } => *sender,
        }
    }
}

/// Represents a DM conversation with another user
#[derive(Clone, Debug, PartialEq)]
pub struct Conversation {
    pub pubkey: String,
    pub messages: Vec<ConversationMessage>,
    pub unread_count: usize,
}

/// Store for DM conversations with fine-grained reactivity
#[derive(Clone, Debug, PartialEq, Default, Store)]
pub struct ConversationsStore {
    pub data: HashMap<String, Conversation>,
}

/// Global store to track DM conversations
pub static CONVERSATIONS: GlobalSignal<Store<ConversationsStore>> =
    Signal::global(|| Store::new(ConversationsStore::default()));

/// Initialize DMs by fetching conversations from relays
pub async fn init_dms() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Loading DMs for {}", pubkey_str);

    // Create filters for all DM types
    let received_nip04 = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(pubkey)
        .limit(200);

    let sent_nip04 = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .author(pubkey)
        .limit(200);

    // NIP-17: Query gift wraps with our pubkey in p-tag
    // This gets BOTH received messages AND sent message copies (per NIP-17 spec)
    let nip17_all = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(pubkey)
        .limit(300);

    // PARALLEL FETCHES - All three at once!
    let (received_nip04_result, sent_nip04_result, nip17_all_result) = tokio::join!(
        nostr_client::fetch_events_aggregated(received_nip04, Duration::from_secs(10)),
        nostr_client::fetch_events_aggregated(sent_nip04, Duration::from_secs(10)),
        nostr_client::fetch_events_aggregated(nip17_all, Duration::from_secs(10))
    );

    // Combine all messages
    let mut all_messages = Vec::new();

    if let Ok(events) = received_nip04_result {
        log::info!("Fetched {} received NIP-04 DMs", events.len());
        all_messages.extend(events);
    } else if let Err(e) = received_nip04_result {
        log::error!("Failed to fetch received NIP-04 DMs: {}", e);
    }

    if let Ok(events) = sent_nip04_result {
        log::info!("Fetched {} sent NIP-04 DMs", events.len());
        all_messages.extend(events);
    } else if let Err(e) = sent_nip04_result {
        log::error!("Failed to fetch sent NIP-04 DMs: {}", e);
    }

    if let Ok(events) = nip17_all_result {
        log::info!("Fetched {} NIP-17 DMs (gift wraps - both received and sent)", events.len());
        all_messages.extend(events);
    } else if let Err(e) = nip17_all_result {
        log::error!("Failed to fetch NIP-17 DMs: {}", e);
    }

    log::info!("Loaded {} total DM events", all_messages.len());

    // Group messages by conversation partner
    let mut conversations: HashMap<String, Conversation> = HashMap::new();

    for msg in all_messages {
        // Handle NIP-17 (GiftWrap) vs NIP-04 (EncryptedDirectMessage)
        if msg.kind == Kind::GiftWrap {
            // NIP-17: Unwrap the gift wrap to get the actual sender and receiver
            match client.unwrap_gift_wrap(&msg).await {
                Ok(unwrapped) => {
                    // The rumor contains the actual message (Kind 14)
                    if unwrapped.rumor.kind == Kind::PrivateDirectMessage {
                        let sender_pubkey = unwrapped.sender.to_string();

                        // Determine the other party (conversation partner)
                        let other_pubkey = if sender_pubkey == pubkey_str {
                            // WE sent this message - get receiver from rumor's p-tag
                            unwrapped.rumor.tags.iter()
                                .find(|tag| tag.kind() == nostr_sdk::TagKind::p())
                                .and_then(|tag| tag.content())
                                .unwrap_or_default()
                                .to_string()
                        } else {
                            // We RECEIVED this message - sender is the other party
                            sender_pubkey
                        };

                        if other_pubkey.is_empty() {
                            log::warn!("Failed to determine conversation partner for NIP-17 message");
                            continue;
                        }

                        // Store as ConversationMessage::Nip17 with unwrapped data
                        let conversation_msg = ConversationMessage::Nip17 {
                            gift_wrap: msg,
                            rumor: unwrapped.rumor,
                            sender: unwrapped.sender,
                        };

                        conversations.entry(other_pubkey.clone())
                            .or_insert_with(|| Conversation {
                                pubkey: other_pubkey.clone(),
                                messages: Vec::new(),
                                unread_count: 0,
                            })
                            .messages.push(conversation_msg);
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

            // Store as ConversationMessage::Nip04
            let conversation_msg = ConversationMessage::Nip04 { event: msg };

            conversations.entry(other_pubkey.clone())
                .or_insert_with(|| Conversation {
                    pubkey: other_pubkey.clone(),
                    messages: Vec::new(),
                    unread_count: 0,
                })
                .messages.push(conversation_msg);
        }
    }

    // Sort messages in each conversation by timestamp (uses actual rumor timestamp for NIP-17)
    for conversation in conversations.values_mut() {
        conversation.messages.sort_by(|a, b| a.created_at().cmp(&b.created_at()));
    }

    log::info!("Organized into {} conversations", conversations.len());
    *CONVERSATIONS.read().data().write() = conversations;

    Ok(())
}

/// Send an encrypted DM to a recipient (NIP-17 compliant with sender copy)
pub async fn send_dm(recipient_pubkey: String, content: String) -> Result<(), String> {
    use nostr_sdk::EventBuilder;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let recipient_pk = PublicKey::parse(&recipient_pubkey)
        .map_err(|e| format!("Invalid recipient pubkey: {}", e))?;

    let signer = client.signer().await
        .map_err(|e| format!("Failed to get signer: {}", e))?;

    let sender_pk = signer.get_public_key().await
        .map_err(|e| format!("Failed to get sender pubkey: {}", e))?;

    log::info!("Sending DM from {} to {}", sender_pk.to_hex(), recipient_pubkey);

    // Build the rumor (kind 14 unsigned message)
    let rumor = EventBuilder::private_msg_rumor(recipient_pk, content.clone())
        .build(sender_pk);

    // Create gift wrap for RECEIVER (with receiver's p-tag)
    let receiver_gift_wrap = EventBuilder::gift_wrap(&signer, &recipient_pk, rumor.clone(), [])
        .await
        .map_err(|e| format!("Failed to create receiver gift wrap: {}", e))?;

    // Create gift wrap for SENDER (with sender's p-tag) - NIP-17 requirement!
    let sender_gift_wrap = EventBuilder::gift_wrap(&signer, &sender_pk, rumor, [])
        .await
        .map_err(|e| format!("Failed to create sender gift wrap: {}", e))?;

    // With gossip, the client automatically routes to appropriate DM inbox relays (NIP-17)
    log::debug!("Sending DM gift wraps using gossip routing");

    // Send gift wrap to receiver's inbox relays (NIP-17 compliant)
    let receiver_result = client.send_event(&receiver_gift_wrap).await
        .map_err(|e| format!("Failed to send to receiver: {}", e))?;

    log::info!("Sent gift wrap to receiver: {:?}", receiver_result.val);

    // Send gift wrap to sender's inbox relays for their own copy
    let sender_result = client.send_event(&sender_gift_wrap).await
    .map_err(|e| format!("Failed to send sender copy: {}", e))?;

    log::info!("Sent gift wrap to sender (copy): {:?}", sender_result.val);

    // Refresh conversations to include new message
    if let Err(e) = init_dms().await {
        log::error!("Failed to refresh DM conversations after sending message: {}", e);
        // Continue despite refresh failure - message was sent successfully
    }

    Ok(())
}

/// Decrypt a DM message (supports both NIP-04 and NIP-17)
pub async fn decrypt_dm(msg: &ConversationMessage) -> Result<String, String> {
    // NIP-17: Content is already available from the unwrapped rumor
    if let ConversationMessage::Nip17 { rumor, .. } = msg {
        log::debug!("Returning NIP-17 message content from rumor");
        return Ok(rumor.content.clone());
    }

    // NIP-04: Need to decrypt
    let ConversationMessage::Nip04 { event } = msg else {
        return Err("Invalid message type".to_string());
    };

    let _client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

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
    CONVERSATIONS.read().data().read().get(pubkey).cloned()
}

/// Get all conversations sorted by last message time
pub fn get_conversations_sorted() -> Vec<Conversation> {
    let mut convos: Vec<Conversation> = CONVERSATIONS.read().data().read().values().cloned().collect();

    // Sort by most recent message (uses actual rumor timestamp for NIP-17)
    convos.sort_by(|a, b| {
        let last_a = a.messages.last().map(|m| m.created_at()).unwrap_or_default();
        let last_b = b.messages.last().map(|m| m.created_at()).unwrap_or_default();
        last_b.cmp(&last_a)
    });

    convos
}
