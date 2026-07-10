use crate::platform::http::http_client;
use reqwest::{Method, Response};
use serde_json::{json, Value};
use url::Url;

pub const ROUTSTR_BASE_URL: &str = "https://api.routstr.com/v1";
const ROUTSTR_LIGHTNING_ROOT: &str = "https://api.routstr.com/lightning";

#[derive(Clone, Debug, PartialEq)]
pub struct RoutstrBalance {
    pub balance_msats: Option<u64>,
    pub total_spent_msats: Option<u64>,
    pub total_requests: Option<u64>,
    pub raw_json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoutstrLightningInvoice {
    pub invoice_id: String,
    pub bolt11: Option<String>,
    pub amount_sats: Option<u64>,
    pub expires_at: Option<u64>,
    pub payment_hash: Option<String>,
    pub raw_json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoutstrInvoiceStatus {
    pub status: Option<String>,
    pub api_key: Option<String>,
    pub amount_sats: Option<u64>,
    pub paid_at: Option<u64>,
    pub raw_json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoutstrTopupResult {
    pub msats: Option<u64>,
    pub raw_json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoutstrRefundResult {
    pub token: Option<String>,
    pub sats: Option<u64>,
    pub msats: Option<u64>,
    pub raw_json: String,
}

pub async fn get_balance(api_key: &str) -> Result<RoutstrBalance, String> {
    let url = format!("{}/balance/info", ROUTSTR_BASE_URL);
    let headers = vec![("Authorization".to_string(), format!("Bearer {}", api_key))];
    let value = send_request(Method::GET, &url, Some(headers), None).await?;
    Ok(RoutstrBalance {
        balance_msats: number_field(&value, &["balance"]).map(|n| n as u64),
        total_spent_msats: number_field(&value, &["total_spent"]).map(|n| n as u64),
        total_requests: number_field(&value, &["total_requests"]).map(|n| n as u64),
        raw_json: pretty_json(&value),
    })
}

pub async fn create_lightning_invoice(
    amount_sats: u64,
    purpose: &str,
    api_key: Option<&str>,
) -> Result<RoutstrLightningInvoice, String> {
    let url = format!("{}/invoice", ROUTSTR_LIGHTNING_ROOT);
    let mut headers = Vec::new();
    if let Some(key) = api_key {
        headers.push(("Authorization".to_string(), format!("Bearer {}", key)));
    }
    let body = json!({ "amount_sats": amount_sats, "purpose": purpose });
    let value = send_request(Method::POST, &url, Some(headers), Some(body)).await?;
    let invoice_id = string_field(&value, &["invoice_id"])
        .ok_or_else(|| "Routstr invoice response missing invoice_id (<redacted response>)".to_string())?;
    Ok(RoutstrLightningInvoice {
        invoice_id,
        bolt11: string_field(&value, &["bolt11", "payment_request"]),
        amount_sats: number_field(&value, &["amount_sats"]).map(|n| n as u64),
        expires_at: number_field(&value, &["expires_at"]).map(|n| n as u64),
        payment_hash: string_field(&value, &["payment_hash"]),
        raw_json: pretty_json(&value),
    })
}

pub async fn get_lightning_invoice_status(
    invoice_id: &str,
) -> Result<RoutstrInvoiceStatus, String> {
    let url = format!("{}/invoice/{}/status", ROUTSTR_LIGHTNING_ROOT, invoice_id);
    let value = send_request(Method::GET, &url, None, None).await?;
    Ok(RoutstrInvoiceStatus {
        status: string_field(&value, &["status"]),
        api_key: string_field(&value, &["api_key"]),
        amount_sats: number_field(&value, &["amount_sats"]).map(|n| n as u64),
        paid_at: number_field(&value, &["paid_at"]).map(|n| n as u64),
        raw_json: pretty_json(&value),
    })
}

pub async fn topup_with_cashu(
    api_key: &str,
    cashu_token: &str,
) -> Result<RoutstrTopupResult, String> {
    let url = format!("{}/balance/topup", ROUTSTR_BASE_URL);
    let headers = vec![("Authorization".to_string(), format!("Bearer {}", api_key))];
    let body = json!({ "cashu_token": cashu_token });
    let value = send_request(Method::POST, &url, Some(headers), Some(body)).await?;
    Ok(RoutstrTopupResult {
        msats: number_field(&value, &["msats"]).map(|n| n as u64),
        raw_json: pretty_json(&value),
    })
}

pub async fn create_key_from_cashu(cashu_token: &str) -> Result<String, String> {
    let base = format!("{}/balance/create", ROUTSTR_BASE_URL);
    let mut url = Url::parse(&base).map_err(|e| format!("Invalid Routstr URL: {}", e))?;
    url.query_pairs_mut()
        .append_pair("initial_balance_token", cashu_token);
    let url = url.to_string();
    let value = send_request(Method::GET, &url, None, None).await?;
    string_field(&value, &["api_key"])
        .ok_or_else(|| "Routstr balance/create response missing api_key (<redacted response>)".to_string())
}

pub async fn refund(api_key: &str) -> Result<RoutstrRefundResult, String> {
    let url = format!("{}/balance/refund", ROUTSTR_BASE_URL);
    let headers = vec![("Authorization".to_string(), format!("Bearer {}", api_key))];
    let value = send_request(Method::POST, &url, Some(headers), None).await?;
    Ok(RoutstrRefundResult {
        token: string_field(&value, &["token", "cashu_token"]),
        sats: number_field(&value, &["sats"]).map(|n| n as u64),
        msats: number_field(&value, &["msats"]).map(|n| n as u64),
        raw_json: pretty_json(&value),
    })
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

async fn send_request(
    method: Method,
    url: &str,
    headers: Option<Vec<(String, String)>>,
    body: Option<Value>,
) -> Result<Value, String> {
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
        .map_err(|e| format!("Failed to send Routstr request: {}", e))?;
    parse_response(response).await
}

async fn parse_response(response: Response) -> Result<Value, String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read Routstr response: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "Routstr request failed ({}). {}",
            status,
            redacted_response_body(body.as_str())
        ));
    }
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&body).map_err(|e| {
        format!(
            "Failed to parse Routstr response: {}. {}",
            e,
            redacted_response_body(body.as_str())
        )
    })
}

