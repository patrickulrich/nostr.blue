//! MDK (Marmot) E2EE messaging store
//! Handles MLS group creation, key packages, and encrypted messaging
//!
//! Type bridging: MDK uses nostr 0.43, nostr.blue uses nostr-sdk 0.44.
//! We use `nostr_mdk` (renamed nostr 0.43) for MDK calls and convert via JSON.

use dioxus::prelude::*;
use mdk_core::prelude::*;
use mdk_memory_storage::MdkMemoryStorage;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;
use chrono::{DateTime, Utc};

use crate::stores::{auth_store, nostr_client};

// nostr 0.43 types for MDK (renamed import)
use nostr_mdk::JsonUtil as MdkJsonUtil;

// nostr-sdk 0.44 types for the application layer
use nostr_sdk::{Event as SdkEvent, EventBuilder as SdkEventBuilder,
    Filter, JsonUtil, Kind, PublicKey as SdkPublicKey, Tag as SdkTag};

/// MLS Group state for UI display
#[derive(Clone, Debug, PartialEq)]
pub struct MlsGroupInfo {
    /// MLS group identifier (bytes)
    pub group_id: Vec<u8>,
    /// Nostr-specific group ID (32 bytes, for event tagging)
    pub nostr_group_id: [u8; 32],
    /// Human-readable group name
    pub name: String,
    /// Group description
    pub description: String,
    /// Group members (as hex strings)
    pub members: Vec<String>,
    /// Whether current user is admin
    pub is_admin: bool,
    /// Group relays
    pub relays: Vec<String>,
}

/// MLS Message for UI display
#[derive(Clone, Debug, PartialEq)]
pub struct MlsMessage {
    /// The wrapper event ID (hex)
    pub event_id: String,
    /// MLS group ID
    pub group_id: Vec<u8>,
    /// Decrypted message content
    pub content: String,
    /// Sender public key (hex)
    pub sender: String,
    /// Timestamp (unix seconds)
    pub created_at: u64,
}

/// MDK instance holder
static MDK_INSTANCE: RwLock<Option<MDK<MdkMemoryStorage>>> = RwLock::new(None);

/// Active MLS groups cache
pub static MLS_GROUPS: GlobalSignal<HashMap<String, MlsGroupInfo>> = Signal::global(|| HashMap::new());

/// MLS messages by group (nostr_group_id hex -> messages)
pub static MLS_MESSAGES: GlobalSignal<HashMap<String, Vec<MlsMessage>>> = Signal::global(|| HashMap::new());

/// Whether MDK has been initialized
pub static MDK_INITIALIZED: GlobalSignal<bool> = Signal::global(|| false);

/// Key package event ID (for deletion/rotation)
pub static KEY_PACKAGE_EVENT_ID: GlobalSignal<Option<String>> = Signal::global(|| None);

/// Key package info for age tracking (rotation after 30 days)
#[derive(Clone, Debug)]
pub struct KeyPackageInfo {
    #[allow(dead_code)] // May be used for deletion/UI display
    pub event_id: String,
    pub published_at: DateTime<Utc>,
}

/// Track key package info (event ID + publish time) for rotation
pub static KEY_PACKAGE_INFO: GlobalSignal<Option<KeyPackageInfo>> = Signal::global(|| None);

/// Initialize MDK with memory storage
pub fn init_mdk() -> Result<(), String> {
    let storage = MdkMemoryStorage::default();
    let mdk = MDK::builder(storage)
        .with_base64_encoding(true)
        .build();

    let mut instance = MDK_INSTANCE.write()
        .map_err(|e| format!("Failed to acquire MDK lock: {}", e))?;
    *instance = Some(mdk);
    *MDK_INITIALIZED.write() = true;

    log::info!("MDK initialized with memory storage");
    Ok(())
}

/// Check if MDK is initialized
pub fn is_initialized() -> bool {
    *MDK_INITIALIZED.read()
}

