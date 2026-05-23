use crate::stores::nostr_client::NOSTR_CLIENT;
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
                return decode_bolt11_amount(invoice);
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

fn decode_bolt11_amount(invoice: &str) -> u64 {
    if let Some(pos) = invoice.find('1') {
        let hrp_end = pos + 1;
        if hrp_end >= invoice.len() {
            return 0;
        }
        let amount_part = &invoice[hrp_end..];
        let mut num_str = String::new();
        for c in amount_part.chars() {
            if c.is_ascii_digit() {
                num_str.push(c);
            } else {
                break;
            }
        }
        if num_str.is_empty() {
            return 0;
        }
        if let Ok(mut amount) = num_str.parse::<u64>() {
            let multiplier = amount_part.chars().nth(num_str.len()).unwrap_or('p');
            amount = match multiplier {
                'm' => amount * 100_000,
                'u' => amount * 100,
                'n' => amount / 10,
                'p' => amount / 10_000,
                _ => return 0,
            };
            return amount * 1000;
        }
    }
    0
}
