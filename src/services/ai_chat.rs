//! Provider-aware AI chat service client.
use crate::platform::http::http_client;
use crate::stores::ai_provider_store::{AiProviderConfig, AiProviderKind, ProviderAuth};
use crate::utils::nip98 as nip98_utils;
use nostr_sdk::hashes::{sha256, Hash};
use nostr_sdk::nips::nip98;
use reqwest::{Method, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionChoice {
    pub message: AssistantMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub total_cost: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<WireModel>,
}

#[derive(Debug, Clone, Deserialize)]
struct WireModel {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    #[serde(rename = "type")]
    model_type: String,
    #[serde(default)]
    pricing: Option<Value>,
}

pub async fn get_available_models(provider: &AiProviderConfig) -> Result<Vec<ChatModel>, String> {
    let url = format!("{}/models", provider.base_url);
    let response = send_provider_request(provider, Method::GET, &url, None).await?;
    parse_models_response(provider, response).await
}

pub async fn send_chat_message(
    provider: &AiProviderConfig,
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, String> {
    let url = format!("{}/chat/completions", provider.base_url);
    let body = serde_json::to_vec(request)
        .map_err(|e| format!("Failed to serialize chat request: {}", e))?;
    let response = send_provider_request(provider, Method::POST, &url, Some(body)).await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Chat request failed ({}): {}", status, body));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse chat response: {}", e))
}

async fn parse_models_response(
    provider: &AiProviderConfig,
    response: Response,
) -> Result<Vec<ChatModel>, String> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch models ({}): {}", status, body));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read models response: {}", e))?;
    let mut parsed: ModelsResponse = serde_json::from_str(&body).map_err(|e| {
        format!(
            "Failed to parse models response: {}. Body preview: {}",
            e,
            preview_body(&body)
        )
    })?;

    parsed.data.retain(|model| is_chat_model(provider, model));

    let mut models: Vec<ChatModel> = parsed
        .data
        .into_iter()
        .map(|model| ChatModel {
            name: if model.name.trim().is_empty() {
                model.id.clone()
            } else {
                model.name
            },
            id: model.id,
            description: model.description,
            total_cost: model.pricing.as_ref().and_then(parse_total_cost),
        })
        .collect();

    models.sort_by(|a, b| match (a.total_cost, b.total_cost) {
        (Some(a_cost), Some(b_cost)) => a_cost
            .partial_cmp(&b_cost)
            .unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });

    Ok(models)
}

fn is_chat_model(provider: &AiProviderConfig, model: &WireModel) -> bool {
    match provider.provider_kind {
        AiProviderKind::Shakespeare | AiProviderKind::OpenAiCompatible => {
            model.model_type.is_empty() || model.model_type == "chat"
        }
    }
}

async fn send_provider_request(
    provider: &AiProviderConfig,
    method: Method,
    url: &str,
    body: Option<Vec<u8>>,
) -> Result<Response, String> {
    let client = http_client().map_err(|e| format!("HTTP client init failed: {}", e))?;

    match &provider.auth {
        ProviderAuth::Nip98 => {
            let signed = if let Some(body_bytes) = body.clone() {
                let payload_hash = sha256::Hash::hash(&body_bytes);
                nip98_utils::create_auth_header_with_payload(
                    url,
                    nip98::HttpMethod::POST,
                    payload_hash,
                )
                .await?
            } else {
                nip98_utils::create_auth_header(url, nip98::HttpMethod::GET).await?
            };

            let mut request = client.request(method, &signed.signed_url);
            request = request.header("Authorization", &signed.header);
            if let Some(body_bytes) = body {
                request = request
                    .header("Content-Type", "application/json")
                    .body(body_bytes);
            }
            request
                .send()
                .await
                .map_err(|e| format!("Failed to send request: {}", e))
        }
        ProviderAuth::BearerToken(api_key) => {
            let mut request = client.request(method, url);
            request = request.header("Authorization", format!("Bearer {}", api_key));
            if let Some(body_bytes) = body {
                request = request
                    .header("Content-Type", "application/json")
                    .body(body_bytes);
            }
            request
                .send()
                .await
                .map_err(|e| format!("Failed to send request: {}", e))
        }
    }
}

fn parse_total_cost(pricing: &Value) -> Option<f64> {
    let prompt = pricing.get("prompt")?.as_str()?.parse::<f64>().ok()?;
    let completion = pricing.get("completion")?.as_str()?.parse::<f64>().ok()?;
    Some(prompt + completion)
}

fn preview_body(body: &str) -> String {
    let trimmed = body.trim();
    let preview_len = trimmed.len().min(200);
    trimmed[..preview_len].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::ai_provider_store::{
        shakespeare_provider, AiProviderConfig, AiProviderKind, ProviderAuth,
    };

    #[test]
    fn parses_total_cost_when_chat_pricing_exists() {
        let value = serde_json::json!({ "prompt": "0.1", "completion": "0.2" });
        let total = parse_total_cost(&value).unwrap();
        assert!((total - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn ignores_non_chat_pricing_shapes() {
        let value = serde_json::json!({ "low_1024": "0.01" });
        assert_eq!(parse_total_cost(&value), None);
    }

    #[test]
    fn keeps_only_chat_models_for_shakespeare() {
        let provider = shakespeare_provider();
        assert!(is_chat_model(
            &provider,
            &WireModel {
                id: "claude".to_string(),
                name: "Claude".to_string(),
                description: String::new(),
                model_type: "chat".to_string(),
                pricing: None,
            }
        ));
        assert!(!is_chat_model(
            &provider,
            &WireModel {
                id: "image".to_string(),
                name: "Image".to_string(),
                description: String::new(),
                model_type: "image".to_string(),
                pricing: None,
            }
        ));
    }

    #[test]
    fn openai_compatible_provider_also_filters_non_chat_models() {
        let provider = AiProviderConfig {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            base_url: "https://example.com/v1".to_string(),
            provider_kind: AiProviderKind::OpenAiCompatible,
            auth: ProviderAuth::BearerToken("secret".to_string()),
            is_builtin: false,
        };
        assert!(is_chat_model(
            &provider,
            &WireModel {
                id: "gpt".to_string(),
                name: String::new(),
                description: String::new(),
                model_type: String::new(),
                pricing: None,
            }
        ));
    }
}
