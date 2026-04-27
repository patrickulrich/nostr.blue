use super::commands::{NoteData, ProfileData};
use super::worker::send_request;
use super::commands::NdbRequest;

pub fn sdk_filter_to_ndb(
    filter: &nostr_sdk::Filter,
) -> Result<nostrdb::Filter, String> {
    let json = serde_json::to_string(filter).map_err(|e| format!("Filter JSON: {}", e))?;
    nostrdb::Filter::from_json(&json).map_err(|e| format!("Filter conversion: {}", e))
}

pub fn sdk_filters_to_ndb_jsons(
    filters: &[nostr_sdk::Filter],
) -> Vec<String> {
    filters
        .iter()
        .filter_map(|f| serde_json::to_string(f).ok())
        .collect()
}

pub async fn query_note_keys(
    filter_jsons: Vec<String>,
    limit: i32,
) -> Result<Vec<(nostrdb::NoteKey, u64)>, String> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    send_request(NdbRequest::QueryNoteKeys {
        filter_jsons,
        limit,
        reply: reply_tx,
    })?;
    reply_rx.await.map_err(|e| e.to_string())?
}

pub async fn get_note_data(key: nostrdb::NoteKey) -> Result<NoteData, String> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    send_request(NdbRequest::GetNoteData {
        key,
        reply: reply_tx,
    })?;
    reply_rx.await.map_err(|e| e.to_string())?
}

pub async fn get_note_data_by_id(id: [u8; 32]) -> Result<Option<NoteData>, String> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    send_request(NdbRequest::GetNoteDataById { id, reply: reply_tx })?;
    reply_rx.await.map_err(|e| e.to_string())?
}

pub async fn get_profile(pubkey: [u8; 32]) -> Result<Option<ProfileData>, String> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    send_request(NdbRequest::GetProfile { pubkey, reply: reply_tx })?;
    reply_rx.await.map_err(|e| e.to_string())?
}

pub fn note_data_to_event(data: &NoteData) -> Result<nostr::Event, String> {
    use nostr::event::{Event, EventId, Kind, Tags};
    use nostr::key::PublicKey;
    use nostr::secp256k1::schnorr::Signature;
    use nostr::types::Timestamp;

    let id = EventId::from_byte_array(data.id);
    let pubkey = PublicKey::from_byte_array(data.pubkey);
    let created_at = Timestamp::from(data.created_at);
    let kind = Kind::from_u16(data.kind);
    let tags = Tags::parse(data.tags.clone()).map_err(|e| format!("Tag parse: {}", e))?;
    let content = data.content.clone();
    let sig =
        Signature::from_slice(&data.sig).map_err(|e| format!("Signature: {}", e))?;
    Ok(Event::new(id, pubkey, created_at, kind, tags, content, sig))
}
