//! Bible Store
//! Handles Bible reading state, chapter caching, and NIP-84 highlights (Kind 9802)
//!
//! Uses HelloAO Bible API for content and Nostr for social highlighting features

#![allow(dead_code)]

use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use nostr::Event as NostrEvent;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Duration;

// Re-export types from bible_api for use by routes
pub use crate::services::bible_api::{
    Translation, Book, ChapterResponse, ChapterContent, VerseContent,
    fetch_translations, fetch_books, fetch_chapter, get_chapter_api_url,
    verse_to_plain_text, filter_english_translations, sort_translations_by_priority,
};

// Use std::result::Result to avoid ambiguity with dioxus/nostr_sdk Result types
type StdResult<T, E> = std::result::Result<T, E>;

// ============================================================================
// Constants
// ============================================================================

/// NIP-84 Highlights use Kind 9802
pub const KIND_HIGHLIGHT: u16 = 9802;

/// Cache sizes
const CHAPTER_CACHE_SIZE: usize = 100;
const HIGHLIGHT_CACHE_SIZE: usize = 500;

/// Default translation
pub const DEFAULT_TRANSLATION: &str = "BSB";

// ============================================================================
// Data Structures
// ============================================================================

/// A parsed highlight from a Kind 9802 event
#[derive(Clone, Debug, PartialEq)]
pub struct BibleHighlight {
    /// Original Nostr event
    pub event: NostrEvent,
    /// The highlighted verse text (from event content)
    pub verse_text: String,
    /// Human-readable reference (e.g., "John 3:16 (BSB)") from context tag
    pub reference: String,
    /// Optional user comment from comment tag
    pub comment: Option<String>,
    /// API URL for the chapter (from r tag) - used for filtering
    pub api_url: String,
    /// Author's pubkey
    pub pubkey: String,
    /// Created timestamp
    pub created_at: u64,
}

/// Cached chapter with highlight data
#[derive(Clone, Debug)]
pub struct CachedChapter {
    pub response: ChapterResponse,
    pub api_url: String,
    pub fetched_at: u64,
}

/// Highlight counts per verse for a chapter
#[derive(Clone, Debug, Default)]
pub struct ChapterHighlightStats {
    /// Map of verse number -> highlight count
    pub verse_counts: HashMap<u32, usize>,
    /// Total highlights for the chapter
    pub total: usize,
}

// ============================================================================
// Global Signals
// ============================================================================

/// All available translations
pub static TRANSLATIONS: GlobalSignal<Vec<Translation>> = GlobalSignal::new(Vec::new);

/// English translations only (filtered for UI)
pub static ENGLISH_TRANSLATIONS: GlobalSignal<Vec<Translation>> = GlobalSignal::new(Vec::new);

/// Currently selected translation
pub static CURRENT_TRANSLATION: GlobalSignal<String> =
    GlobalSignal::new(|| DEFAULT_TRANSLATION.to_string());

/// Books for current translation
pub static CURRENT_BOOKS: GlobalSignal<Vec<Book>> = GlobalSignal::new(Vec::new);

/// Chapter cache (keyed by "translation:book:chapter")
pub static CHAPTER_CACHE: GlobalSignal<LruCache<String, CachedChapter>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(CHAPTER_CACHE_SIZE).unwrap()));

/// User's own highlights
pub static USER_HIGHLIGHTS: GlobalSignal<Vec<BibleHighlight>> = GlobalSignal::new(Vec::new);

/// Highlights for current chapter (from all users)
pub static CURRENT_CHAPTER_HIGHLIGHTS: GlobalSignal<Vec<BibleHighlight>> = GlobalSignal::new(Vec::new);

/// Loading states
pub static LOADING_TRANSLATIONS: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_BOOKS: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_CHAPTER: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_HIGHLIGHTS: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Store initialization flag
pub static BIBLE_STORE_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Last viewed position (for "Continue Reading" feature)
/// Tuple: (translation, book_id, book_common_name, chapter)
pub static LAST_POSITION: GlobalSignal<Option<(String, String, String, u32)>> = GlobalSignal::new(|| None);

/// Latest requested translation for race guard (prevents stale data on fast navigation)
static LATEST_REQUESTED_TRANSLATION: GlobalSignal<String> = GlobalSignal::new(String::new);

/// Latest requested chapter key for race guard (prevents stale data on fast navigation)
static LATEST_REQUESTED_CHAPTER: GlobalSignal<String> = GlobalSignal::new(String::new);

