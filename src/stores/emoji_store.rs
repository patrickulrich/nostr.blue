use dioxus::prelude::*;
use nostr_sdk::{Filter, Kind, Timestamp, PublicKey};

/// Custom emoji from Nostr (NIP-30 format)
#[derive(Clone, Debug, PartialEq)]
pub struct CustomEmoji {
    pub shortcode: String,
    pub image_url: String,
}

/// Emoji set (kind 30030) from Nostr
#[derive(Clone, Debug, PartialEq)]
pub struct EmojiSet {
    pub identifier: String,
    pub name: Option<String>,
    pub emojis: Vec<CustomEmoji>,
    pub author: String,
}

/// Global state for custom emojis from Nostr
pub static CUSTOM_EMOJIS: GlobalSignal<Vec<CustomEmoji>> = Signal::global(Vec::new);
pub static EMOJI_SETS: GlobalSignal<Vec<EmojiSet>> = Signal::global(Vec::new);
pub static EMOJI_FETCH_TIME: GlobalSignal<Option<Timestamp>> = Signal::global(|| None);

/// Fetch user's custom emojis (kind 10030) and emoji sets (kind 30030)
pub async fn fetch_custom_emojis(pubkey: String) {
    log::info!("Fetching custom emojis for pubkey: {}", pubkey);

    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::warn!("Client not initialized, skipping emoji fetch");
            return;
        }
    };

    // Parse pubkey
    let public_key = match PublicKey::parse(&pubkey) {
        Ok(pk) => pk,
        Err(e) => {
            log::error!("Failed to parse pubkey: {}", e);
            return;
        }
    };

    // Fetch user's emoji list (kind 10030) from relays first, then query local DB
    let emoji_list_filter = Filter::new()
        .kind(Kind::from(10030))
        .author(public_key)
        .limit(1);

    // Fetch from relays to populate local database
    let fetch_result = crate::stores::nostr_client::fetch_events_aggregated(
        emoji_list_filter.clone(),
        std::time::Duration::from_secs(5)
    ).await;

    if let Err(e) = fetch_result {
        log::warn!("Failed to fetch emoji list from relays: {}, will try local DB", e);
    }

    // Now query the local database which should have the fetched events
    let emoji_list_events = match client.database().query(emoji_list_filter).await {
        Ok(events) => events,
        Err(e) => {
            log::error!("Failed to query emoji list from database: {}", e);
            return;
        }
    };

    log::info!("Found {} emoji list events", emoji_list_events.len());

    let mut custom_emojis = Vec::new();
    let mut emoji_set_refs = Vec::new();

    // Parse emoji list
    if let Some(emoji_list) = emoji_list_events.first() {
        for tag in emoji_list.tags.iter() {
            let tag_slice = tag.as_slice();
            if tag_slice.len() >= 3 && tag_slice[0] == "emoji" {
                // Direct emoji tag: ["emoji", "shortcode", "url"]
                let shortcode = tag_slice[1].to_string();
                let image_url = tag_slice[2].to_string();
                custom_emojis.push(CustomEmoji {
                    shortcode,
                    image_url,
                });
            } else if tag_slice.len() >= 2 && tag_slice[0] == "a" {
                // Reference to emoji set: ["a", "30030:pubkey:identifier"]
                emoji_set_refs.push(tag_slice[1].to_string());
            }
        }
    }

    log::info!("Found {} direct emojis and {} emoji set references",
               custom_emojis.len(), emoji_set_refs.len());

    // Parse emoji set references and fetch them
    let mut emoji_sets = Vec::new();
    for set_ref in emoji_set_refs {
        let parts: Vec<&str> = set_ref.split(':').collect();
        if parts.len() >= 3 && parts[0] == "30030" {
            let author = parts[1].to_string();
            let identifier = parts[2].to_string();

            // Parse author pubkey
            let author_pk = match PublicKey::parse(&author) {
                Ok(pk) => pk,
                Err(e) => {
                    log::warn!("Failed to parse author pubkey {}: {}", author, e);
                    continue;
                }
            };

            // Fetch this emoji set from relays first, then query local DB
            let set_filter = Filter::new()
                .kind(Kind::from(30030))
                .author(author_pk)
                .identifier(identifier.clone())
                .limit(1);

            // Fetch from relays to populate local database
            let fetch_result = crate::stores::nostr_client::fetch_events_aggregated(
                set_filter.clone(),
                std::time::Duration::from_secs(5)
            ).await;

            if let Err(e) = fetch_result {
                log::warn!("Failed to fetch emoji set {} from relays: {}, will try local DB", identifier, e);
            }

            // Now query the local database which should have the fetched events
            if let Ok(set_events) = client.database().query(set_filter).await {
                if let Some(set_event) = set_events.first() {
                    let mut set_emojis = Vec::new();
                    let mut set_name = None;

                    for tag in set_event.tags.iter() {
                        let tag_slice = tag.as_slice();
                        if tag_slice.len() >= 3 && tag_slice[0] == "emoji" {
                            let shortcode = tag_slice[1].to_string();
                            let image_url = tag_slice[2].to_string();
                            set_emojis.push(CustomEmoji {
                                shortcode,
                                image_url,
                            });
                        } else if tag_slice.len() >= 2 && tag_slice[0] == "name" {
                            set_name = Some(tag_slice[1].to_string());
                        }
                    }

                    if !set_emojis.is_empty() {
                        emoji_sets.push(EmojiSet {
                            identifier: identifier.clone(),
                            name: set_name,
                            emojis: set_emojis,
                            author: author.clone(),
                        });
                    }
                }
            }
        }
    }

    log::info!("Loaded {} emoji sets with emojis", emoji_sets.len());

    // Update global state
    *CUSTOM_EMOJIS.write() = custom_emojis;
    *EMOJI_SETS.write() = emoji_sets;
    *EMOJI_FETCH_TIME.write() = Some(Timestamp::now());
}

/// Initialize emoji fetching for the authenticated user
pub fn init_emoji_fetch() {
    let auth_state = crate::stores::auth_store::AUTH_STATE.read();
    if let Some(pubkey) = &auth_state.pubkey {
        let pubkey = pubkey.clone();
        spawn(async move {
            fetch_custom_emojis(pubkey).await;
        });
    }
}

/// Check if we should refresh emojis (older than 5 minutes)
#[allow(dead_code)]
pub fn should_refresh_emojis() -> bool {
    if let Some(last_fetch) = *EMOJI_FETCH_TIME.read() {
        let now = Timestamp::now();
        let diff = now.as_u64() - last_fetch.as_u64();
        diff > 300 // 5 minutes
    } else {
        true
    }
}
