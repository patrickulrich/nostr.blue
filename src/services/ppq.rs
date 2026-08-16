use crate::platform::http::http_client;
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};
use std::collections::HashSet;
use url::Url;

pub const PPQ_API_ROOT: &str = "https://api.ppq.ai";
pub const PPQ_CHAT_BASE_URL: &str = "https://api.ppq.ai/v1";

fn build_ppq_url(segments: &[&str]) -> Result<String, String> {
    let mut url = Url::parse(PPQ_API_ROOT).map_err(|e| format!("Invalid PPQ API root: {}", e))?;
    {
        let mut path_segments = url
            .path_segments_mut()
            .map_err(|_| "PPQ API root cannot be a base URL".to_string())?;
        for segment in segments {
            path_segments.push(segment);
        }
    }
    Ok(url.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PpqAccount {
    pub credit_id: String,
    pub api_key: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PpqBalance {
    pub amount: Option<f64>,
    pub currency: String,
    pub raw_json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PpqTopupInvoice {
    pub invoice_id: String,
    pub status: Option<String>,
    pub payment_request: Option<String>,
    pub address: Option<String>,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub raw_json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PpqNwcAutoTopup {
    pub nwc_url: Option<String>,
    pub threshold_usd: Option<f64>,
    pub topup_amount_usd: Option<f64>,
    pub raw_json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PpqApiKey {
    pub id: String,
    pub name: String,
    pub api_key: Option<String>,
    pub usage_limit_usd: Option<f64>,
    pub current_period_usage_usd: Option<f64>,
    pub total_usage_all_time_usd: Option<f64>,
    pub reset_period: Option<String>,
    pub reset_at: Option<String>,
    pub expire_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PpqApiKeyInput {
    pub name: String,
    pub usage_limit_usd: Option<f64>,
    pub reset_period: Option<String>,
    pub expire_at: Option<String>,
}

pub async fn create_account() -> Result<PpqAccount, String> {
    let value = send_request(
        Method::POST,
        &format!("{PPQ_API_ROOT}/accounts/create"),
        None,
        None,
    )
    .await?;
    let data = data_or_root(&value);
    let credit_id = string_field(data, &["credit_id", "creditId"])
        .ok_or_else(|| "PPQ account response missing field: credit_id".to_string())?;
    let api_key = string_field(data, &["api_key", "apiKey"])
        .ok_or_else(|| "PPQ account response missing field: api_key".to_string())?;
    Ok(PpqAccount { credit_id, api_key })
}

pub async fn get_balance(credit_id: &str) -> Result<PpqBalance, String> {
    let value = send_request(
        Method::POST,
        &format!("{PPQ_API_ROOT}/credits/balance"),
        None,
        Some(json!({ "credit_id": credit_id })),
    )
    .await?;
    let data = data_or_root(&value);
    let amount = number_field(
        data,
        &[
            "balance",
            "balance_usd",
            "credit_balance",
            "creditBalance",
            "usd_balance",
            "usdBalance",
        ],
    );
    let currency = string_field(data, &["currency"]).unwrap_or_else(|| "USD".to_string());
    Ok(PpqBalance {
        amount,
        currency,
        raw_json: pretty_json(&value),
    })
}

pub const IMPORT_KEY_NAME: &str = "nostr.blue";

#[derive(Clone, Debug, PartialEq)]
pub struct PpqImportedAccount {
    pub credit_id: String,
    pub api_key: Option<String>,
    pub key_id: Option<String>,
}

pub fn normalize_credit_id(input: &str) -> Result<String, String> {
    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        return Err("Credit ID is required".to_string());
    }
    if !is_uuid_format(&trimmed) {
        return Err(
            "Credit ID must be a UUID, e.g. 4af59b9d-f6ec-4531-82f7-ce776d49e207".to_string(),
        );
    }
    Ok(trimmed)
}

fn is_uuid_format(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn pick_import_key(keys: &[PpqApiKey]) -> Option<String> {
    keys.iter()
        .find(|key| key.deleted_at.is_none() && key.name == IMPORT_KEY_NAME)
        .map(|key| key.id.clone())
}

fn next_import_key_name(keys: &[PpqApiKey]) -> String {
    let taken: HashSet<&str> = keys.iter().map(|key| key.name.as_str()).collect();
    if !taken.contains(IMPORT_KEY_NAME) {
        return IMPORT_KEY_NAME.to_string();
    }
    for attempt in 2..100 {
        let candidate = format!("{IMPORT_KEY_NAME}-{attempt}");
        if !taken.contains(candidate.as_str()) {
            return candidate;
        }
    }
    format!("{IMPORT_KEY_NAME}-{}", keys.len() + 100)
}

/// Validate a Credit ID (and, when provided, the pasted API key) via the
/// GET /keys probe. Attaching the key's Bearer header makes a typo'd key
/// fail loudly at import time instead of surfacing as a confusing 401 on
/// the first chat call.
pub async fn validate_credit_id(
    credit_id: &str,
    api_key: Option<&str>,
) -> Result<String, String> {
    let credit_id = normalize_credit_id(credit_id)?;
    let mut headers = vec![("x-credit-id".to_string(), credit_id.clone())];
    if let Some(api_key) = api_key {
        headers.push(("Authorization".to_string(), format!("Bearer {api_key}")));
    }
    let response = send_request_raw(
        Method::GET,
        &format!("{PPQ_API_ROOT}/keys"),
        Some(headers),
        None,
    )
    .await?;
    let status = response.status;
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(if api_key.is_some() {
            "PPQ rejected this API key".to_string()
        } else {
            "PPQ does not recognize this Credit ID".to_string()
        });
    }
    ensure_credit_id_accepted(status)?;
    Ok(credit_id)
}

fn ensure_credit_id_accepted(status: StatusCode) -> Result<(), String> {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_FOUND {
        return Err("PPQ does not recognize this Credit ID".to_string());
    }
    if !status.is_success() {
        return Err(format!("PPQ request failed ({}).", status));
    }
    Ok(())
}

pub async fn import_credit_id(credit_id: &str) -> Result<PpqImportedAccount, String> {
    // Listing keys doubles as validation: the credits/balance endpoint cannot
    // be used because it returns 200 `{"balance":0}` even for unknown ids.
    // Revoked keys are included so the created key name avoids collisions
    // with names still held by deleted keys.
    let credit_id = normalize_credit_id(credit_id)?;
    let mut list_url = Url::parse(&build_ppq_url(&["keys"])?)
        .map_err(|e| format!("Invalid PPQ API url: {}", e))?;
    list_url
        .query_pairs_mut()
        .append_pair("include_disabled", "true");
    let response = send_request_raw(
        Method::GET,
        list_url.as_ref(),
        Some(vec![("x-credit-id".to_string(), credit_id.clone())]),
        None,
    )
    .await?;
    ensure_credit_id_accepted(response.status)?;
    let keys: Vec<PpqApiKey> = array_from_value(&response.value)
        .iter()
        .map(|item| parse_api_key(item))
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(key_id) = pick_import_key(&keys) {
        if let Ok(existing) = get_api_key(&credit_id, &key_id, true).await {
            if let Some(full_key) = existing.api_key {
                return Ok(PpqImportedAccount {
                    credit_id,
                    api_key: Some(full_key),
                    key_id: Some(existing.id),
                });
            }
        }
    }

    let input = PpqApiKeyInput {
        name: next_import_key_name(&keys),
        usage_limit_usd: None,
        reset_period: None,
        expire_at: None,
    };
    let created = create_api_key(&credit_id, &input).await?;
    let key_id = created.api_key.as_ref().map(|_| created.id.clone());
    Ok(PpqImportedAccount {
        credit_id,
        api_key: created.api_key,
        key_id,
    })
}

pub async fn create_topup_invoice(
    api_key: &str,
    method: &str,
    amount: f64,
    currency: &str,
) -> Result<PpqTopupInvoice, String> {
    let url = build_ppq_url(&["topup", "create", method])?;
    let value = send_request(
        Method::POST,
        &url,
        Some(vec![(
            "Authorization".to_string(),
            format!("Bearer {api_key}"),
        )]),
        Some(json!({
            "amount": amount,
            "currency": currency,
        })),
    )
    .await?;
    parse_topup_invoice(&value)
}

pub async fn get_topup_status(api_key: &str, invoice_id: &str) -> Result<PpqTopupInvoice, String> {
    let url = build_ppq_url(&["topup", "status", invoice_id])?;
    let value = send_request(
        Method::GET,
        &url,
        Some(vec![(
            "Authorization".to_string(),
            format!("Bearer {api_key}"),
        )]),
        None,
    )
    .await?;
    parse_topup_invoice(&value)
}

pub async fn get_nwc_auto_topup(credit_id: &str) -> Result<Option<PpqNwcAutoTopup>, String> {
    let value = send_request(
        Method::GET,
        &format!("{PPQ_API_ROOT}/nwc-auto-topup"),
        Some(vec![("x-credit-id".to_string(), credit_id.to_string())]),
        None,
    )
    .await?;
    let data = data_or_root(&value);
    if data.is_null() {
        return Ok(None);
    }
    Ok(Some(PpqNwcAutoTopup {
        nwc_url: string_field(data, &["nwc_url", "nwcUrl"]),
        threshold_usd: number_field(data, &["threshold_usd", "thresholdUsd"]),
        topup_amount_usd: number_field(data, &["topup_amount_usd", "topupAmountUsd"]),
        raw_json: pretty_json(&value),
    }))
}

pub async fn connect_nwc_auto_topup(
    credit_id: &str,
    nwc_url: &str,
    threshold_usd: Option<f64>,
    topup_amount_usd: Option<f64>,
) -> Result<PpqNwcAutoTopup, String> {
    let value = send_request(
        Method::POST,
        &format!("{PPQ_API_ROOT}/nwc-auto-topup/connect"),
        Some(vec![("x-credit-id".to_string(), credit_id.to_string())]),
        Some(json!({
            "nwc_url": nwc_url,
            "threshold_usd": threshold_usd,
            "topup_amount_usd": topup_amount_usd,
        })),
    )
    .await?;
    let data = data_or_root(&value);
    Ok(PpqNwcAutoTopup {
        nwc_url: string_field(data, &["nwc_url", "nwcUrl"]),
        threshold_usd: number_field(data, &["threshold_usd", "thresholdUsd"]),
        topup_amount_usd: number_field(data, &["topup_amount_usd", "topupAmountUsd"]),
        raw_json: pretty_json(&value),
    })
}

pub async fn disconnect_nwc_auto_topup(credit_id: &str) -> Result<(), String> {
    let _ = send_request(
        Method::DELETE,
        &format!("{PPQ_API_ROOT}/nwc-auto-topup/connection"),
        Some(vec![("x-credit-id".to_string(), credit_id.to_string())]),
        None,
    )
    .await?;
    Ok(())
}

pub async fn list_api_keys(credit_id: &str) -> Result<Vec<PpqApiKey>, String> {
    let value = send_request(
        Method::GET,
        &format!("{PPQ_API_ROOT}/keys"),
        Some(vec![("x-credit-id".to_string(), credit_id.to_string())]),
        None,
    )
    .await?;
    let keys = array_from_value(&value);
    keys.iter().map(|value| parse_api_key(value)).collect()
}

pub async fn get_api_key(
    credit_id: &str,
    key_id: &str,
    show_key: bool,
) -> Result<PpqApiKey, String> {
    let mut url = Url::parse(&build_ppq_url(&["keys", key_id])?)
        .map_err(|e| format!("Invalid PPQ API url: {}", e))?;
    if show_key {
        url.query_pairs_mut().append_pair("show_key", "true");
    }
    let value = send_request(
        Method::GET,
        url.as_ref(),
        Some(vec![("x-credit-id".to_string(), credit_id.to_string())]),
        None,
    )
    .await?;
    parse_api_key(data_or_root(&value))
}

pub async fn create_api_key(credit_id: &str, input: &PpqApiKeyInput) -> Result<PpqApiKey, String> {
    let value = send_request(
        Method::POST,
        &format!("{PPQ_API_ROOT}/keys"),
        Some(vec![("x-credit-id".to_string(), credit_id.to_string())]),
        Some(api_key_payload(input)),
    )
    .await?;
    parse_api_key(data_or_root(&value))
}

pub async fn update_api_key(
    credit_id: &str,
    key_id: &str,
    input: &PpqApiKeyInput,
) -> Result<PpqApiKey, String> {
    let url = build_ppq_url(&["keys", key_id])?;
    let value = send_request(
        Method::PATCH,
        &url,
        Some(vec![("x-credit-id".to_string(), credit_id.to_string())]),
        Some(api_key_payload(input)),
    )
    .await?;
    parse_api_key(data_or_root(&value))
}

pub async fn delete_api_key(credit_id: &str, key_id: &str) -> Result<(), String> {
    let url = build_ppq_url(&["keys", key_id])?;
    let _ = send_request(
        Method::DELETE,
        &url,
        Some(vec![("x-credit-id".to_string(), credit_id.to_string())]),
        None,
    )
    .await?;
    Ok(())
}

fn api_key_payload(input: &PpqApiKeyInput) -> Value {
    json!({
        "name": input.name,
        "usage_limit_usd": input.usage_limit_usd,
        "reset_period": input.reset_period,
        "expire_at": input.expire_at,
    })
}

fn parse_topup_invoice(value: &Value) -> Result<PpqTopupInvoice, String> {
    let data = data_or_root(value);
    let invoice_id = string_field(data, &["invoice_id", "id", "invoiceId"]).ok_or_else(|| {
        "PPQ topup invoice response missing required id (<redacted response>)".to_string()
    })?;
    Ok(PpqTopupInvoice {
        invoice_id,
        status: string_field(data, &["status"]),
        payment_request: string_field(
            data,
            &[
                "payment_request",
                "paymentRequest",
                "lightning_invoice",
                "invoice",
            ],
        ),
        address: string_field(data, &["address", "payment_address", "paymentAddress"]),
        amount: number_field(data, &["amount"]),
        currency: string_field(data, &["currency"]),
        raw_json: pretty_json(value),
    })
}

fn parse_api_key(value: &Value) -> Result<PpqApiKey, String> {
    let id = string_field(value, &["_id", "id"]).ok_or_else(|| {
        "PPQ API key response missing required id (<redacted response>)".to_string()
    })?;
    Ok(PpqApiKey {
        id,
        name: string_field(value, &["name"]).unwrap_or_default(),
        api_key: string_field(value, &["api_key", "apiKey"]),
        usage_limit_usd: number_field(value, &["usage_limit_usd", "usageLimitUsd"]),
        current_period_usage_usd: number_field(
            value,
            &["current_period_usage_usd", "currentPeriodUsageUsd"],
        ),
        total_usage_all_time_usd: number_field(
            value,
            &["total_usage_all_time_usd", "totalUsageAllTimeUsd"],
        ),
        reset_period: string_field(value, &["reset_period", "resetPeriod"]),
        reset_at: string_field(value, &["reset_at", "resetAt"]),
        expire_at: string_field(value, &["expire_at", "expireAt"]),
        created_at: string_field(value, &["created_at", "createdAt"]),
        updated_at: string_field(value, &["updated_at", "updatedAt"]),
        deleted_at: string_field(value, &["deleted_at", "deletedAt"]),
    })
}

fn data_or_root(value: &Value) -> &Value {
    value.get("data").unwrap_or(value)
}

fn array_from_value(value: &Value) -> Vec<&Value> {
    if let Some(items) = data_or_root(value).as_array() {
        return items.iter().collect();
    }
    if let Some(items) = value.as_array() {
        return items.iter().collect();
    }
    Vec::new()
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(found) = value.get(*key) {
            if let Some(as_str) = found.as_str() {
                let trimmed = as_str.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

fn number_field(value: &Value, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(found) = value.get(*key) {
            if let Some(number) = found.as_f64() {
                return Some(number);
            }
            if let Some(as_str) = found.as_str() {
                if let Ok(number) = as_str.parse::<f64>() {
                    return Some(number);
                }
            }
        }
    }
    None
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

struct PpqRawResponse {
    status: StatusCode,
    value: Value,
    body_len: usize,
}

async fn send_request_raw(
    method: Method,
    url: &str,
    headers: Option<Vec<(String, String)>>,
    body: Option<Value>,
) -> Result<PpqRawResponse, String> {
    let client = http_client().map_err(|e| format!("HTTP client init failed: {}", e))?;
    let mut request = client.request(method, url);
    if let Some(headers) = headers {
        for (name, value) in headers {
            request = request.header(name, value);
        }
    }
    if let Some(body) = body {
        request = request
            .header("Content-Type", "application/json")
            .json(&body);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to send PPQ request: {}", e))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read PPQ response: {}", e))?;
    let body_len = body.len();
    let value = if body.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&body).map_err(|e| {
            format!(
                "Failed to parse PPQ response: {}. {}",
                e,
                redacted_response_body(&body)
            )
        })?
    };
    Ok(PpqRawResponse {
        status,
        value,
        body_len,
    })
}

async fn send_request(
    method: Method,
    url: &str,
    headers: Option<Vec<(String, String)>>,
    body: Option<Value>,
) -> Result<Value, String> {
    let raw = send_request_raw(method, url, headers, body).await?;
    if !raw.status.is_success() {
        return Err(format!(
            "PPQ request failed ({}). {}",
            raw.status,
            redacted_response_body_len(raw.body_len)
        ));
    }
    Ok(raw.value)
}

fn redacted_response_body(body: &str) -> String {
    redacted_response_body_len(body.len())
}

fn redacted_response_body_len(body_len: usize) -> String {
    format!("Response body redacted ({} bytes).", body_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_account_data_from_wrapped_response() {
        let value = json!({
            "status": "success",
            "data": {
                "credit_id": "credit-123",
                "api_key": "sk-test"
            }
        });
        let data = data_or_root(&value);
        assert_eq!(
            PpqAccount {
                credit_id: string_field(data, &["credit_id"]).unwrap(),
                api_key: string_field(data, &["api_key"]).unwrap(),
            },
            PpqAccount {
                credit_id: "credit-123".to_string(),
                api_key: "sk-test".to_string(),
            }
        );
    }

    #[test]
    fn parses_api_key_usage_fields() {
        let parsed = parse_api_key(&json!({
            "_id": "abc",
            "name": "default",
            "usage_limit_usd": "10.5",
            "current_period_usage_usd": 1.25,
            "reset_period": "monthly"
        }))
        .unwrap();
        assert_eq!(parsed.id, "abc");
        assert_eq!(parsed.usage_limit_usd, Some(10.5));
        assert_eq!(parsed.current_period_usage_usd, Some(1.25));
        assert_eq!(parsed.reset_period.as_deref(), Some("monthly"));
    }

    #[test]
    fn redacted_response_body_hides_content() {
        let body = "{\"secret\":\"sk-live-123\"}";
        let summary = redacted_response_body(body);
        assert_eq!(
            summary,
            format!("Response body redacted ({} bytes).", body.len())
        );
        assert!(!summary.contains("sk-live-123"));
    }

    #[test]
    fn normalize_credit_id_accepts_uuid_with_whitespace_and_case() {
        assert_eq!(
            normalize_credit_id("  4AF59B9D-F6EC-4531-82F7-CE776D49E207 \n").unwrap(),
            "4af59b9d-f6ec-4531-82f7-ce776d49e207"
        );
    }

    #[test]
    fn normalize_credit_id_rejects_invalid_input() {
        assert!(normalize_credit_id("").is_err());
        assert!(normalize_credit_id("   ").is_err());
        assert!(normalize_credit_id("not-a-real-id").is_err());
        assert!(normalize_credit_id("4af59b9df6ec453182f7ce776d49e207").is_err());
        assert!(normalize_credit_id("4af59b9d-f6ec-4531-82f7-ce776d49e2070").is_err());
        assert!(normalize_credit_id("zap59b9d-f6ec-4531-82f7-ce776d49e207").is_err());
    }

    fn key(id: &str, name: &str, deleted: bool) -> PpqApiKey {
        PpqApiKey {
            id: id.to_string(),
            name: name.to_string(),
            api_key: None,
            usage_limit_usd: None,
            current_period_usage_usd: None,
            total_usage_all_time_usd: None,
            reset_period: None,
            reset_at: None,
            expire_at: None,
            created_at: None,
            updated_at: None,
            deleted_at: deleted.then(|| "2026-01-01T00:00:00Z".to_string()),
        }
    }

    #[test]
    fn pick_import_key_reuses_only_active_nostr_blue_key() {
        let keys = vec![
            key("1", "other", false),
            key("2", IMPORT_KEY_NAME, true),
            key("3", IMPORT_KEY_NAME, false),
        ];
        assert_eq!(pick_import_key(&keys), Some("3".to_string()));
        assert_eq!(pick_import_key(&[]), None);
        assert_eq!(
            pick_import_key(&[key("2", IMPORT_KEY_NAME, true)]),
            None
        );
    }

    #[test]
    fn next_import_key_name_avoids_taken_names() {
        assert_eq!(next_import_key_name(&[]), IMPORT_KEY_NAME);
        assert_eq!(
            next_import_key_name(&[key("1", "other", false)]),
            IMPORT_KEY_NAME
        );
        assert_eq!(
            next_import_key_name(&[key("1", IMPORT_KEY_NAME, false)]),
            "nostr.blue-2"
        );
        // A revoked key still holds its name server-side.
        assert_eq!(
            next_import_key_name(&[
                key("1", IMPORT_KEY_NAME, true),
                key("2", "nostr.blue-2", false),
            ]),
            "nostr.blue-3"
        );
    }
}