fn redacted_response_body(body: &str) -> String {
    format!("Response body redacted ({} bytes).", body.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_balance_msats() {
        let value: Value = serde_json::from_str(
            r#"{"api_key":"sk-abc","balance":8500000,"reserved":0,"total_requests":42,"total_spent":1500000}"#,
        ).unwrap();
        let balance = RoutstrBalance {
            balance_msats: number_field(&value, &["balance"]).map(|n| n as u64),
            total_spent_msats: number_field(&value, &["total_spent"]).map(|n| n as u64),
            total_requests: number_field(&value, &["total_requests"]).map(|n| n as u64),
            raw_json: pretty_json(&value),
        };
        assert_eq!(balance.balance_msats, Some(8500000));
        assert_eq!(balance.total_spent_msats, Some(1500000));
        assert_eq!(balance.total_requests, Some(42));
    }

    #[test]
    fn parses_invoice_status_with_api_key() {
        let value: Value = serde_json::from_str(
            r#"{"status":"paid","api_key":"sk-paid-key","amount_sats":1000,"paid_at":1750000500}"#,
        ).unwrap();
        let status = RoutstrInvoiceStatus {
            status: string_field(&value, &["status"]),
            api_key: string_field(&value, &["api_key"]),
            amount_sats: number_field(&value, &["amount_sats"]).map(|n| n as u64),
            paid_at: number_field(&value, &["paid_at"]).map(|n| n as u64),
            raw_json: pretty_json(&value),
        };
        assert_eq!(status.status.as_deref(), Some("paid"));
        assert_eq!(status.api_key.as_deref(), Some("sk-paid-key"));
    }

    #[test]
    fn parses_invoice_status_pending() {
        let value: Value = serde_json::from_str(
            r#"{"status":"pending","api_key":null,"amount_sats":1000,"paid_at":null}"#,
        ).unwrap();
        let status = RoutstrInvoiceStatus {
            status: string_field(&value, &["status"]),
            api_key: string_field(&value, &["api_key"]),
            amount_sats: number_field(&value, &["amount_sats"]).map(|n| n as u64),
            paid_at: number_field(&value, &["paid_at"]).map(|n| n as u64),
            raw_json: pretty_json(&value),
        };
        assert_eq!(status.status.as_deref(), Some("pending"));
        assert!(status.api_key.is_none());
    }

    #[test]
    fn parses_refund_with_string_sats() {
        let value: Value = serde_json::from_str(
            r#"{"token":"cashuAeyJ0b2tlbiI6...","sats":"1000"}"#,
        ).unwrap();
        let refund = RoutstrRefundResult {
            token: string_field(&value, &["token", "cashu_token"]),
            sats: number_field(&value, &["sats"]).map(|n| n as u64),
            msats: number_field(&value, &["msats"]).map(|n| n as u64),
            raw_json: pretty_json(&value),
        };
        assert_eq!(refund.token.as_deref(), Some("cashuAeyJ0b2tlbiI6..."));
        assert_eq!(refund.sats, Some(1000));
        assert!(refund.msats.is_none());
    }

    #[test]
    fn parses_topup_result() {
        let value: Value = serde_json::from_str(r#"{"msats":1000000}"#).unwrap();
        let topup = RoutstrTopupResult {
            msats: number_field(&value, &["msats"]).map(|n| n as u64),
            raw_json: pretty_json(&value),
        };
        assert_eq!(topup.msats, Some(1000000));
    }

    #[test]
    fn redacted_response_body_hides_content() {
        let body = r#"{"api_key":"sk-secret-key-123","detail":"some error"}"#;
        let redacted = redacted_response_body(body);
        assert!(redacted.contains("bytes"));
        assert!(!redacted.contains("sk-secret-key-123"));
    }
}
