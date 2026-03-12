//! Shakespeare AI service client.
//!
//! Provides authenticated access to model discovery and chat completions.
use crate::platform::http::http_client;
use crate::utils::nip98 as nip98_utils;
use nostr_sdk::hashes::{sha256, Hash};
use nostr_sdk::nips::nip98;
use serde::{Deserialize, Serialize};

const SHAKESPEARE_API_BASE: &str = "https://ai.shakespeare.diy/v1";

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

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsResponse {
    pub data: Vec<Model>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Model {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub pricing: ModelPricing,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ModelPricing {
    pub prompt: String,
    pub completion: String,
}

pub async fn get_available_models() -> Result<Vec<Model>, String> {
    let url = format!("{}/models", SHAKESPEARE_API_BASE);
    let auth_result = nip98_utils::create_auth_header(&url, nip98::HttpMethod::GET).await?;
    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(&auth_result.signed_url)
        .header("Authorization", &auth_result.header)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch models ({}): {}", status, body));
    }

    let mut parsed: ModelsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse models response: {}", e))?;
    parsed.data.sort_by(|a, b| {
        let a_total = parse_price(&a.pricing);
        let b_total = parse_price(&b.pricing);
        a_total
            .partial_cmp(&b_total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(parsed.data)
}

pub async fn send_chat_message(
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, String> {
    let url = format!("{}/chat/completions", SHAKESPEARE_API_BASE);
    let body = serde_json::to_vec(request)
        .map_err(|e| format!("Failed to serialize chat request: {}", e))?;
    let payload_hash = sha256::Hash::hash(&body);
    let auth_result =
        nip98_utils::create_auth_header_with_payload(&url, nip98::HttpMethod::POST, payload_hash)
            .await?;
    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .post(&auth_result.signed_url)
        .header("Authorization", &auth_result.header)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Failed to send chat message: {}", e))?;

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

fn parse_price(pricing: &ModelPricing) -> f64 {
    pricing.prompt.parse::<f64>().unwrap_or(f64::MAX)
        + pricing.completion.parse::<f64>().unwrap_or(f64::MAX)
}
