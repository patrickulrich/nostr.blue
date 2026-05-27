use serde_json::{json, Value};
use std::str::FromStr;

const BLIP10_TLV_TYPE: u64 = 7629169;

pub struct BoostMetadata {
    pub action: &'static str,
    pub value_msat_total: u64,
    pub value_msat: u64,
    pub app_name: Option<String>,
    pub app_version: Option<String>,
    pub sender_name: Option<String>,
    pub message: Option<String>,
    pub podcast: Option<String>,
    pub episode: Option<String>,
    pub guid: Option<String>,
    pub episode_guid: Option<String>,
    pub url: Option<String>,
    pub feed_id: Option<u64>,
    pub ts: Option<u64>,
    pub time: Option<String>,
    pub recipient_name: Option<String>,
}

pub fn build_blip10_json(meta: &BoostMetadata) -> String {
    let mut obj = serde_json::Map::new();
    obj.insert("action".to_string(), json!(meta.action));
    if meta.value_msat_total > 0 {
        obj.insert("value_msat_total".to_string(), json!(meta.value_msat_total));
    }
    if meta.value_msat > 0 {
        obj.insert("value_msat".to_string(), json!(meta.value_msat));
    }
    if let Some(ref v) = meta.app_name {
        obj.insert("app_name".to_string(), json!(v));
    }
    if let Some(ref v) = meta.app_version {
        obj.insert("app_version".to_string(), json!(v));
    }
    if let Some(ref v) = meta.sender_name {
        obj.insert("sender_name".to_string(), json!(v));
    }
    if let Some(ref v) = meta.message {
        obj.insert("message".to_string(), json!(v));
    }
    if let Some(ref v) = meta.podcast {
        obj.insert("podcast".to_string(), json!(v));
    }
    if let Some(ref v) = meta.episode {
        obj.insert("episode".to_string(), json!(v));
    }
    if let Some(ref v) = meta.guid {
        obj.insert("guid".to_string(), json!(v));
    }
    if let Some(ref v) = meta.episode_guid {
        obj.insert("episode_guid".to_string(), json!(v));
    }
    if let Some(ref v) = meta.url {
        obj.insert("url".to_string(), json!(v));
    }
    if let Some(v) = meta.feed_id {
        obj.insert("feedID".to_string(), json!(v));
    }
    if let Some(v) = meta.ts {
        obj.insert("ts".to_string(), json!(v));
    }
    if let Some(ref v) = meta.time {
        obj.insert("time".to_string(), json!(v));
    }
    if let Some(ref v) = meta.recipient_name {
        obj.insert("name".to_string(), json!(v));
    }
    Value::Object(obj).to_string()
}

pub struct TlvRecord {
    pub tlv_type: u64,
    pub value: String,
}

pub fn build_custom_records(
    blip_json: &str,
    custom_key: Option<&str>,
    custom_value: Option<&str>,
) -> Vec<TlvRecord> {
    let mut records = Vec::new();
    records.push(TlvRecord {
        tlv_type: BLIP10_TLV_TYPE,
        value: blip_json.to_string(),
    });
    if let (Some(ck), Some(cv)) = (custom_key, custom_value) {
        if let Ok(tlv_type) = u64::from_str(ck) {
            records.push(TlvRecord {
                tlv_type,
                value: cv.to_string(),
            });
        }
    }
    records
}
