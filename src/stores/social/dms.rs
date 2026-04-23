use crate::error::NostrBlueError;
use crate::stores::nostr_client::PublishResult;
use crate::stores::{auth_store, nostr_client, relay};
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use instant::Instant;
use lru::LruCache;
use nostr_sdk::nips::nip17;
use nostr_sdk::{Event, EventId, Filter, Kind, PublicKey, Timestamp, UnsignedEvent};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const MAX_DECRYPT_CACHE_SIZE: usize = 500;

/// Cached decryption result for NIP-04 messages
enum DecryptResult {
    Success(String),
    RetryableFailure(Instant),
}

static DECRYPT_CACHE: OnceLock<Mutex<LruCache<EventId, DecryptResult>>> = OnceLock::new();

fn get_decrypt_cache() -> &'static Mutex<LruCache<EventId, DecryptResult>> {
    DECRYPT_CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(MAX_DECRYPT_CACHE_SIZE).unwrap(),
        ))
    })
}

const RETRY_COOLDOWN_SECS: u64 = 10;

async fn recipient_has_inbox_relays(client: &nostr_sdk::Client, recipient_pk: PublicKey) -> bool {
    let filter = Filter::new()
        .author(recipient_pk)
        .kind(Kind::InboxRelays)
        .limit(1);

    let cached_events = client
        .database()
        .query(filter.clone())
        .await
        .unwrap_or_default();

    if !cached_events.is_empty() {
        if let Some(event) = cached_events.into_iter().next() {
            return nip17::extract_relay_list(&event).next().is_some();
        }
        return false;
    }

    match client.fetch_events(filter, Duration::from_secs(3)).await {
        Ok(events) if !events.is_empty() => {
            if let Some(event) = events.into_iter().next() {
                nip17::extract_relay_list(&event).next().is_some()
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Clear DM caches (called on logout/account switch)
pub fn clear_caches() {
    CONVERSATIONS.read().data().write().clear();
    get_decrypt_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
}

/// Represents a message in a conversation, handling NIP-04 and NIP-17
#[derive(Clone, Debug, PartialEq)]
pub enum ConversationMessage {
    /// NIP-04 legacy encrypted direct message
    Nip04 { event: Event },
    /// NIP-17 gift wrap message with unwrapped data
    Nip17 {
        gift_wrap: Event,
        rumor: UnsignedEvent,
        sender: PublicKey,
    },
}
impl ConversationMessage {
    /// Get a unique identifier for this message (for keying in UI lists)
    pub fn id(&self) -> EventId {
        match self {
            Self::Nip04 { event } => event.id,
            Self::Nip17 { gift_wrap, .. } => gift_wrap.id,
        }
    }
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
    /// Get the encryption type for display
    pub fn encryption_type(&self) -> &'static str {
        match self {
            Self::Nip04 { .. } => "NIP-04",
            Self::Nip17 { .. } => "NIP-17",
        }
    }
    pub fn tags(&self) -> &nostr_sdk::Tags {
        match self {
            Self::Nip04 { event } => &event.tags,
            Self::Nip17 { rumor, .. } => &rumor.tags,
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
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    log::info!("Loading DMs for {}", pubkey_str);
    let dm_relays = relay::specialty::ensure_dm_relays_connected(&client).await;
    if dm_relays.is_empty() {
        CONVERSATIONS.read().data().write().clear();
        return Err("No DM relays could be connected".to_string());
    }
    log::info!(
        "Using {} connected DM relays: {:?}",
        dm_relays.len(),
        dm_relays
    );
    let received_nip04 = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .pubkey(pubkey)
        .limit(200);
    let sent_nip04 = Filter::new()
        .kind(Kind::EncryptedDirectMessage)
        .author(pubkey)
        .limit(200);
    let nip17_all = Filter::new().kind(Kind::GiftWrap).pubkey(pubkey).limit(300);
    let (received_nip04_result, sent_nip04_result, nip17_all_result) = tokio::join!(
        relay::connection::fetch_events_from_relays(
            &client,
            received_nip04,
            dm_relays.clone(),
            Duration::from_secs(10)
        ),
        relay::connection::fetch_events_from_relays(
            &client,
            sent_nip04,
            dm_relays.clone(),
            Duration::from_secs(10)
        ),
        relay::connection::fetch_events_from_relays(
            &client,
            nip17_all,
            dm_relays.clone(),
            Duration::from_secs(10)
        )
    );
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
        log::info!(
            "Fetched {} NIP-17 DMs (gift wraps - both received and sent)",
            events.len()
        );
        all_messages.extend(events);
    } else if let Err(e) = nip17_all_result {
        log::error!("Failed to fetch NIP-17 DMs: {}", e);
    }
    log::info!("Loaded {} total DM events", all_messages.len());
    let mut conversations: HashMap<String, Conversation> = HashMap::new();
    for msg in all_messages {
        if msg.kind == Kind::GiftWrap {
            match client.unwrap_gift_wrap(&msg).await {
                Ok(unwrapped) => {
                    if unwrapped.rumor.kind == Kind::PrivateDirectMessage {
                        let sender_pubkey = unwrapped.sender.to_string();
                        let other_pubkey = if sender_pubkey == pubkey_str {
                            unwrapped
                                .rumor
                                .tags
                                .iter()
                                .find(|tag| tag.kind() == nostr_sdk::TagKind::p())
                                .and_then(|tag| tag.content())
                                .unwrap_or_default()
                                .to_string()
                        } else {
                            sender_pubkey
                        };
                        if other_pubkey.is_empty() {
                            log::warn!(
                                "Failed to determine conversation partner for NIP-17 message"
                            );
                            continue;
                        }
                        let conversation_msg = ConversationMessage::Nip17 {
                            gift_wrap: msg,
                            rumor: unwrapped.rumor,
                            sender: unwrapped.sender,
                        };
                        conversations
                            .entry(other_pubkey.clone())
                            .or_insert_with(|| Conversation {
                                pubkey: other_pubkey.clone(),
                                messages: Vec::new(),
                                unread_count: 0,
                            })
                            .messages
                            .push(conversation_msg);
                    }
                }
                Err(e) => {
                    log::error!("Failed to unwrap gift wrap: {}", e);
                    continue;
                }
            }
        } else {
            let other_pubkey = if msg.pubkey.to_string() == pubkey_str {
                msg.tags
                    .iter()
                    .find(|tag| tag.kind() == nostr_sdk::TagKind::p())
                    .and_then(|tag| tag.content())
                    .unwrap_or_default()
                    .to_string()
            } else {
                msg.pubkey.to_string()
            };
            if other_pubkey.is_empty() {
                continue;
            }
            let conversation_msg = ConversationMessage::Nip04 { event: msg };
            conversations
                .entry(other_pubkey.clone())
                .or_insert_with(|| Conversation {
                    pubkey: other_pubkey.clone(),
                    messages: Vec::new(),
                    unread_count: 0,
                })
                .messages
                .push(conversation_msg);
        }
    }
    for conversation in conversations.values_mut() {
        conversation.messages.sort_by_key(|a| a.created_at());
    }
    log::info!("Organized into {} conversations", conversations.len());
    *CONVERSATIONS.read().data().write() = conversations;
    Ok(())
}
/// Send an encrypted DM to a recipient (NIP-17 compliant with sender copy)
/// Returns PublishResult with combined relay statistics from both gift wraps
///
/// Per NIP-17, this sends to inbox relays (Kind 10050) for privacy:
/// - Receiver's gift wrap -> recipient's NIP-17 relays (via SDK gossip routing)
/// - Sender's copy -> sender's NIP-17 relays (via SDK gossip routing)
///
/// The SDK's gossip feature automatically routes gift wraps to the correct
/// NIP-17 relays based on the p-tag, eliminating the need for manual relay fetching.
pub async fn send_dm(recipient_pubkey: String, content: String) -> Result<PublishResult, String> {
    use nostr_sdk::EventBuilder;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let recipient_pk = PublicKey::parse(&recipient_pubkey)
        .map_err(|e| format!("Invalid recipient pubkey: {}", e))?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let sender_pk = crate::stores::nostr_client::get_cached_pubkey()
        .map_err(|e| format!("Failed to get sender pubkey: {}", e))?;
    log::info!(
        "Sending DM from {} to {}",
        sender_pk.to_hex(),
        recipient_pubkey
    );
    nostr_client::ensure_relays_ready(&client).await;
    let has_inbox_relays = recipient_has_inbox_relays(&client, recipient_pk).await;
    if !has_inbox_relays {
        return Err(
            "Recipient has no inbox relays configured (NIP-17 kind 10050). \
                   They may not be able to receive private messages. \
                   Ask them to set up inbox relays in their Nostr client."
                .to_string(),
        );
    }
    let rumor = crate::utils::nips::nip89::tag_event_builder(EventBuilder::private_msg_rumor(
        recipient_pk,
        content.clone(),
    ))
    .build(sender_pk);
    let receiver_gift_wrap = EventBuilder::gift_wrap(&signer, &recipient_pk, rumor.clone(), [])
        .await
        .map_err(|e| format!("Failed to create receiver gift wrap: {}", e))?;
    let sender_gift_wrap = EventBuilder::gift_wrap(&signer, &sender_pk, rumor, [])
        .await
        .map_err(|e| format!("Failed to create sender gift wrap: {}", e))?;
    let receiver_event_id = receiver_gift_wrap.id.to_hex();
    crate::stores::publish_queue::enqueue(
        receiver_gift_wrap,
        crate::stores::publish_queue::types::QueueEventType::DirectMessage,
        None,
        std::collections::HashMap::new(),
    ).await;
    crate::stores::publish_queue::enqueue(
        sender_gift_wrap,
        crate::stores::publish_queue::types::QueueEventType::DirectMessage,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("DM enqueued: {}", receiver_event_id);
    if let Err(e) = init_dms().await {
        log::error!(
            "Failed to refresh DM conversations after sending message: {}",
            e
        );
    }
    Ok(PublishResult {
        event_id: receiver_event_id,
        queue_id: None,
        queued: true,
        successful_relays: vec![],
        failed_relays: vec![],
    })
}

/// Send an encrypted DM using a temporary generated key instead of the current
/// app signer. This is useful for one-off contact/reporting flows that should
/// work even when the user is not signed in.
pub async fn send_dm_with_temporary_keys(
    recipient_pubkey: String,
    content: String,
) -> Result<PublishResult, String> {
    use nostr::Keys;
    use nostr_sdk::EventBuilder;

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let recipient_pk = PublicKey::parse(&recipient_pubkey)
        .map_err(|e| format!("Invalid recipient pubkey: {}", e))?;

    nostr_client::ensure_relays_ready(&client).await;
    let has_inbox_relays = recipient_has_inbox_relays(&client, recipient_pk).await;
    if !has_inbox_relays {
        return Err(
            "Recipient has no inbox relays configured (NIP-17 kind 10050). \
                   They may not be able to receive private messages."
                .to_string(),
        );
    }

    let temp_keys = Keys::generate();
    let sender_pk = temp_keys.public_key();
    log::info!(
        "Sending DM from temporary key {} to {}",
        sender_pk.to_hex(),
        recipient_pubkey
    );

    let rumor = crate::utils::nips::nip89::tag_event_builder(EventBuilder::private_msg_rumor(
        recipient_pk,
        content,
    ))
    .build(sender_pk);
    let receiver_gift_wrap = EventBuilder::gift_wrap(&temp_keys, &recipient_pk, rumor, [])
        .await
        .map_err(|e| format!("Failed to create receiver gift wrap: {}", e))?;

    let receiver_event_id = receiver_gift_wrap.id.to_hex();
    crate::stores::publish_queue::enqueue(
        receiver_gift_wrap,
        crate::stores::publish_queue::types::QueueEventType::DirectMessage,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Temporary-key DM enqueued: {}", receiver_event_id);
    Ok(PublishResult {
        event_id: receiver_event_id,
        queue_id: None,
        queued: true,
        successful_relays: vec![],
        failed_relays: vec![],
    })
}
/// Decrypt a DM message (supports NIP-04 and NIP-17)
/// NIP-04 results are cached to avoid repeated signer popup spam.
pub async fn decrypt_dm(msg: &ConversationMessage) -> Result<String, NostrBlueError> {
    if let ConversationMessage::Nip17 { rumor, .. } = msg {
        log::debug!("Returning NIP-17 message content from rumor");
        return Ok(rumor.content.clone());
    }
    let ConversationMessage::Nip04 { event } = msg else {
        return Err(NostrBlueError::Other("Invalid message type".to_string()));
    };
    if event.kind != Kind::EncryptedDirectMessage {
        return Err(NostrBlueError::Other(format!(
            "Unsupported message kind: {:?}",
            event.kind
        )));
    }
    // Unencrypted NIP-04 content (no ?iv= parameter)
    if !event.content.contains("?iv=") {
        return Ok(event.content.clone());
    }
    // Check decrypt cache first
    {
        let mut cache = get_decrypt_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.get(&event.id) {
            match cached {
                DecryptResult::Success(content) => return Ok(content.clone()),
                DecryptResult::RetryableFailure(failed_at) => {
                    if failed_at.elapsed().as_secs() < RETRY_COOLDOWN_SECS {
                        return Err(NostrBlueError::Other(
                            "Decryption failed, retrying soon".to_string(),
                        ));
                    }
                    // Cooldown expired, fall through to retry
                }
            }
        }
    }
    let my_pubkey = auth_store::get_pubkey().ok_or(NostrBlueError::NotAuthenticated)?;
    let other_pubkey = if event.pubkey.to_string() == my_pubkey {
        event
            .tags
            .iter()
            .find(|tag| tag.kind() == nostr_sdk::TagKind::p())
            .and_then(|tag| tag.content())
            .ok_or(NostrBlueError::Other(
                "No recipient found in sent message".to_string(),
            ))?
            .to_string()
    } else {
        event.pubkey.to_string()
    };
    let other_pk = PublicKey::parse(&other_pubkey)?;
    let signer_result = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or(NostrBlueError::Other("Client not initialized".to_string()))?
        .signer()
        .await;
    let signer = match signer_result {
        Ok(s) => s,
        Err(_) => {
            return Err(NostrBlueError::SignerNotAvailable);
        }
    };
    match signer.nip04_decrypt(&other_pk, &event.content).await {
        Ok(decrypted) => {
            log::debug!("Successfully decrypted NIP-04 message");
            let mut cache = get_decrypt_cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            cache.put(event.id, DecryptResult::Success(decrypted.clone()));
            Ok(decrypted)
        }
        Err(e) => {
            log::error!("Failed to decrypt NIP-04 message: {}", e);
            let mut cache = get_decrypt_cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            cache.put(event.id, DecryptResult::RetryableFailure(Instant::now()));
            Err(NostrBlueError::Other(format!(
                "Failed to decrypt NIP-04 message: {}",
                e
            )))
        }
    }
}
/// Get a specific conversation
pub fn get_conversation(pubkey: &str) -> Option<Conversation> {
    CONVERSATIONS.read().data().read().get(pubkey).cloned()
}
/// Get all conversations sorted by last message time
pub fn get_conversations_sorted() -> Vec<Conversation> {
    let mut convos: Vec<Conversation> = CONVERSATIONS
        .read()
        .data()
        .read()
        .values()
        .cloned()
        .collect();
    convos.sort_by(|a, b| {
        let last_a = a
            .messages
            .last()
            .map(|m| m.created_at())
            .unwrap_or_default();
        let last_b = b
            .messages
            .last()
            .map(|m| m.created_at())
            .unwrap_or_default();
        last_b.cmp(&last_a)
    });
    convos
}