/// Latest requested highlight URL for race guard (prevents stale data on fast navigation)
static LATEST_REQUESTED_HIGHLIGHT_URL: GlobalSignal<String> = GlobalSignal::new(String::new);

// ============================================================================
// Cache Key Functions
// ============================================================================

/// Generate cache key for a chapter
fn chapter_cache_key(translation: &str, book: &str, chapter: u32) -> String {
    format!("{}:{}:{}", translation, book, chapter)
}

// ============================================================================
// Cache Functions
// ============================================================================

/// Get a chapter from cache.
/// Uses write() + get() to properly update LRU access order.
pub fn get_cached_chapter(translation: &str, book: &str, chapter: u32) -> Option<CachedChapter> {
    let key = chapter_cache_key(translation, book, chapter);
    CHAPTER_CACHE.write().get(&key).cloned()
}

/// Cache a chapter
pub fn cache_chapter(translation: &str, book: &str, chapter: u32, response: ChapterResponse) {
    let key = chapter_cache_key(translation, book, chapter);
    let api_url = get_chapter_api_url(translation, book, chapter);
    let cached = CachedChapter {
        response,
        api_url,
        fetched_at: Timestamp::now().as_secs(),
    };
    CHAPTER_CACHE.write().put(key, cached);
}

/// Get all cached chapters (for search)
pub fn get_all_cached_chapters() -> Vec<CachedChapter> {
    CHAPTER_CACHE.read().iter().map(|(_, c)| c.clone()).collect()
}

/// Get count of cached chapters without cloning
pub fn cached_chapter_count() -> usize {
    CHAPTER_CACHE.read().len()
}

/// Clear chapter cache
pub fn clear_chapter_cache() {
    CHAPTER_CACHE.write().clear();
}

// ============================================================================
// Initialization Functions
// ============================================================================

/// Initialize the Bible store - fetch translations and default books
/// Only marks as initialized after ALL setup completes successfully
pub async fn initialize() -> StdResult<(), String> {
    if *BIBLE_STORE_INITIALIZED.read() {
        return Ok(());
    }

    *LOADING_TRANSLATIONS.write() = true;

    // Use async block to ensure LOADING_TRANSLATIONS is reset on all paths
    let result = async {
        let translations = fetch_translations().await?;

        // Filter and sort English translations
        let english = filter_english_translations(&translations);
        let sorted_english = sort_translations_by_priority(english);

        *ENGLISH_TRANSLATIONS.write() = sorted_english;
        *TRANSLATIONS.write() = translations;

        // Load books for default translation - propagate error instead of discarding
        load_books(DEFAULT_TRANSLATION).await?;

        Ok(())
    }.await;

    *LOADING_TRANSLATIONS.write() = false;

    // Only set initialized on complete success (nostr-sdk pattern)
    if result.is_ok() {
        *BIBLE_STORE_INITIALIZED.write() = true;
    }

    result
}

/// Load books for a translation
pub async fn load_books(translation: &str) -> StdResult<Vec<Book>, String> {
    // Set this as the latest requested translation for race prevention
    *LATEST_REQUESTED_TRANSLATION.write() = translation.to_string();
    *LOADING_BOOKS.write() = true;

    match fetch_books(translation).await {
        Ok(books) => {
            // Only update global state if still the latest request (nostr-sdk double-check pattern)
            if *LATEST_REQUESTED_TRANSLATION.read() == translation {
                *CURRENT_BOOKS.write() = books.clone();
                *CURRENT_TRANSLATION.write() = translation.to_string();
                *LOADING_BOOKS.write() = false;
            }
            Ok(books)
        }
        Err(e) => {
            // Only clear loading if still the latest request
            if *LATEST_REQUESTED_TRANSLATION.read() == translation {
                *LOADING_BOOKS.write() = false;
            }
            Err(e)
        }
    }
}

/// Load a chapter (with caching)
pub async fn load_chapter(translation: &str, book: &str, chapter: u32) -> StdResult<ChapterResponse, String> {
    // Check cache first
    if let Some(cached) = get_cached_chapter(translation, book, chapter) {
        return Ok(cached.response);
    }

    // Set this as the latest requested chapter for race prevention
    let chapter_key = chapter_cache_key(translation, book, chapter);
    *LATEST_REQUESTED_CHAPTER.write() = chapter_key.clone();
    *LOADING_CHAPTER.write() = true;

    match fetch_chapter(translation, book, chapter).await {
        Ok(response) => {
            cache_chapter(translation, book, chapter, response.clone());

            // Only update loading state if still the latest request (prevents race condition)
            if *LATEST_REQUESTED_CHAPTER.read() == chapter_key {
                *LOADING_CHAPTER.write() = false;
                // Update last position with book's common_name for correct display
                *LAST_POSITION.write() = Some((
                    translation.to_string(),
                    book.to_string(),
                    response.book.common_name.clone(),
                    chapter,
                ));
            }

            Ok(response)
        }
        Err(e) => {
            // Only clear loading if still the latest request
            if *LATEST_REQUESTED_CHAPTER.read() == chapter_key {
                *LOADING_CHAPTER.write() = false;
            }
            Err(e)
        }
    }
}

