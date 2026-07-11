//! Provider-aware AI chat service client.
use crate::platform::http::http_client;
use crate::stores::ai_provider_store::{AiProviderConfig, AiProviderKind, ProviderAuth};
use reqwest::{Method, Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: ChatMessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ChatMessageContent {
    Text(String),
    Parts(Vec<ChatMessagePart>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatMessagePart {
    Text { text: String },
    ImageUrl { image_url: ChatImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatImageUrl {
    pub url: String,
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
    pub content: Option<AssistantContent>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum AssistantContent {
    Text(String),
    Parts(Vec<AssistantContentPart>),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantContentPart {
    Text { text: String },
    ImageUrl { image_url: ChatImageUrl },
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
struct AnthropicMessagesRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicToolDef>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase", tag = "role")]
enum AnthropicMessage {
    User {
        content: serde_json::Value,
    },
    Assistant {
        content: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize)]
struct AnthropicToolDef {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicMessagesResponse {
    content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModel>,
    #[serde(default)]
    #[allow(dead_code)]
    has_more: bool,
    #[serde(default)]
    #[allow(dead_code)]
    last_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicModel {
    id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    capabilities: Option<AnthropicModelCapabilities>,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicModelCapabilities {
    #[serde(default)]
    image_input: Option<AnthropicCapabilitySupport>,
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicCapabilitySupport {
    #[serde(default)]
    supported: bool,
}

fn convert_to_anthropic_request(request: &ChatCompletionRequest) -> AnthropicMessagesRequest {
    let mut system_prompt = None;
    let mut messages = Vec::new();

    for msg in &request.messages {
        match msg.role {
            ChatRole::System => {
                system_prompt = Some(extract_text_content(&msg.content));
            }
            ChatRole::User => {
                let content = convert_user_content(&msg.content, &msg.tool_call_id);
                messages.push(AnthropicMessage::User { content });
            }
            ChatRole::Assistant => {
                let content = convert_assistant_content(
                    &msg.content,
                    &msg.tool_calls,
                );
                messages.push(AnthropicMessage::Assistant { content });
            }
            ChatRole::Tool => {
                if let Some(tool_call_id) = &msg.tool_call_id {
                    let tool_result_content = extract_text_content(&msg.content);
                    let content = serde_json::json!([{
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": tool_result_content
                    }]);
                    messages.push(AnthropicMessage::User { content });
                }
            }
        }
    }

    let anthropic_tools = request.tools.as_ref().map(|tools| {
        tools
            .iter()
            .map(|t| AnthropicToolDef {
                name: t.function.name.clone(),
                description: t.function.description.clone(),
                input_schema: t.function.parameters.clone(),
            })
            .collect::<Vec<_>>()
    });

    AnthropicMessagesRequest {
        model: request.model.clone(),
        messages,
        max_tokens: 8192,
        system: system_prompt,
        tools: anthropic_tools,
    }
}

fn convert_user_content(
    content: &ChatMessageContent,
    tool_call_id: &Option<String>,
) -> serde_json::Value {
    if let ChatMessageContent::Parts(parts) = content {
        let blocks: Vec<serde_json::Value> = parts
            .iter()
            .map(|part| match part {
                ChatMessagePart::Text { text } => {
                    serde_json::json!({"type": "text", "text": text})
                }
                ChatMessagePart::ImageUrl { image_url } => {
                    serde_json::json!({
                        "type": "image",
                        "source": {"type": "url", "url": image_url.url}
                    })
                }
            })
            .collect();
        if blocks.len() == 1 {
            return serde_json::Value::Array(blocks);
        }
        serde_json::Value::Array(blocks)
    } else {
        let text = extract_text_content(content);
        if let Some(_id) = tool_call_id {
            serde_json::json!([{"type": "text", "text": text}])
        } else {
            serde_json::json!(text)
        }
    }
}

fn convert_assistant_content(
    content: &ChatMessageContent,
    tool_calls: &Option<Vec<ToolCall>>,
) -> serde_json::Value {
    let mut blocks = Vec::new();

    let text = extract_text_content(content);
    if !text.is_empty() {
        blocks.push(serde_json::json!({"type": "text", "text": text}));
    }

    if let Some(calls) = tool_calls {
        for tc in calls {
            let input: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));
            blocks.push(serde_json::json!({
                "type": "tool_use",
                "id": tc.id,
                "name": tc.function.name,
                "input": input
            }));
        }
    }

    if blocks.is_empty() {
        serde_json::json!("")
    } else if blocks.len() == 1 && tool_calls.is_none() {
        serde_json::json!(text)
    } else {
        serde_json::Value::Array(blocks)
    }
}

fn extract_text_content(content: &ChatMessageContent) -> String {
    match content {
        ChatMessageContent::Text(text) => text.clone(),
        ChatMessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                ChatMessagePart::Text { text } => Some(text.as_str()),
                ChatMessagePart::ImageUrl { .. } => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn parse_anthropic_response(response: AnthropicMessagesResponse) -> ChatCompletionResponse {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in &response.content {
        match block {
            AnthropicContentBlock::Text { text } => text_parts.push(text.clone()),
            AnthropicContentBlock::ToolUse {
                id,
                name,
                input,
            } => {
                tool_calls.push(ToolCall {
                    id: id.clone(),
                    function: ToolCallFunction {
                        name: name.clone(),
                        arguments: serde_json::to_string(input).unwrap_or_default(),
                    },
                });
            }
            AnthropicContentBlock::Unknown => {}
        }
    }

    let content_str = text_parts.join("");
    let assistant_message = AssistantMessage {
        content: if content_str.is_empty() && tool_calls.is_empty() {
            None
        } else {
            Some(AssistantContent::Text(content_str))
        },
        tool_calls,
    };

    ChatCompletionResponse {
        choices: vec![ChatCompletionChoice {
            message: assistant_message,
        }],
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: ChatModelKind,
    pub supports_image_input: bool,
    pub total_cost: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatModelKind {
    Chat,
    Image,
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
    image_input: Option<bool>,
    #[serde(default)]
    capabilities: Option<Vec<String>>,
    #[serde(default)]
    input_modalities: Option<Vec<String>>,
    #[serde(default)]
    pricing: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageGenerationRequest {
    pub model: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageGenerationResponse {
    pub images: Vec<GeneratedImage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedImage {
    pub url: String,
}

pub async fn get_available_models(provider: &AiProviderConfig) -> Result<Vec<ChatModel>, String> {
    if let Some(ref model_id) = provider.default_model {
        return Ok(vec![ChatModel {
            id: model_id.clone(),
            name: model_id.clone(),
            description: String::new(),
            kind: ChatModelKind::Chat,
            supports_image_input: false,
            total_cost: None,
        }]);
    }

    let mut models = if provider.provider_kind == AiProviderKind::Anthropic {
        fetch_anthropic_models(provider).await?
    } else {
        fetch_models_from_endpoint(provider, "models").await?
    };

    if provider.provider_kind == AiProviderKind::Ppq {
        match fetch_models_from_endpoint(provider, "models?type=image").await {
            Ok(mut image_models) => models.append(&mut image_models),
            Err(err) if err.starts_with("404:") => {}
            Err(err) => {
                log::warn!(
                    "Failed to fetch PPQ image models: {}",
                    err.trim_start_matches("request_failed:").trim()
                );
            }
        }
    }
    sort_and_dedup_models(&mut models);
    Ok(models)
}

pub async fn send_chat_message(
    provider: &AiProviderConfig,
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, String> {
    if provider.provider_kind == AiProviderKind::Anthropic {
        return send_anthropic_chat_message(provider, request).await;
    }

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

async fn send_anthropic_chat_message(
    provider: &AiProviderConfig,
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, String> {
    let anthropic_req = convert_to_anthropic_request(request);
    let url = format!("{}/messages", provider.base_url);
    let body = serde_json::to_vec(&anthropic_req)
        .map_err(|e| format!("Failed to serialize Anthropic request: {}", e))?;
    let response = send_provider_request(provider, Method::POST, &url, Some(body)).await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Anthropic request failed ({}): {}", status, body));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read Anthropic response: {}", e))?;
    let anthropic_resp: AnthropicMessagesResponse = serde_json::from_str(&body).map_err(|e| {
        format!(
            "Failed to parse Anthropic response: {}. Body preview: {}",
            e,
            preview_body(&body)
        )
    })?;

    Ok(parse_anthropic_response(anthropic_resp))
}

async fn fetch_anthropic_models(
    provider: &AiProviderConfig,
) -> Result<Vec<ChatModel>, String> {
    let url = format!("{}/models", provider.base_url);
    let response = send_provider_request(provider, Method::GET, &url, None).await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Failed to fetch Anthropic models ({}): {}",
            status, body
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read Anthropic models response: {}", e))?;
    let parsed: AnthropicModelsResponse = serde_json::from_str(&body).map_err(|e| {
        format!(
            "Failed to parse Anthropic models response: {}. Body preview: {}",
            e,
            preview_body(&body)
        )
    })?;

    Ok(parsed
        .data
        .into_iter()
        .map(|model| ChatModel {
            name: if model.display_name.trim().is_empty() {
                model.id.clone()
            } else {
                model.display_name
            },
            id: model.id,
            description: String::new(),
            kind: ChatModelKind::Chat,
            supports_image_input: model
                .capabilities
                .as_ref()
                .and_then(|c| c.image_input.as_ref())
                .map(|s| s.supported)
                .unwrap_or(false),
            total_cost: None,
        })
        .collect())
}

async fn fetch_models_from_endpoint(
    provider: &AiProviderConfig,
    endpoint: &str,
) -> Result<Vec<ChatModel>, String> {
    let url = format!("{}/{}", provider.base_url, endpoint);
    let response = send_provider_request(provider, Method::GET, &url, None).await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let prefix = if status == StatusCode::NOT_FOUND {
            "404"
        } else {
            "request_failed"
        };
        return Err(format!(
            "{}:Failed to fetch {} ({}): {}",
            prefix, endpoint, status, body
        ));
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

    parsed
        .data
        .retain(|model| is_supported_model(provider, endpoint, model));

    Ok(parsed
        .data
        .into_iter()
        .map(|model| {
            let kind = model_kind(provider, endpoint, &model).unwrap_or(ChatModelKind::Chat);
            let supports_image_input = model_supports_image_input(&model);
            ChatModel {
                name: if model.name.trim().is_empty() {
                    model.id.clone()
                } else {
                    model.name
                },
                id: model.id,
                description: model.description,
                kind,
                supports_image_input,
                total_cost: model.pricing.as_ref().and_then(parse_total_cost),
            }
        })
        .collect())
}

fn sort_and_dedup_models(models: &mut Vec<ChatModel>) {
    fn dedup_key(model: &ChatModel) -> String {
        let kind = match model.kind {
            ChatModelKind::Chat => "chat",
            ChatModelKind::Image => "image",
        };
        format!("{}:{kind}", model.id)
    }

    let mut deduped = std::collections::BTreeMap::<String, ChatModel>::new();
    for model in models.drain(..) {
        let key = dedup_key(&model);
        match deduped.get_mut(&key) {
            Some(existing) => {
                if existing.description.is_empty() && !model.description.is_empty() {
                    existing.description = model.description.clone();
                }
                if existing.total_cost.is_none() {
                    existing.total_cost = model.total_cost;
                }
                if !existing.supports_image_input && model.supports_image_input {
                    existing.supports_image_input = true;
                }
                if existing.name == existing.id && model.name != model.id {
                    existing.name = model.name.clone();
                }
            }
            None => {
                deduped.insert(key, model);
            }
        }
    }
    models.extend(deduped.into_values());
    models.sort_by(|a, b| {
        model_kind_rank(a.kind)
            .cmp(&model_kind_rank(b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });
    models.sort_by(|a, b| {
        model_kind_rank(a.kind)
            .cmp(&model_kind_rank(b.kind))
            .then_with(|| match (a.total_cost, b.total_cost) {
                (Some(a_cost), Some(b_cost)) => a_cost
                    .partial_cmp(&b_cost)
                    .unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.name.cmp(&b.name),
            })
    });
}

fn is_supported_model(provider: &AiProviderConfig, endpoint: &str, model: &WireModel) -> bool {
    match provider.provider_kind {
        AiProviderKind::Ppq | AiProviderKind::OpenAiCompatible => {
            model_kind(provider, endpoint, model).is_some()
        }
        AiProviderKind::Anthropic => model_kind(provider, endpoint, model).is_some(),
    }
}

fn model_kind(
    provider: &AiProviderConfig,
    endpoint: &str,
    model: &WireModel,
) -> Option<ChatModelKind> {
    if provider.provider_kind == AiProviderKind::Anthropic {
        return Some(ChatModelKind::Chat);
    }

    if provider.provider_kind == AiProviderKind::Ppq {
        if endpoint.starts_with("models?type=image") {
            return Some(ChatModelKind::Image);
        }
        if endpoint.starts_with("models?type=video") {
            return None;
        }
        if endpoint == "models" || endpoint.starts_with("models?type=chat") {
            return Some(ChatModelKind::Chat);
        }
    }

    let model_type = model.model_type.trim().to_ascii_lowercase();
    if model_type.is_empty() {
        return Some(ChatModelKind::Chat);
    }
    if model_type == "image" || model_type == "images" || model_type == "image_generation" {
        return Some(ChatModelKind::Image);
    }
    if model_type == "video" || model_type == "videos" || model_type == "video_generation" {
        return None;
    }
    if matches!(
        model_type.as_str(),
        "chat" | "text" | "llm" | "language" | "completion" | "completions"
    ) {
        return Some(ChatModelKind::Chat);
    }

    None
}

fn model_supports_image_input(model: &WireModel) -> bool {
    if model.image_input == Some(true) {
        return true;
    }

    model
        .capabilities
        .iter()
        .flatten()
        .chain(model.input_modalities.iter().flatten())
        .map(|value| value.trim().to_ascii_lowercase())
        .any(|value| {
            matches!(
                value.as_str(),
                "image" | "images" | "image_input" | "vision"
            )
        })
}

fn model_kind_rank(kind: ChatModelKind) -> u8 {
    match kind {
        ChatModelKind::Chat => 0,
        ChatModelKind::Image => 1,
    }
}

pub async fn generate_images(
    provider: &AiProviderConfig,
    request: &ImageGenerationRequest,
) -> Result<ImageGenerationResponse, String> {
    let endpoint = if request.image_url.is_none() {
        "images/generations"
    } else {
        "images/edits"
    };
    let url = format!("{}/{}", provider.base_url, endpoint);
    let body = serde_json::to_vec(request)
        .map_err(|e| format!("Failed to serialize image request: {}", e))?;
    let response = send_provider_request(provider, Method::POST, &url, Some(body)).await?;
    parse_image_generation_response(response).await
}

async fn parse_image_generation_response(
    response: Response,
) -> Result<ImageGenerationResponse, String> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Image request failed ({}): {}", status, body));
    }

    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse image response: {}", e))?;
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let images = data
        .into_iter()
        .filter_map(|item| {
            let url = item.get("url").and_then(Value::as_str);
            let image_url = item.get("image_url");
            let image_url_string = image_url.and_then(Value::as_str);
            let nested_image_url = image_url
                .and_then(Value::as_object)
                .and_then(|map| map.get("url"))
                .and_then(Value::as_str);
            let b64_json = item.get("b64_json").and_then(Value::as_str);
            let base64 = item.get("base64").and_then(Value::as_str);

            url.and_then(normalize_generated_image_reference)
                .or_else(|| {
                    image_url_string
                        .or(nested_image_url)
                        .and_then(normalize_generated_image_reference)
                })
                .or_else(|| b64_json.and_then(normalize_generated_image_reference))
                .or_else(|| base64.and_then(normalize_generated_image_reference))
                .map(|url| GeneratedImage { url })
        })
        .collect::<Vec<_>>();

    if images.is_empty() {
        let safe_preview: String = serde_json::to_string(&value)
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        return Err(format!(
            "Image response did not include any images: {}",
            preview_body(&safe_preview)
        ));
    }

    Ok(ImageGenerationResponse { images })
}

fn normalize_generated_image_reference(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("data:image/")
        || trimmed.starts_with("blob:")
    {
        return Some(trimmed.to_string());
    }

    if looks_like_base64(trimmed) {
        return Some(format!("data:image/png;base64,{}", trimmed));
    }

    None
}

fn looks_like_base64(value: &str) -> bool {
    value.len() >= 128
        && value.len() % 4 == 0
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'\n' | b'\r')
        })
}

async fn send_provider_request(
    provider: &AiProviderConfig,
    method: Method,
    url: &str,
    body: Option<Vec<u8>>,
) -> Result<Response, String> {
    let client = http_client().map_err(|e| format!("HTTP client init failed: {}", e))?;

    match &provider.auth {
        ProviderAuth::PpqManaged { api_key } => {
            let Some(api_key) = api_key
                .as_deref()
                .map(str::trim)
                .filter(|key| !key.is_empty())
            else {
                return Err("PPQ account is not set up yet".to_string());
            };
            let mut request = client.request(method, url);
            request = request.header("Authorization", format!("Bearer {api_key}"));
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
        ProviderAuth::Routstr { api_key } => {
            let Some(api_key) = api_key
                .as_deref()
                .map(str::trim)
                .filter(|key| !key.is_empty())
            else {
                return Err("Routstr account is not set up yet".to_string());
            };
            let mut request = client.request(method, url);
            request = request.header("Authorization", format!("Bearer {api_key}"));
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
        ProviderAuth::XApiKey(api_key) => {
            let mut request = client.request(method, url);
            request = request
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01");
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
    body.trim().chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::ai_provider_store::{
        ppq_provider, AiProviderConfig, AiProviderKind, ProviderAuth,
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
    fn preview_body_is_utf8_safe_for_multibyte_text() {
        let body = format!("  {}  ", "🙂".repeat(250));
        let preview = preview_body(&body);
        assert_eq!(preview.chars().count(), 200);
        assert!(preview.chars().all(|ch| ch == '🙂'));
    }

    #[test]
    fn sort_and_dedup_models_keeps_distinct_kinds_for_same_id() {
        let mut models = vec![
            ChatModel {
                id: "google/gemini-2.5-flash".to_string(),
                name: "Gemini Chat".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: false,
                total_cost: None,
            },
            ChatModel {
                id: "google/gemini-2.5-flash".to_string(),
                name: "Gemini Image".to_string(),
                description: "image".to_string(),
                kind: ChatModelKind::Image,
                supports_image_input: false,
                total_cost: None,
            },
        ];

        sort_and_dedup_models(&mut models);

        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|model| model.kind == ChatModelKind::Chat));
        assert!(models
            .iter()
            .any(|model| model.kind == ChatModelKind::Image));
    }

    #[test]
    fn keeps_chat_and_image_models_for_ppq() {
        let provider = ppq_provider(None);
        assert!(is_supported_model(
            &provider,
            "models",
            &WireModel {
                id: "claude".to_string(),
                name: "Claude".to_string(),
                description: String::new(),
                model_type: "chat".to_string(),
                image_input: None,
                capabilities: None,
                input_modalities: None,
                pricing: None,
            }
        ));
        assert!(is_supported_model(
            &provider,
            "models?type=image",
            &WireModel {
                id: "image".to_string(),
                name: "Image".to_string(),
                description: String::new(),
                model_type: String::new(),
                image_input: None,
                capabilities: None,
                input_modalities: None,
                pricing: None,
            }
        ));
    }

    #[test]
    fn openai_compatible_provider_supports_default_chat_models() {
        let provider = AiProviderConfig {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            base_url: "https://example.com/v1".to_string(),
            provider_kind: AiProviderKind::OpenAiCompatible,
            auth: ProviderAuth::BearerToken("secret".to_string()),
            is_builtin: false,
            default_model: None,
        };
        assert!(is_supported_model(
            &provider,
            "models",
            &WireModel {
                id: "gpt".to_string(),
                name: String::new(),
                description: String::new(),
                model_type: String::new(),
                image_input: None,
                capabilities: None,
                input_modalities: None,
                pricing: None,
            }
        ));
    }

    #[test]
    fn parses_image_models() {
        let provider = AiProviderConfig {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            base_url: "https://example.com/v1".to_string(),
            provider_kind: AiProviderKind::OpenAiCompatible,
            auth: ProviderAuth::BearerToken("secret".to_string()),
            is_builtin: false,
            default_model: None,
        };
        let model = WireModel {
            id: "image-model".to_string(),
            name: "Image Model".to_string(),
            description: String::new(),
            model_type: "image".to_string(),
            image_input: None,
            capabilities: None,
            input_modalities: None,
            pricing: None,
        };

        assert_eq!(
            model_kind(&provider, "models", &model),
            Some(ChatModelKind::Image)
        );
    }

    #[test]
    fn ppq_model_kind_uses_endpoint_modality() {
        let provider = ppq_provider(None);
        let model = WireModel {
            id: "google/gemini-2.5-flash-image".to_string(),
            name: "Gemini 2.5 Flash Image".to_string(),
            description: String::new(),
            model_type: String::new(),
            image_input: None,
            capabilities: None,
            input_modalities: None,
            pricing: None,
        };

        assert_eq!(
            model_kind(&provider, "models", &model),
            Some(ChatModelKind::Chat)
        );
        assert_eq!(
            model_kind(&provider, "models?type=image", &model),
            Some(ChatModelKind::Image)
        );
        assert_eq!(model_kind(&provider, "models?type=video", &model), None);
    }

    #[test]
    fn sort_and_dedup_prefers_image_kind_for_duplicate_ids() {
        let mut models = vec![
            ChatModel {
                id: "google/gemini-2.5-flash-image".to_string(),
                name: "Gemini".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
            ChatModel {
                id: "google/gemini-2.5-flash-image".to_string(),
                name: "Gemini".to_string(),
                description: String::new(),
                kind: ChatModelKind::Image,
                supports_image_input: false,
                total_cost: None,
            },
        ];

        sort_and_dedup_models(&mut models);
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|model| model.kind == ChatModelKind::Chat));
        assert!(models
            .iter()
            .any(|model| model.kind == ChatModelKind::Image));
    }

    #[test]
    fn rejects_unknown_explicit_model_types() {
        let provider = AiProviderConfig {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            base_url: "https://example.com/v1".to_string(),
            provider_kind: AiProviderKind::OpenAiCompatible,
            auth: ProviderAuth::BearerToken("secret".to_string()),
            is_builtin: false,
            default_model: None,
        };
        let model = WireModel {
            id: "embedding-model".to_string(),
            name: "Embedding".to_string(),
            description: String::new(),
            model_type: "embedding".to_string(),
            image_input: Some(true),
            capabilities: None,
            input_modalities: None,
            pricing: None,
        };

        assert_eq!(model_kind(&provider, "models", &model), None);
        assert!(!is_supported_model(&provider, "models", &model));
    }

    #[test]
    fn supports_image_input_only_with_explicit_metadata() {
        let with_capability = WireModel {
            id: "vision-model".to_string(),
            name: "Vision".to_string(),
            description: String::new(),
            model_type: "chat".to_string(),
            image_input: None,
            capabilities: Some(vec!["vision".to_string()]),
            input_modalities: None,
            pricing: None,
        };
        let without_capability = WireModel {
            id: "chat-model".to_string(),
            name: "Chat".to_string(),
            description: String::new(),
            model_type: "chat".to_string(),
            image_input: None,
            capabilities: None,
            input_modalities: None,
            pricing: None,
        };

        assert!(model_supports_image_input(&with_capability));
        assert!(!model_supports_image_input(&without_capability));
    }

    #[test]
    fn normalizes_raw_base64_image_payloads() {
        let base64 = "QUJD".repeat(64);
        let normalized = normalize_generated_image_reference(&base64);
        assert_eq!(
            normalized,
            Some(format!("data:image/png;base64,{}", base64))
        );
    }

    #[test]
    fn preserves_http_image_urls() {
        let url = "https://cdn.example.com/image.png?token=abc";
        assert_eq!(
            normalize_generated_image_reference(url),
            Some(url.to_string())
        );
    }

    #[test]
    fn converts_openai_request_to_anthropic_format() {
        let request = ChatCompletionRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![
                ChatMessage {
                    role: ChatRole::System,
                    content: ChatMessageContent::Text("You are helpful.".to_string()),
                    tool_call_id: None,
                    tool_calls: None,
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: ChatMessageContent::Text("Hello".to_string()),
                    tool_call_id: None,
                    tool_calls: None,
                },
            ],
            tools: None,
        };

        let anthropic_req = convert_to_anthropic_request(&request);
        assert_eq!(anthropic_req.system, Some("You are helpful.".to_string()));
        assert_eq!(anthropic_req.max_tokens, 8192);
        assert_eq!(anthropic_req.messages.len(), 1);
    }

    #[test]
    fn converts_tool_calls_to_anthropic_format() {
        let request = ChatCompletionRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![
                ChatMessage {
                    role: ChatRole::User,
                    content: ChatMessageContent::Text("What's the weather?".to_string()),
                    tool_call_id: None,
                    tool_calls: None,
                },
                ChatMessage {
                    role: ChatRole::Assistant,
                    content: ChatMessageContent::Text(String::new()),
                    tool_call_id: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "toolu_123".to_string(),
                        function: ToolCallFunction {
                            name: "get_weather".to_string(),
                            arguments: r#"{"location":"SF"}"#.to_string(),
                        },
                    }]),
                },
                ChatMessage {
                    role: ChatRole::Tool,
                    content: ChatMessageContent::Text("15 degrees".to_string()),
                    tool_call_id: Some("toolu_123".to_string()),
                    tool_calls: None,
                },
            ],
            tools: None,
        };

        let anthropic_req = convert_to_anthropic_request(&request);
        assert_eq!(anthropic_req.system, None);
        assert_eq!(anthropic_req.messages.len(), 3);
    }

    #[test]
    fn parses_anthropic_response_with_tool_use() {
        let response = AnthropicMessagesResponse {
            content: vec![
                AnthropicContentBlock::Text {
                    text: "Let me check.".to_string(),
                },
                AnthropicContentBlock::ToolUse {
                    id: "toolu_abc".to_string(),
                    name: "get_weather".to_string(),
                    input: serde_json::json!({"location": "SF"}),
                },
            ],
            stop_reason: Some("tool_use".to_string()),
        };

        let parsed = parse_anthropic_response(response);
        assert_eq!(parsed.choices.len(), 1);
        let msg = &parsed.choices[0].message;
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].id, "toolu_abc");
        assert_eq!(msg.tool_calls[0].function.name, "get_weather");
    }

    #[test]
    fn parses_anthropic_text_only_response() {
        let response = AnthropicMessagesResponse {
            content: vec![AnthropicContentBlock::Text {
                text: "Hello there!".to_string(),
            }],
            stop_reason: Some("end_turn".to_string()),
        };

        let parsed = parse_anthropic_response(response);
        assert_eq!(parsed.choices.len(), 1);
        let msg = &parsed.choices[0].message;
        assert!(msg.tool_calls.is_empty());
        assert_eq!(
            msg.content,
            Some(AssistantContent::Text("Hello there!".to_string()))
        );
    }

    #[test]
    fn converts_tool_definitions_to_anthropic_format() {
        let request = ChatCompletionRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: ChatMessageContent::Text("test".to_string()),
                tool_call_id: None,
                tool_calls: None,
            }],
            tools: Some(vec![ToolDefinition {
                tool_type: "function".to_string(),
                function: ToolFunction {
                    name: "get_profile".to_string(),
                    description: "Get a profile".to_string(),
                    parameters: serde_json::json!({"type": "object", "properties": {}}),
                },
            }]),
        };

        let anthropic_req = convert_to_anthropic_request(&request);
        let tools = anthropic_req.tools.unwrap();
        assert_eq!(tools[0].name, "get_profile");
        assert_eq!(tools[0].description, "Get a profile");
    }

    #[tokio::test]
    async fn returns_default_model_without_fetching() {
        let provider = AiProviderConfig {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            base_url: "https://example.com/v1".to_string(),
            provider_kind: AiProviderKind::Anthropic,
            auth: ProviderAuth::XApiKey("secret".to_string()),
            is_builtin: false,
            default_model: Some("claude-sonnet-4-20250514".to_string()),
        };

        let models = get_available_models(&provider).await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "claude-sonnet-4-20250514");
        assert_eq!(models[0].kind, ChatModelKind::Chat);
    }
}