/// Create and publish a key package for receiving MLS invites
pub async fn publish_key_package(relays: Vec<String>) -> Result<String, String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let signer = client.signer().await
        .map_err(|e| format!("Failed to get signer: {}", e))?;

    // Parse pubkey using nostr_mdk (0.43) for MDK
    let mdk_pubkey = nostr_mdk::PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Parse relay URLs using nostr_mdk
    let relay_urls: Vec<nostr_mdk::RelayUrl> = relays.iter()
        .filter_map(|r| nostr_mdk::RelayUrl::parse(r).ok())
        .collect();

    // Create key package with MDK
    let (content, mdk_tags) = {
        let instance = MDK_INSTANCE.read()
            .map_err(|e| format!("Failed to acquire MDK lock: {}", e))?;
        let mdk = instance.as_ref()
            .ok_or("MDK not initialized")?;

        mdk.create_key_package_for_event(&mdk_pubkey, relay_urls)
            .map_err(|e| format!("Failed to create key package: {}", e))?
    };

    // Convert nostr_mdk tags to nostr-sdk tags via JSON
    let sdk_tags: Vec<SdkTag> = mdk_tags.iter()
        .filter_map(|tag| {
            let tag_vec: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
            if tag_vec.is_empty() {
                None
            } else {
                SdkTag::parse(tag_vec).ok()
            }
        })
        .collect();

    // Build and sign the key package event (kind 443)
    let event = SdkEventBuilder::new(Kind::Custom(443), content)
        .tags(sdk_tags)
        .sign(&signer)
        .await
        .map_err(|e| format!("Failed to sign key package event: {}", e))?;

    let event_id = event.id.to_hex();

    // Publish to relays
    let result = client.send_event(&event).await
        .map_err(|e| format!("Failed to publish key package: {}", e))?;

    log::info!("Published key package: {:?}", result.val);
    *KEY_PACKAGE_EVENT_ID.write() = Some(event_id.clone());

    // Track key package info for rotation (30 day expiry)
    *KEY_PACKAGE_INFO.write() = Some(KeyPackageInfo {
        event_id: event_id.clone(),
        published_at: Utc::now(),
    });

    Ok(event_id)
}

