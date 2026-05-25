use crate::stores::nostr_client::NOSTR_CLIENT;
use crate::utils::bolt11::parse_bolt11_amount;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

const CHUNK_SIZE: usize = 100;
const ENGAGEMENT_TIMEOUT_SECS: u64 = 5;

#[derive(Clone, Debug, Default)]
pub struct EngagementData {
    pub reaction_count: usize,
    pub repost_count: usize,
    pub zap_total_msat: u64,
}

pub async fn fetch_engagement(
    event_ids: &[EventId],
) -> std::result::Result<HashMap<EventId, EngagementData>, String> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => return Err("Nostr client not initialized".to_string()),
    };

    let mut engagement: HashMap<EventId, EngagementData> = HashMap::new();
    for id in event_ids {
        engagement.insert(*id, EngagementData::default());
    }

    let chunks: Vec<Vec<EventId>> = event_ids
        .chunks(CHUNK_SIZE)
        .map(|c| c.to_vec())
        .collect();

    for chunk in chunks {
        let id_hexes: Vec<String> = chunk.iter().map(|id| id.to_hex()).collect();
        let filter = Filter::new()
            .kinds([Kind::Reaction, Kind::Repost, Kind::from(9735)])
            .custom_tags(
                SingleLetterTag::lowercase(Alphabet::E),
                id_hexes.iter().map(|s| s.as_str()),
            )
            .limit(500);

        match client
            .fetch_events(filter, Duration::from_secs(ENGAGEMENT_TIMEOUT_SECS))
            .await
        {
            Ok(events) => {
                for event in events {
                    let target_id = extract_e_tag_event_id(&event);
                    if let Some(target_id) = target_id {
                        if let Some(data) = engagement.get_mut(&target_id) {
                            match event.kind.as_u16() {
                                7 => data.reaction_count += 1,
                                6 => data.repost_count += 1,
                                9735 => {
                                    data.zap_total_msat += extract_zap_amount(&event);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Err(e) => {
                log::debug!("Failed to fetch engagement for chunk: {}", e);
            }
        }
    }

    Ok(engagement)
}

fn extract_e_tag_event_id(event: &Event) -> Option<EventId> {
    for tag in event.tags.iter() {
        if tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)) {
            if let Some(content) = tag.content() {
                if let Ok(id) = EventId::from_hex(content) {
                    return Some(id);
                }
            }
        }
        if tag.kind() == TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::E)) {
            if let Some(content) = tag.content() {
                if let Ok(id) = EventId::from_hex(content) {
                    return Some(id);
                }
            }
        }
    }
    None
}

fn extract_zap_amount(event: &Event) -> u64 {
    for tag in event.tags.iter() {
        if tag.kind() == TagKind::Custom("bolt11".into()) {
            if let Some(invoice) = tag.content() {
                return parse_bolt11_amount(invoice)
                    .and_then(|sats| sats.checked_mul(1000))
                    .unwrap_or(0);
            }
        }
        if tag.kind() == TagKind::Custom("amount".into()) {
            if let Some(amount_str) = tag.content() {
                if let Ok(msat) = amount_str.parse::<u64>() {
                    return msat;
                }
            }
        }
    }
    0
}