// ============================================================================
// NIP-84 Highlight Functions
// ============================================================================

/// Create a highlight event for verse(s)
pub async fn create_highlight(
    verse_text: &str,
    reference: &str,
    api_url: &str,
    comment: Option<&str>,
) -> StdResult<EventId, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached - please sign in".to_string());
    }

    // Build event using validated SDK patterns
    // NIP-84: r-tag with "source" attribute marks this as the highlighted source URL
    let mut builder = EventBuilder::new(Kind::Custom(KIND_HIGHLIGHT), verse_text)
        .tag(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::R)),
            vec![api_url, "source"]
        ))
        .tag(Tag::custom(TagKind::Custom("context".into()), vec![reference]))
        .tag(Tag::hashtag("bible"));

    if let Some(c) = comment {
        builder = builder.tag(Tag::custom(TagKind::Custom("comment".into()), vec![c]));
    }

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish highlight: {}", e))?;

    let event_id = output.id();
    log::info!("Bible highlight published: {}", event_id.to_hex());

    // Refresh user highlights
    let pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    let _ = fetch_user_highlights(&pubkey).await;

    Ok(*event_id)
}

/// Fetch user's Bible highlights
pub async fn fetch_user_highlights(pubkey: &PublicKey) -> StdResult<Vec<BibleHighlight>, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    *LOADING_HIGHLIGHTS.write() = true;

    let filter = Filter::new()
        .author(*pubkey)
        .kind(Kind::Custom(KIND_HIGHLIGHT))
        .hashtag("bible")
        .limit(200);

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let highlights: Vec<BibleHighlight> = events
                .iter()
                .filter_map(parse_highlight)
                .collect();

            *USER_HIGHLIGHTS.write() = highlights.clone();
            *LOADING_HIGHLIGHTS.write() = false;

            log::info!("Fetched {} user Bible highlights", highlights.len());
            Ok(highlights)
        }
        Err(e) => {
            *LOADING_HIGHLIGHTS.write() = false;
            Err(format!("Failed to fetch highlights: {}", e))
        }
    }
}

/// Fetch all highlights for a specific chapter (from all users)
pub async fn fetch_chapter_highlights(api_url: &str) -> StdResult<Vec<BibleHighlight>, String> {
    // Set this as the latest requested URL for race prevention
    *LATEST_REQUESTED_HIGHLIGHT_URL.write() = api_url.to_string();

    // Clear stale highlights immediately to prevent showing old data during fetch
    CURRENT_CHAPTER_HIGHLIGHTS.write().clear();

    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_HIGHLIGHT))
        .reference(api_url)
        .limit(500);

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let highlights: Vec<BibleHighlight> = events
                .iter()
                .filter_map(parse_highlight)
                .collect();

            // Only update global state if still the latest request
            if *LATEST_REQUESTED_HIGHLIGHT_URL.read() == api_url {
                *CURRENT_CHAPTER_HIGHLIGHTS.write() = highlights.clone();
            }

            log::info!("Fetched {} chapter highlights for {}", highlights.len(), api_url);
            Ok(highlights)
        }
        Err(e) => {
            Err(format!("Failed to fetch chapter highlights: {}", e))
        }
    }
}

