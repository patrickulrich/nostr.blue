//! Embedding Store
//! Handles NKBIP-02 Vector Embeddings - Kind 1987
//!
//! Vector embeddings for semantic search:
//! - Reference to source event
//! - Model information with optional download link
//! - Vector as comma-separated floats
//! - Content type (text, image, audio, video)
#![allow(dead_code)]
use dioxus::prelude::*;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::time::Duration;
type StdResult<T, E> = std::result::Result<T, E>;
/// Kind 1987 - Vector Embedding
pub const KIND_EMBEDDING: u16 = 1987;
/// Maximum dimensions for display
const MAX_DISPLAY_DIMS: usize = 10;
/// Content type for embeddings
#[derive(Clone, Debug, PartialEq, Default)]
pub enum EmbeddingContentType {
    #[default]
    Text,
    Image,
    Audio,
    Video,
}
impl EmbeddingContentType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "image" => Self::Image,
            "audio" => Self::Audio,
            "video" => Self::Video,
            _ => Self::Text,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
        }
    }
}
/// Parsed embedding event
#[derive(Clone, Debug)]
pub struct EmbeddingEvent {
    pub event: NostrEvent,
    pub event_id: String,
    /// ID of the event being embedded
    pub source_event_id: String,
    /// Model name/version
    pub model: String,
    /// Model download link (optional)
    pub model_download: Option<String>,
    /// Content type
    pub content_type: EmbeddingContentType,
    /// Embedding vector
    pub vector: Vec<f32>,
    /// Vector dimensions
    pub dimensions: usize,
    /// Whether vector is normalized to [0,1]
    pub normalized: bool,
    /// Original text snippet (for text embeddings)
    pub source_text: Option<String>,
    /// Model hash for verification
    pub model_hash: Option<String>,
    /// Hash type (e.g., "sha256")
    pub hash_type: Option<String>,
}
impl PartialEq for EmbeddingEvent {
    fn eq(&self, other: &Self) -> bool {
        self.event_id == other.event_id
    }
}
/// Model info for display
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingModel {
    pub name: String,
    pub download_url: Option<String>,
    pub dimensions: usize,
    pub hash: Option<String>,
}
/// Embeddings cache (keyed by source event ID -> list of embeddings)
pub static EMBEDDINGS_CACHE: GlobalSignal<HashMap<String, Vec<EmbeddingEvent>>> =
    GlobalSignal::new(HashMap::new);
/// Known models
pub static KNOWN_MODELS: GlobalSignal<HashMap<String, EmbeddingModel>> =
    GlobalSignal::new(HashMap::new);