/// Fetch key package for a user
/// First tries user's key package relays (kind 10051), then falls back to default relays
pub async fn fetch_key_package(pubkey_hex: &str) -> Result<SdkEvent, String> {
    let pubkey = SdkPublicKey::parse(pubkey_hex)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Key package filter
    let kp_filter = Filter::new()
        .kind(Kind::Custom(443))
        .author(pubkey)
        .limit(1);

    // First, try to get the user's key package relays (kind 10051)
    let relay_filter = Filter::new()
        .kind(Kind::Custom(10051))
        .author(pubkey)
        .limit(1);

    if let Ok(events) = client.fetch_events(relay_filter, Duration::from_secs(5)).await {
        if let Some(event) = events.first() {
            let key_package_relays: Vec<&str> = event.tags.iter()
                .filter_map(|t| {
                    let parts = t.as_slice();
                    if parts.first().map(|s| s.as_str()) == Some("relay") {
                        parts.get(1).map(|s| s.as_str())
                    } else {
                        None
                    }
                })
                .collect();

            if !key_package_relays.is_empty() {
                log::debug!("Found {} key package relays for {}", key_package_relays.len(), pubkey_hex);

                // Query those specific relays
                if let Ok(kp_events) = client.fetch_events_from(
                    key_package_relays,
                    kp_filter.clone(),
                    Duration::from_secs(10)
                ).await {
                    if let Some(event) = kp_events.into_iter().next() {
                        log::info!("Found key package for {} from their specified relays", pubkey_hex);
                        return Ok(event);
                    }
                }
            }
        }
    }

    // Fall back to general fetch from connected relays
    log::debug!("Querying connected relays for key package: {}", pubkey_hex);
    let events = nostr_client::fetch_events_aggregated(kp_filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch key package: {}", e))?;

    events.into_iter()
        .next()
        .ok_or_else(|| format!("No key package found for {}", pubkey_hex))
}

/// Load existing key package info for the authenticated user
/// Call on startup to check if rotation is needed
pub async fn load_key_package_info() -> Result<(), String> {
    let pubkey = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    match fetch_key_package(&pubkey).await {
        Ok(event) => {
            // Convert nostr timestamp to chrono DateTime
            let published_at = DateTime::from_timestamp(event.created_at.as_secs() as i64, 0)
                .unwrap_or_else(Utc::now);

            *KEY_PACKAGE_INFO.write() = Some(KeyPackageInfo {
                event_id: event.id.to_hex(),
                published_at,
            });

            log::info!("Loaded existing key package from {}", published_at.format("%Y-%m-%d"));
            Ok(())
        }
        Err(e) => {
            // No key package found - that's ok, scheduler will publish one
            log::debug!("No existing key package: {}", e);
            *KEY_PACKAGE_INFO.write() = None;
            Ok(())
        }
    }
}

/// Convert SDK event to MDK event via JSON
fn sdk_to_mdk_event(sdk_event: &SdkEvent) -> Result<nostr_mdk::Event, String> {
    let json = sdk_event.as_json();
    nostr_mdk::Event::from_json(&json)
        .map_err(|e| format!("Failed to convert event: {}", e))
}

/// Convert MDK event to SDK event via JSON
fn mdk_to_sdk_event(mdk_event: &nostr_mdk::Event) -> Result<SdkEvent, String> {
    let json = mdk_event.as_json();
    serde_json::from_str(&json)
        .map_err(|e| format!("Failed to convert event: {}", e))
}

/// Create a new MLS group
pub async fn create_mls_group(
    name: String,
    description: String,
    member_pubkeys: Vec<String>,
    relays: Vec<String>,
) -> Result<MlsGroupInfo, String> {
    let my_pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let mdk_my_pubkey = nostr_mdk::PublicKey::parse(&my_pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let relay_urls: Vec<nostr_mdk::RelayUrl> = relays.iter()
        .filter_map(|r| nostr_mdk::RelayUrl::parse(r).ok())
        .collect();

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let signer = client.signer().await
        .map_err(|e| format!("Failed to get signer: {}", e))?;

    // Fetch and convert key packages for members
    let mut mdk_key_package_events = Vec::new();
    let mut members_without_keypackage = Vec::new();

    for pk_str in &member_pubkeys {
        match fetch_key_package(pk_str).await {
            Ok(sdk_event) => {
                if let Ok(mdk_event) = sdk_to_mdk_event(&sdk_event) {
                    mdk_key_package_events.push(mdk_event);
                }
            }
            Err(e) => {
                log::warn!("Could not fetch key package for {}: {}", pk_str, e);
                // Track members who don't have key packages
                let display = format!("{}...{}", &pk_str[..8], &pk_str[pk_str.len()-8..]);
                members_without_keypackage.push(display);
            }
        }
    }

    // MDK requires at least one key package to create a group
    // Check if we have any key packages to work with
    if mdk_key_package_events.is_empty() {
        if member_pubkeys.is_empty() {
            return Err(
                "At least one member must be added to create an MLS group. \
                Add members who have published their MLS key packages.".to_string()
            );
        } else {
            return Err(format!(
                "None of the selected members have published MLS key packages. \
                Members must publish a key package before they can be invited to MLS groups. \
                Missing key packages: {}",
                members_without_keypackage.join(", ")
            ));
        }
    }

    // Log if some members were skipped due to missing key packages
    if !members_without_keypackage.is_empty() {
        log::warn!(
            "Some members skipped (no key package): {}",
            members_without_keypackage.join(", ")
        );
    }

    // Create the group with MDK
    let result: GroupResult = {
        let instance = MDK_INSTANCE.read()
            .map_err(|e| format!("Failed to acquire MDK lock: {}", e))?;
        let mdk = instance.as_ref()
            .ok_or("MDK not initialized")?;

        let config = NostrGroupConfigData::new(
            name.clone(),
            description.clone(),
            None,
            None,
            None,
            relay_urls.clone(),
            vec![mdk_my_pubkey],
        );

        mdk.create_group(&mdk_my_pubkey, mdk_key_package_events, config)
            .map_err(|e| format!("Failed to create group: {}", e))?
    };

    // Get member list
    let members: Vec<String> = {
        let instance = MDK_INSTANCE.read()
            .map_err(|e| format!("Failed to acquire MDK lock: {}", e))?;
        let mdk = instance.as_ref()
            .ok_or("MDK not initialized")?;

        mdk.get_members(&result.group.mls_group_id)
            .map_err(|e| format!("Failed to get members: {}", e))?
            .iter()
            .map(|pk| pk.to_hex())
            .collect()
    };

    let group_info = MlsGroupInfo {
        group_id: result.group.mls_group_id.as_slice().to_vec(),
        nostr_group_id: result.group.nostr_group_id,
        name: result.group.name.clone(),
        description: result.group.description.clone(),
        members,
        is_admin: true,
        relays: relays.clone(),
    };

    // Send welcome messages (gift-wrapped)
    for rumor in result.welcome_rumors {
        if let Some(recipient_hex) = rumor.tags.iter()
            .find(|t| t.kind() == nostr_mdk::TagKind::p())
            .and_then(|t| t.content())
        {
            if let Ok(recipient) = SdkPublicKey::parse(recipient_hex) {
                // Convert rumor to SDK unsigned event
                let rumor_json = rumor.as_json();
                if let Ok(sdk_rumor) = serde_json::from_str::<nostr_sdk::UnsignedEvent>(&rumor_json) {
                    match SdkEventBuilder::gift_wrap(&signer, &recipient, sdk_rumor, []).await {
                        Ok(gift_wrap) => {
                            if let Err(e) = client.send_event(&gift_wrap).await {
                                log::error!("Failed to send welcome to {}: {}", recipient_hex, e);
                            } else {
                                log::info!("Sent welcome to {}", recipient_hex);
                            }
                        }
                        Err(e) => log::error!("Failed to gift wrap welcome: {}", e),
                    }
                }
            }
        }
    }

    // Update cache
    let group_id_hex = hex::encode(&group_info.nostr_group_id);
    MLS_GROUPS.write().insert(group_id_hex, group_info.clone());

    log::info!("Created MLS group: {}", name);
    Ok(group_info)
}

/// Process incoming MLS group message (kind 445)
pub fn process_mls_message(event: &SdkEvent) -> Result<Option<MlsMessage>, String> {
    let mdk_event = sdk_to_mdk_event(event)?;

    let instance = MDK_INSTANCE.read()
        .map_err(|e| format!("Failed to acquire MDK lock: {}", e))?;
    let mdk = instance.as_ref()
        .ok_or("MDK not initialized")?;

    let result = mdk.process_message(&mdk_event)
        .map_err(|e| format!("Failed to process MLS message: {}", e))?;

    match result {
        MessageProcessingResult::ApplicationMessage(msg) => {
            let mls_msg = MlsMessage {
                event_id: event.id.to_hex(),
                group_id: msg.mls_group_id.as_slice().to_vec(),
                content: msg.content.clone(),
                sender: msg.pubkey.to_hex(),
                created_at: event.created_at.as_secs(),
            };

            let group_id_hex = hex::encode(msg.mls_group_id.as_slice());
            MLS_MESSAGES.write()
                .entry(group_id_hex)
                .or_insert_with(Vec::new)
                .push(mls_msg.clone());

            Ok(Some(mls_msg))
        }
        _ => {
            log::debug!("Processed MLS control message");
            Ok(None)
        }
    }
}

/// Send a message to an MLS group
pub async fn send_mls_message(
    group_id: &[u8],
    content: String,
) -> Result<String, String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let my_pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let mdk_my_pubkey = nostr_mdk::PublicKey::parse(&my_pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Create rumor (kind 9 chat message) using nostr_mdk
    let rumor = nostr_mdk::EventBuilder::new(nostr_mdk::Kind::Custom(9), content.clone())
        .build(mdk_my_pubkey);

    // Convert group_id bytes to GroupId
    let mdk_group_id = GroupId::from_slice(group_id);

    // Encrypt with MDK
    let mdk_event = {
        let instance = MDK_INSTANCE.read()
            .map_err(|e| format!("Failed to acquire MDK lock: {}", e))?;
        let mdk = instance.as_ref()
            .ok_or("MDK not initialized")?;

        mdk.create_message(&mdk_group_id, rumor)
            .map_err(|e| format!("Failed to create MLS message: {}", e))?
    };

    // Convert to SDK event for publishing
    let sdk_event = mdk_to_sdk_event(&mdk_event)?;
    let event_id = sdk_event.id.to_hex();

    let result = client.send_event(&sdk_event).await
        .map_err(|e| format!("Failed to send MLS message: {}", e))?;

    log::info!("Sent MLS message: {:?}", result.val);

    // Add to local cache
    if let Some(ngid) = MLS_GROUPS.read().values()
        .find(|g| g.group_id == group_id)
        .map(|g| g.nostr_group_id)
    {
        let msg = MlsMessage {
            event_id: event_id.clone(),
            group_id: group_id.to_vec(),
            content,
            sender: my_pubkey_str,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        MLS_MESSAGES.write()
            .entry(hex::encode(ngid))
            .or_insert_with(Vec::new)
            .push(msg);
    }

    Ok(event_id)
}

/// Fetch and process MLS messages for all groups
pub async fn fetch_mls_messages() -> Result<usize, String> {
    let groups = MLS_GROUPS.read().clone();

    if groups.is_empty() {
        return Ok(0);
    }

    let mut count = 0;

    for group in groups.values() {
        let group_id_hex = hex::encode(group.nostr_group_id);

        let filter = Filter::new()
            .kind(Kind::Custom(445))
            .custom_tag(
                nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::H),
                group_id_hex
            )
            .limit(100);

        let events = match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Failed to fetch MLS messages for group: {}", e);
                continue;
            }
        };

        for event in events {
            match process_mls_message(&event) {
                Ok(Some(_)) => count += 1,
                Ok(None) => {}
                Err(e) => log::warn!("Failed to process MLS message: {}", e),
            }
        }
    }

    log::info!("Processed {} MLS messages", count);
    Ok(count)
}