/// Parse a Kind 9802 event into a BibleHighlight
fn parse_highlight(event: &NostrEvent) -> Option<BibleHighlight> {
    // Must be Kind 9802
    if event.kind.as_u16() != KIND_HIGHLIGHT {
        return None;
    }

    // Verify event signature and ID before processing
    // (nostr-sdk's client.fetch_events() does NOT automatically verify events)
    if event.verify().is_err() {
        log::warn!("Invalid event signature for highlight {}", event.id);
        return None;
    }

    let tags = &event.tags;

    // Extract context tag - REQUIRED, non-empty (nostr-sdk pattern: early validation)
    let reference = tags.iter()
        .find_map(|tag| {
            let slice = tag.as_slice();
            if slice.first().map(|s| s.as_str()) == Some("context") {
                slice.get(1).and_then(|s| {
                    let trimmed = s.trim();
                    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
                })
            } else {
                None
            }
        })?; // Return None if missing or empty

    // Extract comment tag (optional)
    let comment = tags.iter()
        .find_map(|tag| {
            let slice = tag.as_slice();
            if slice.first().map(|s| s.as_str()) == Some("comment") {
                slice.get(1).map(|s| s.to_string())
            } else {
                None
            }
        });

    // Extract r tag (API URL) - REQUIRED, non-empty (nostr-sdk pattern: early validation)
    let api_url = tags.iter()
        .find_map(|tag| {
            let slice = tag.as_slice();
            if slice.first().map(|s| s.as_str()) == Some("r") {
                slice.get(1).and_then(|s| {
                    let trimmed = s.trim();
                    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
                })
            } else {
                None
            }
        })?; // Return None if missing or empty

    // Must have bible hashtag
    let has_bible_tag = tags.iter().any(|tag| {
        let slice = tag.as_slice();
        slice.first().map(|s| s.as_str()) == Some("t") &&
        slice.get(1).map(|s| s.as_str()) == Some("bible")
    });

    if !has_bible_tag {
        return None;
    }

    Some(BibleHighlight {
        verse_text: event.content.clone(),
        reference,
        comment,
        api_url,
        pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs(),
        event: event.clone(),
    })
}

// ============================================================================
// Highlight Query Functions
// ============================================================================

/// Extract verse number(s) from a reference string like "John 3:16 (BSB)" or "John 3:16-18 (BSB)".
/// Returns (start_verse, end_verse) where start == end for single verses.
fn extract_verses_from_reference(reference: &str) -> Option<(u32, u32)> {
    // Find the colon, then extract the verse part before the space/parenthesis
    let after_colon = reference.split(':').nth(1)?;
    let verse_part = after_colon
        .split([' ', '('])
        .next()?
        .trim();

    if let Some((start, end)) = verse_part.split_once('-') {
        Some((start.trim().parse().ok()?, end.trim().parse().ok()?))
    } else {
        let v = verse_part.parse().ok()?;
        Some((v, v))
    }
}

/// Check if a reference matches a specific verse number.
/// Handles both single verses ("John 3:16") and ranges ("John 3:16-18").
fn verse_matches_reference(reference: &str, target_verse: u32) -> bool {
    if let Some((start, end)) = extract_verses_from_reference(reference) {
        target_verse >= start && target_verse <= end
    } else {
        false
    }
}

/// Check if a verse is highlighted by the current user
pub fn is_verse_highlighted(translation: &str, book: &str, chapter: u32, verse: u32) -> bool {
    let api_url = get_chapter_api_url(translation, book, chapter);
    let user_highlights = USER_HIGHLIGHTS.read();

    user_highlights.iter().any(|h| {
        h.api_url == api_url && verse_matches_reference(&h.reference, verse)
    })
}

/// Get highlight count for a verse from all users
pub fn get_verse_highlight_count(verse: u32) -> usize {
    let highlights = CURRENT_CHAPTER_HIGHLIGHTS.read();
    highlights.iter().filter(|h| {
        verse_matches_reference(&h.reference, verse)
    }).count()
}

/// Get highlight stats for the current chapter
pub fn get_chapter_highlight_stats() -> ChapterHighlightStats {
    let highlights = CURRENT_CHAPTER_HIGHLIGHTS.read();
    let mut stats = ChapterHighlightStats::default();

    for h in highlights.iter() {
        // Count each highlight event once
        stats.total += 1;

        // Extract verse numbers and count each verse in the range
        if let Some((start, end)) = extract_verses_from_reference(&h.reference) {
            for verse in start..=end {
                *stats.verse_counts.entry(verse).or_insert(0) += 1;
            }
        }
    }

    stats
}

/// Get user's highlight for a specific verse (if any)
pub fn get_user_highlight_for_verse(translation: &str, book: &str, chapter: u32, verse: u32) -> Option<BibleHighlight> {
    let api_url = get_chapter_api_url(translation, book, chapter);
    let user_highlights = USER_HIGHLIGHTS.read();

    user_highlights.iter().find(|h| {
        h.api_url == api_url && verse_matches_reference(&h.reference, verse)
    }).cloned()
}

