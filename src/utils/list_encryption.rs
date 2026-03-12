//! NIP-51 List Encryption Utilities
//!
//! Handles encrypting/decrypting private list members using NIP-44.
//! Private members are stored as a JSON array in the event content,
//! encrypted to the user's own public key.
use crate::stores::nostr_client;
use crate::utils::list_kinds::NAMED_PEOPLE;
use nostr_sdk::{Event, EventBuilder, Kind, PublicKey, Tag};
/// Decrypt private tags from a list event's content
///
/// Private tags are stored as a NIP-44 encrypted JSON array in the event content.
/// The array format is: `[["p", "pubkey1"], ["p", "pubkey2"], ...]`
pub async fn decrypt_private_tags(event: &Event) -> Result<Vec<Tag>, String> {
    if event.content.is_empty() {
        return Ok(Vec::new());
    }
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {}", e))?;
    let decrypted = signer
        .nip44_decrypt(&event.pubkey, &event.content)
        .await
        .map_err(|e| format!("Failed to decrypt: {}", e))?;
    if decrypted.is_empty() || decrypted == "[]" {
        return Ok(Vec::new());
    }
    let tag_arrays: Vec<Vec<String>> = serde_json::from_str(&decrypted)
        .map_err(|e| format!("Invalid private tags format: {}", e))?;
    let tags: Vec<Tag> = tag_arrays
        .iter()
        .filter_map(|arr| {
            if arr.is_empty() {
                return None;
            }
            match arr[0].as_str() {
                "p" if arr.len() >= 2 => PublicKey::parse(&arr[1]).ok().map(Tag::public_key),
                _ => None,
            }
        })
        .collect();
    Ok(tags)
}
/// Encrypt tags for private storage in event content
///
/// Tags are serialized to JSON array format and encrypted using NIP-44.
pub async fn encrypt_private_tags(tags: &[Tag]) -> Result<String, String> {
    if tags.is_empty() {
        return Ok(String::new());
    }
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {}", e))?;
    let pubkey = nostr_client::get_cached_pubkey()?;
    let tag_arrays: Vec<Vec<String>> = tags
        .iter()
        .map(|tag| tag.as_slice().iter().map(|s| s.to_string()).collect())
        .collect();
    let json =
        serde_json::to_string(&tag_arrays).map_err(|e| format!("Failed to serialize: {}", e))?;
    signer
        .nip44_encrypt(&pubkey, &json)
        .await
        .map_err(|e| format!("Failed to encrypt: {}", e))
}
/// Add a person to a list, optionally as a private (encrypted) member
///
/// Returns the new event after publishing to relays.
pub async fn add_person_to_list(
    list_event: &Event,
    person_pubkey: &str,
    is_private: bool,
) -> Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let target_pubkey =
        PublicKey::parse(person_pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let mut public_tags: Vec<Tag> = list_event.tags.clone().into_iter().collect();
    let mut private_tags = decrypt_private_tags(list_event).await.map_err(|e| {
        log::error!("Failed to decrypt private tags: {}", e);
        format!(
            "Cannot modify list: failed to decrypt private members ({})",
            e
        )
    })?;
    let pubkey_hex = target_pubkey.to_hex();
    let already_public = public_tags.iter().any(|tag| {
        tag.kind() == nostr_sdk::TagKind::p()
            && tag
                .content()
                .map(|c| c.eq_ignore_ascii_case(&pubkey_hex))
                .unwrap_or(false)
    });
    let already_private = private_tags.iter().any(|tag| {
        tag.kind() == nostr_sdk::TagKind::p()
            && tag
                .content()
                .map(|c| c.eq_ignore_ascii_case(&pubkey_hex))
                .unwrap_or(false)
    });
    if already_public || already_private {
        return Err("Person is already in this list".to_string());
    }
    let new_tag = Tag::public_key(target_pubkey);
    if is_private {
        private_tags.push(new_tag);
    } else {
        public_tags.push(new_tag);
    }
    let encrypted_content = encrypt_private_tags(&private_tags).await?;
    let builder = EventBuilder::new(list_event.kind, encrypted_content).tags(public_tags);
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to update list: {}", e))?;
    Ok(())
}
/// Remove a person from a list (checks both public and private members)
pub async fn remove_person_from_list(
    list_event: &Event,
    person_pubkey: &str,
) -> Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let target_pubkey =
        PublicKey::parse(person_pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let pubkey_hex = target_pubkey.to_hex();
    let public_tags: Vec<Tag> = list_event
        .tags
        .clone()
        .into_iter()
        .filter(|tag| {
            !(tag.kind() == nostr_sdk::TagKind::p()
                && tag
                    .content()
                    .map(|c| c.eq_ignore_ascii_case(&pubkey_hex))
                    .unwrap_or(false))
        })
        .collect();
    let private_tags: Vec<Tag> = decrypt_private_tags(list_event)
        .await
        .map_err(|e| {
            log::error!("Failed to decrypt private tags: {}", e);
            format!(
                "Cannot modify list: failed to decrypt private members ({})",
                e
            )
        })?
        .into_iter()
        .filter(|tag| {
            !(tag.kind() == nostr_sdk::TagKind::p()
                && tag
                    .content()
                    .map(|c| c.eq_ignore_ascii_case(&pubkey_hex))
                    .unwrap_or(false))
        })
        .collect();
    let encrypted_content = encrypt_private_tags(&private_tags).await?;
    let builder = EventBuilder::new(list_event.kind, encrypted_content).tags(public_tags);
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to update list: {}", e))?;
    Ok(())
}
/// Result of getting list members with decryption status
pub struct ListMembersResult {
    pub members: Vec<PublicKey>,
    /// Whether private member decryption succeeded (false if decryption failed)
    pub private_decryption_succeeded: bool,
}
/// Get all members of a list (public + decrypted private)
///
/// This is the main function used when loading a feed from a people list.
pub async fn get_all_list_members(list_event: &Event) -> Result<Vec<PublicKey>, String> {
    let result = get_all_list_members_with_status(list_event).await?;
    Ok(result.members)
}
/// Get all members with indication of whether private decryption succeeded
///
/// Use this when you need to know if the member count is accurate or partial.
/// If `private_decryption_succeeded` is false, the returned members are only
/// the public ones (private members could not be decrypted).
pub async fn get_all_list_members_with_status(
    list_event: &Event,
) -> Result<ListMembersResult, String> {
    let mut members = Vec::new();
    for tag in list_event.tags.iter() {
        if tag.kind() == nostr_sdk::TagKind::p() {
            if let Some(pk_str) = tag.content() {
                if let Ok(pk) = PublicKey::parse(pk_str) {
                    members.push(pk);
                }
            }
        }
    }
    let (private_tags, decryption_succeeded) = match decrypt_private_tags(list_event).await {
        Ok(tags) => (tags, true),
        Err(e) => {
            log::warn!(
                "Failed to decrypt private members, showing public only: {}",
                e
            );
            (Vec::new(), false)
        }
    };
    for tag in private_tags {
        if tag.kind() == nostr_sdk::TagKind::p() {
            if let Some(pk_str) = tag.content() {
                if let Ok(pk) = PublicKey::parse(pk_str) {
                    if !members.contains(&pk) {
                        members.push(pk);
                    }
                }
            }
        }
    }
    Ok(ListMembersResult {
        members,
        private_decryption_succeeded: decryption_succeeded,
    })
}
/// Create a new people list with optional initial members
///
/// If `is_private_default` is true, an empty encrypted content field is created
/// to establish the pattern for future private members.
///
/// Returns the created Event on success.
pub async fn create_people_list(
    name: String,
    description: Option<String>,
    is_private_default: bool,
) -> Result<Event, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {}", e))?;
    let name = name.trim();
    if name.is_empty() {
        return Err("List name cannot be empty".to_string());
    }
    if name.len() > 100 {
        return Err("List name too long (max 100 characters)".to_string());
    }
    let unique_id = uuid::Uuid::new_v4().to_string();
    let mut tags = vec![
        Tag::identifier(&unique_id),
        Tag::custom(
            nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("name")),
            vec![name.to_string()],
        ),
        Tag::custom(
            nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("alt")),
            vec!["List of people".to_string()],
        ),
    ];
    if let Some(desc) = description {
        let desc = desc.trim();
        if !desc.is_empty() {
            tags.push(Tag::custom(
                nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("description")),
                vec![desc.to_string()],
            ));
        }
    }
    let content = if is_private_default {
        let pubkey = signer
            .get_public_key()
            .await
            .map_err(|e| format!("Failed to get pubkey: {}", e))?;
        signer
            .nip44_encrypt(&pubkey, "[]")
            .await
            .map_err(|e| format!("Failed to encrypt: {}", e))?
    } else {
        String::new()
    };
    let builder = EventBuilder::new(Kind::from(NAMED_PEOPLE), content).tags(tags);
    let event = client
        .sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign list event: {}", e))?;
    client
        .send_event(&event)
        .await
        .map_err(|e| format!("Failed to publish list: {}", e))?;
    log::info!("Created new people list: {}", name);
    Ok(event)
}