/// Get all groups
pub fn get_groups() -> Vec<MlsGroupInfo> {
    MLS_GROUPS.read().values().cloned().collect()
}

/// Get a specific group
pub fn get_group(nostr_group_id_hex: &str) -> Option<MlsGroupInfo> {
    MLS_GROUPS.read().get(nostr_group_id_hex).cloned()
}

/// Get messages for a group
pub fn get_messages(nostr_group_id_hex: &str) -> Vec<MlsMessage> {
    MLS_MESSAGES.read()
        .get(nostr_group_id_hex)
        .cloned()
        .unwrap_or_default()
}

/// Process an MLS welcome message (kind 444) to join a group
pub fn process_welcome(
    wrapper_event_id: &nostr_mdk::EventId,
    welcome_rumor: &nostr_mdk::UnsignedEvent
) -> Result<MlsGroupInfo, String> {
    let instance = MDK_INSTANCE.read()
        .map_err(|e| format!("Failed to acquire MDK lock: {}", e))?;
    let mdk = instance.as_ref()
        .ok_or("MDK not initialized")?;

    // Process the welcome with MDK
    let welcome = mdk.process_welcome(wrapper_event_id, welcome_rumor)
        .map_err(|e| format!("Failed to process welcome: {}", e))?;

    // Get group members
    let members: Vec<String> = mdk.get_members(&welcome.mls_group_id)
        .map_err(|e| format!("Failed to get members: {}", e))?
        .iter()
        .map(|pk| pk.to_hex())
        .collect();

    let group_info = MlsGroupInfo {
        group_id: welcome.mls_group_id.as_slice().to_vec(),
        nostr_group_id: welcome.nostr_group_id,
        name: welcome.group_name.clone(),
        description: welcome.group_description.clone(),
        members,
        is_admin: false, // We're joining, not creating
        relays: vec![], // Relays not available in welcome
    };

    // Add to cache
    let group_id_hex = hex::encode(&group_info.nostr_group_id);
    MLS_GROUPS.write().insert(group_id_hex, group_info.clone());

    log::info!("Joined MLS group via welcome: {}", welcome.group_name);
    Ok(group_info)
}