/// Loading state
pub static LOADING_EMBEDDINGS: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Get embeddings for a source event
pub fn get_cached_embeddings(source_event_id: &str) -> Vec<EmbeddingEvent> {
    EMBEDDINGS_CACHE
        .read()
        .get(source_event_id)
        .cloned()
        .unwrap_or_default()
}
/// Cache embeddings for a source event
pub fn cache_embeddings(source_event_id: &str, embeddings: Vec<EmbeddingEvent>) {
    EMBEDDINGS_CACHE
        .write()
        .insert(source_event_id.to_string(), embeddings);
}
/// Add a single embedding to cache
pub fn add_embedding_to_cache(embedding: EmbeddingEvent) {
    let mut cache = EMBEDDINGS_CACHE.write();
    let entry = cache.entry(embedding.source_event_id.clone()).or_default();
    if !entry.iter().any(|e| e.event_id == embedding.event_id) {
        entry.push(embedding);
    }
}
/// Clear cache
pub fn clear_cache() {
    EMBEDDINGS_CACHE.write().clear();
    KNOWN_MODELS.write().clear();
}
/// Parse a Kind 1987 event into an EmbeddingEvent
pub fn parse_embedding_event(event: &NostrEvent) -> Option<EmbeddingEvent> {
    if event.kind.as_u16() != KIND_EMBEDDING {
        return None;
    }
    let mut source_event_id: Option<String> = None;
    let mut model: Option<String> = None;
    let mut model_download: Option<String> = None;
    let mut content_type = EmbeddingContentType::default();
    let mut vector: Vec<f32> = Vec::new();
    let mut dimensions: Option<usize> = None;
    let mut normalized = false;
    let mut source_text: Option<String> = None;
    let mut model_hash: Option<String> = None;
    let mut hash_type: Option<String> = None;
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        match slice.first().map(|s| s.as_str()) {
            Some("e") => {
                source_event_id = slice.get(1).map(|s| s.to_string());
            }
            Some("model") => {
                model = slice.get(1).map(|s| s.to_string());
                model_download = slice.get(2).map(|s| s.to_string());
            }
            Some("type") => {
                if let Some(t) = slice.get(1) {
                    content_type = EmbeddingContentType::from_str(t);
                }
            }
            Some("vector") => {
                if let Some(v) = slice.get(1) {
                    vector = v
                        .split(',')
                        .filter_map(|s| s.trim().parse::<f32>().ok())
                        .collect();
                }
            }
            Some("dims") => {
                dimensions = slice.get(1).and_then(|s| s.parse().ok());
            }
            Some("norm") => {
                if let Some(n) = slice.get(1) {
                    normalized = n == "true" || n == "1";
                }
            }
            Some("source") => {
                source_text = slice.get(1).map(|s| s.to_string());
            }
            Some("hash") => {
                model_hash = slice.get(1).map(|s| s.to_string());
            }
            Some("hash_type") => {
                hash_type = slice.get(1).map(|s| s.to_string());
            }
            _ => {}
        }
    }
    let source_event_id = source_event_id?;
    let model = model?;
    if vector.is_empty() {
        return None;
    }
    let dimensions = dimensions.unwrap_or(vector.len());
    Some(EmbeddingEvent {
        event: event.clone(),
        event_id: event.id.to_hex(),
        source_event_id,
        model,
        model_download,
        content_type,
        vector,
        dimensions,
        normalized,
        source_text,
        model_hash,
        hash_type,
    })
}
/// Build filter for embeddings by source event ID
/// Returns None if the source_event_id is invalid hex
pub fn embeddings_filter(source_event_id: &str) -> Option<Filter> {
    let event_id = EventId::from_hex(source_event_id).ok()?;
    Some(
        Filter::new()
            .kind(Kind::Custom(KIND_EMBEDDING))
            .event(event_id),
    )
}
/// Build filter for embeddings by author
pub fn embeddings_by_author_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_EMBEDDING))
        .author(pubkey)
        .limit(limit)
}
/// Build filter for embeddings by model
pub fn embeddings_by_model_filter(model: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_EMBEDDING))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::M), model)
        .limit(limit)
}
/// Build filter for recent embeddings
pub fn recent_embeddings_filter(limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_EMBEDDING))
        .limit(limit)
}
/// Fetch embeddings for a source event
pub async fn fetch_embeddings_for_event(
    source_event_id: &str,
) -> StdResult<Vec<EmbeddingEvent>, String> {
    let cached = get_cached_embeddings(source_event_id);
    if !cached.is_empty() {
        return Ok(cached);
    }
    *LOADING_EMBEDDINGS.write() = true;
    let filter = match embeddings_filter(source_event_id) {
        Some(f) => f,
        None => {
            *LOADING_EMBEDDINGS.write() = false;
            return Err(format!("Invalid event ID: {}", source_event_id));
        }
    };
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await;
    *LOADING_EMBEDDINGS.write() = false;
    match result {
        Ok(events) => {
            let embeddings: Vec<EmbeddingEvent> =
                events.iter().filter_map(parse_embedding_event).collect();
            cache_embeddings(source_event_id, embeddings.clone());
            log::info!(
                "Fetched {} embeddings for event {}",
                embeddings.len(),
                source_event_id
            );
            Ok(embeddings)
        }
        Err(e) => {
            log::error!("Failed to fetch embeddings: {}", e);
            Err(e)
        }
    }
}
/// Fetch embeddings by author
pub async fn fetch_embeddings_by_author(
    pubkey_hex: &str,
    limit: usize,
) -> StdResult<Vec<EmbeddingEvent>, String> {
    let pubkey = PublicKey::from_hex(pubkey_hex).map_err(|e| format!("Invalid pubkey: {}", e))?;
    *LOADING_EMBEDDINGS.write() = true;
    let filter = embeddings_by_author_filter(pubkey, limit);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await;
    *LOADING_EMBEDDINGS.write() = false;
    match result {
        Ok(events) => {
            let embeddings: Vec<EmbeddingEvent> =
                events.iter().filter_map(parse_embedding_event).collect();
            for embedding in &embeddings {
                add_embedding_to_cache(embedding.clone());
            }
            Ok(embeddings)
        }
        Err(e) => Err(e),
    }
}
/// Calculate cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}
/// Calculate Euclidean distance between two vectors
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::MAX;
    }
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}
/// Find most similar embeddings to a query vector
pub fn find_similar(
    query: &[f32],
    embeddings: &[EmbeddingEvent],
    limit: usize,
) -> Vec<(EmbeddingEvent, f32)> {
    let mut scored: Vec<(EmbeddingEvent, f32)> = embeddings
        .iter()
        .map(|e| {
            let similarity = cosine_similarity(query, &e.vector);
            (e.clone(), similarity)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}
/// Publish a vector embedding
pub async fn publish_embedding(
    source_event_id: &str,
    model: &str,
    model_download: Option<&str>,
    content_type: EmbeddingContentType,
    vector: &[f32],
    normalized: bool,
    source_text: Option<&str>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let source_id = EventId::from_hex(source_event_id)
        .map_err(|e| format!("Invalid source event ID: {}", e))?;
    let vector_str = vector
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut tags: Vec<Tag> = vec![
        Tag::event(source_id),
        Tag::custom(
            TagKind::Custom("type".into()),
            vec![content_type.as_str().to_string()],
        ),
        Tag::custom(TagKind::Custom("vector".into()), vec![vector_str]),
        Tag::custom(
            TagKind::Custom("dims".into()),
            vec![vector.len().to_string()],
        ),
    ];
    let mut model_values = vec![model.to_string()];
    if let Some(download) = model_download {
        model_values.push(download.to_string());
    }
    tags.push(Tag::custom(TagKind::Custom("model".into()), model_values));
    if normalized {
        tags.push(Tag::custom(
            TagKind::Custom("norm".into()),
            vec!["true".to_string()],
        ));
    }
    if let Some(text) = source_text {
        tags.push(Tag::custom(
            TagKind::Custom("source".into()),
            vec![text.to_string()],
        ));
    }
    for mime_tag in crate::utils::nkbip06::generate_mime_tags(KIND_EMBEDDING) {
        tags.push(mime_tag);
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_EMBEDDING), "").tags(tags);
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish embedding: {}", e))?;
    log::info!("Embedding published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_cosine_similarity_same_vector() {
        let v = vec![1.0, 2.0, 3.0];
        let similarity = cosine_similarity(&v, &v);
        assert!((similarity - 1.0).abs() < 0.0001);
    }
    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let similarity = cosine_similarity(&a, &b);
        assert!(similarity.abs() < 0.0001);
    }
    #[test]
    fn test_euclidean_distance_same_point() {
        let v = vec![1.0, 2.0, 3.0];
        let distance = euclidean_distance(&v, &v);
        assert!(distance.abs() < 0.0001);
    }
    #[test]
    fn test_euclidean_distance_known() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        let distance = euclidean_distance(&a, &b);
        assert!((distance - 5.0).abs() < 0.0001);
    }
    #[test]
    fn test_content_type_from_str() {
        assert_eq!(
            EmbeddingContentType::from_str("text"),
            EmbeddingContentType::Text
        );
        assert_eq!(
            EmbeddingContentType::from_str("image"),
            EmbeddingContentType::Image
        );
        assert_eq!(
            EmbeddingContentType::from_str("audio"),
            EmbeddingContentType::Audio
        );
        assert_eq!(
            EmbeddingContentType::from_str("video"),
            EmbeddingContentType::Video
        );
        assert_eq!(
            EmbeddingContentType::from_str("unknown"),
            EmbeddingContentType::Text,
        );
    }
}