/// Get all highlights for a specific verse (from all users)
pub fn get_highlights_for_verse(verse: u32) -> Vec<BibleHighlight> {
    let highlights = CURRENT_CHAPTER_HIGHLIGHTS.read();
    highlights.iter().filter(|h| {
        verse_matches_reference(&h.reference, verse)
    }).cloned().collect()
}

// ============================================================================
// Search Functions (Client-Side)
// ============================================================================

/// Search result for Bible search
#[derive(Clone, Debug, PartialEq)]
pub struct BibleSearchResult {
    pub translation: String,
    pub book: String,
    pub book_name: String,
    pub chapter: u32,
    pub verse: u32,
    pub text: String,
    pub api_url: String,
}

/// Search cached chapters for verses containing query
pub fn search_cached_verses(query: &str, limit: usize) -> Vec<BibleSearchResult> {
    // Trim whitespace to prevent whitespace-only queries passing length check
    let query = query.trim();
    // Use chars().count() for Unicode-safe length check (not bytes)
    if query.chars().count() < 3 {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let cache = CHAPTER_CACHE.read();
    let mut results = Vec::new();

    for (key, cached) in cache.iter() {
        let parts: Vec<&str> = key.split(':').collect();
        if parts.len() != 3 {
            continue;
        }

        let translation = parts[0];
        let book = parts[1];
        let chapter: u32 = match parts[2].parse() {
            Ok(c) => c,
            Err(_) => continue,
        };

        let book_name = &cached.response.book.common_name;

        for content in &cached.response.chapter.content {
            if let ChapterContent::Verse { number, content: verse_content } = content {
                let text = verse_to_plain_text(verse_content);
                if text.to_lowercase().contains(&query_lower) {
                    results.push(BibleSearchResult {
                        translation: translation.to_string(),
                        book: book.to_string(),
                        book_name: book_name.clone(),
                        chapter,
                        verse: *number,
                        text,
                        api_url: cached.api_url.clone(),
                    });

                    if results.len() >= limit {
                        return results;
                    }
                }
            }
        }
    }

    results
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Get translation by ID
pub fn get_translation(id: &str) -> Option<Translation> {
    TRANSLATIONS.read().iter().find(|t| t.id == id).cloned()
}

/// Get book by ID for current translation
pub fn get_book(book_id: &str) -> Option<Book> {
    CURRENT_BOOKS.read().iter().find(|b| b.id == book_id).cloned()
}

/// Split books into Old and New Testament
pub fn split_books_by_testament(books: &[Book]) -> (Vec<Book>, Vec<Book>) {
    let ot_books: Vec<&str> = vec![
        "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA",
        "1KI", "2KI", "1CH", "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO",
        "ECC", "SNG", "ISA", "JER", "LAM", "EZK", "DAN", "HOS", "JOL", "AMO",
        "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    ];

    let old_testament: Vec<Book> = books.iter()
        .filter(|b| ot_books.contains(&b.id.as_str()))
        .cloned()
        .collect();

    let new_testament: Vec<Book> = books.iter()
        .filter(|b| !ot_books.contains(&b.id.as_str()) && b.is_apocryphal != Some(true))
        .cloned()
        .collect();

    (old_testament, new_testament)
}

/// Format chapter navigation URL
pub fn format_bible_url(translation: &str, book: &str, chapter: u32) -> String {
    format!("/bible/{}/{}/{}", translation, book, chapter)
}

/// Clear all store state
pub fn clear_store() {
    // Data caches
    CHAPTER_CACHE.write().clear();
    *USER_HIGHLIGHTS.write() = Vec::new();
    *CURRENT_CHAPTER_HIGHLIGHTS.write() = Vec::new();
    *CURRENT_BOOKS.write() = Vec::new();
    *LAST_POSITION.write() = None;

    // Translation state
    *TRANSLATIONS.write() = Vec::new();
    *ENGLISH_TRANSLATIONS.write() = Vec::new();
    *CURRENT_TRANSLATION.write() = DEFAULT_TRANSLATION.to_string();

    // Loading flags
    *LOADING_TRANSLATIONS.write() = false;
    *LOADING_BOOKS.write() = false;
    *LOADING_CHAPTER.write() = false;
    *LOADING_HIGHLIGHTS.write() = false;

    // Race guard signals
    *LATEST_REQUESTED_TRANSLATION.write() = String::new();
    *LATEST_REQUESTED_CHAPTER.write() = String::new();
    *LATEST_REQUESTED_HIGHLIGHT_URL.write() = String::new();

    // Initialization flag (must be last)
    *BIBLE_STORE_INITIALIZED.write() = false;
}