/// Fetch and process MLS welcome messages (kind 444 gift-wrapped)
pub async fn fetch_and_process_welcomes() -> Result<usize, String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let pubkey = SdkPublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Fetch gift-wrapped welcomes (kind 1059 with welcome rumor inside)
    // MLS welcomes are sent as gift wraps with kind 444 rumor
    let filter = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(pubkey)
        .limit(50);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch welcomes: {}", e))?;

    log::info!("Fetched {} potential welcome gift wraps", events.len());

    let mut count = 0;

    for event in events {
        // Unwrap the gift wrap
        match client.unwrap_gift_wrap(&event).await {
            Ok(unwrapped) => {
                // Check if this is an MLS welcome (kind 444)
                if unwrapped.rumor.kind == Kind::Custom(444) {
                    log::info!("Found MLS welcome message");

                    // Convert wrapper event ID to nostr_mdk format
                    let wrapper_id_hex = event.id.to_hex();
                    let mdk_wrapper_id = match nostr_mdk::EventId::parse(&wrapper_id_hex) {
                        Ok(id) => id,
                        Err(e) => {
                            log::warn!("Failed to parse wrapper event ID: {}", e);
                            continue;
                        }
                    };

                    // Convert the SDK unsigned event to nostr_mdk format for MDK
                    let rumor_json = serde_json::to_string(&unwrapped.rumor)
                        .map_err(|e| format!("Failed to serialize rumor: {}", e))?;

                    if let Ok(mdk_rumor) = nostr_mdk::UnsignedEvent::from_json(&rumor_json) {
                        match process_welcome(&mdk_wrapper_id, &mdk_rumor) {
                            Ok(group) => {
                                log::info!("Successfully joined group: {}", group.name);
                                count += 1;
                            }
                            Err(e) => {
                                log::warn!("Failed to process welcome: {}", e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                // Not all gift wraps are for us or valid - this is normal
                log::debug!("Could not unwrap gift wrap: {}", e);
            }
        }
    }

    log::info!("Processed {} MLS welcome messages", count);
    Ok(count)
}
