use crate::stores::{auth_store, nostr_client};
use crate::utils::list_encryption::get_all_list_members_with_status;
use crate::utils::list_kinds::{get_p_tag_count, LIST_KINDS, NAMED_PEOPLE};
use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey};
use std::time::Duration;
/// User list data structure
#[derive(Clone, Debug, PartialEq)]
pub struct UserList {
    pub id: String,
    pub kind: u16,
    pub name: String,
    pub description: String,
    pub identifier: String,
    pub tags: Vec<nostr_sdk::Tag>,
    pub created_at: u64,
    pub author: String,
    pub event: Event,
    /// Indicates if the list has encrypted private content (NIP-44)
    pub has_private_content: bool,
    /// Cached total member count including private members (populated after decryption)
    pub total_member_count: Option<usize>,
}
impl UserList {
    /// Create a UserList from a Nostr event
    pub fn from_event(event: Event) -> Option<Self> {
        let identifier = event
            .tags
            .iter()
            .find(|tag| tag.kind() == nostr_sdk::TagKind::d())
            .and_then(|tag| tag.content())
            .map(|s| s.to_string())?;
        let name = event
            .tags
            .iter()
            .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("name"))
            .and_then(|tag| tag.content())
            .map(|s| s.to_string())
            .or_else(|| {
                event
                    .tags
                    .iter()
                    .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("title"))
                    .and_then(|tag| tag.content())
                    .map(|s| s.to_string())
            })
            .or_else(|| Some(identifier.clone()))
            .unwrap_or_else(|| "Untitled List".to_string());
        let description = event
            .tags
            .iter()
            .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("description"))
            .and_then(|tag| tag.content())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let has_private_content = !event.content.is_empty();
        Some(UserList {
            id: event.id.to_string(),
            kind: event.kind.as_u16(),
            name,
            description,
            identifier,
            tags: event.tags.clone().to_vec(),
            created_at: event.created_at.as_secs(),
            author: event.pubkey.to_string(),
            has_private_content,
            total_member_count: None,
            event,
        })
    }
}
/// Hook to fetch all user lists (NIP-51)
/// Returns (lists, loading, error, refresh)
#[allow(clippy::type_complexity)]
pub fn use_user_lists() -> (
    Signal<Vec<UserList>>,
    Signal<bool>,
    Signal<Option<String>>,
    Signal<u32>,
) {
    let mut lists = use_signal(Vec::<UserList>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let refresh_trigger = use_signal(|| 0u32);
    use_effect(move || {
        let _trigger = refresh_trigger.read();
        let client_ready = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = *nostr_client::HAS_SIGNER.read();
        let auth = auth_store::AUTH_STATE.read();
        if !auth.is_authenticated {
            lists.set(Vec::new());
            return;
        }
        if !client_ready {
            return;
        }
        // Wait for signer before fetching lists (needed for private list decryption)
        // ReadOnly (npub) users don't need a signer - they can still see public list info
        let requires_signer = matches!(
            auth.login_method,
            Some(auth_store::LoginMethod::BrowserExtension)
                | Some(auth_store::LoginMethod::PrivateKey)
                | Some(auth_store::LoginMethod::RemoteSigner)
        ) || {
            #[cfg(feature = "mobile")]
            {
                matches!(
                    auth.login_method,
                    Some(auth_store::LoginMethod::AndroidSigner)
                )
            }
            #[cfg(not(feature = "mobile"))]
            {
                false
            }
        };
        if requires_signer && !has_signer {
            log::debug!("Waiting for signer before fetching lists...");
            return;
        }
        let pubkey_str = match &auth.pubkey {
            Some(pk) => pk.clone(),
            None => return,
        };
        loading.set(true);
        error.set(None);
        spawn(async move {
            match fetch_user_lists(&pubkey_str).await {
                Ok(fetched_lists) => {
                    lists.set(fetched_lists);
                    error.set(None);
                }
                Err(e) => {
                    log::error!("Failed to fetch user lists: {}", e);
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });
    (lists, loading, error, refresh_trigger)
}
/// Fetch user lists from relays
async fn fetch_user_lists(pubkey_str: &str) -> Result<Vec<UserList>, String> {
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    // Wait for user relay lists if signer is present (critical for NIP-46
    // where signer restoration triggers HAS_SIGNER before relays are applied)
    crate::stores::relay::wait_for_user_relays(
        std::time::Duration::from_secs(5),
        "fetch_user_lists",
    )
    .await;
    nostr_client::ensure_relays_ready(&client).await;
    let pubkey = PublicKey::parse(pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    log::info!("Fetching lists for {}", pubkey_str);
    let filter = Filter::new()
        .author(pubkey)
        .kinds(LIST_KINDS.iter().map(|&k| Kind::from(k)));
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch events: {}", e))?;
    let mut lists: Vec<UserList> = events
        .into_iter()
        .filter_map(UserList::from_event)
        .collect();
    for list in &mut lists {
        if list.kind == NAMED_PEOPLE && !list.has_private_content {
            list.total_member_count = Some(get_p_tag_count(&list.tags));
        }
    }
    if *nostr_client::HAS_SIGNER.peek() {
        let private_indices: Vec<usize> = lists
            .iter()
            .enumerate()
            .filter(|(_, list)| list.kind == NAMED_PEOPLE && list.has_private_content)
            .map(|(i, _)| i)
            .collect();
        if !private_indices.is_empty() {
            let mut futures = Vec::with_capacity(private_indices.len());
            for &idx in &private_indices {
                futures.push(get_all_list_members_with_status(&lists[idx].event));
            }
            let results = futures::future::join_all(futures).await;
            for (idx, result) in private_indices.into_iter().zip(results) {
                match result {
                    Ok(r) => {
                        if r.private_decryption_succeeded {
                            lists[idx].total_member_count = Some(r.members.len());
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to get members for list '{}': {}",
                            lists[idx].name,
                            e
                        );
                    }
                }
            }
        }
    }
    lists.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    log::info!("Fetched {} lists", lists.len());
    Ok(lists)
}
/// Delete a list by publishing a deletion event (kind 5)
pub async fn delete_list(event: &Event) -> Result<(), String> {
    use nostr_sdk::{EventBuilder, Kind, Tag, TagStandard};
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !auth_store::is_authenticated() {
        return Err("Must be logged in to delete lists".to_string());
    }
    log::info!("Deleting list: {}", event.id);
    let tags = vec![
        Tag::event(event.id),
        Tag::from_standardized(TagStandard::Kind {
            kind: event.kind,
            uppercase: false,
        }),
    ];
    let builder = EventBuilder::new(Kind::EventDeletion, "Deleted list").tags(tags);
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish deletion: {}", e))?;
    log::info!("List deleted successfully");
    Ok(())
}
