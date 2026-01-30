//! Bible Store
//! Handles Bible reading state, chapter caching, and NIP-84 highlights (Kind 9802)
//!
//! Uses HelloAO Bible API for content and Nostr for social highlighting features.
//! Highlight logic is delegated to the centralized nip84 module.
#![allow(dead_code)]
use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use crate::utils::nip84::{self, Highlight, HighlightSource};
pub use crate::services::bible_api::{
    fetch_books, fetch_chapter, fetch_translations, filter_english_translations,
    get_chapter_api_url, sort_translations_by_priority, verse_to_plain_text, Book,
    ChapterContent, ChapterResponse, Translation, VerseContent,
};
pub use crate::utils::nip84::Highlight as BibleHighlight;
type StdResult<T, E> = std::result::Result<T, E>;
/// Re-export KIND_HIGHLIGHT from nip84 for backwards compatibility
pub const KIND_HIGHLIGHT: u16 = nip84::KIND_HIGHLIGHT;
/// Cache sizes
const CHAPTER_CACHE_SIZE: usize = 100;
/// Default translation
pub const DEFAULT_TRANSLATION: &str = "BSB";
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
/// All available translations
pub static TRANSLATIONS: GlobalSignal<Vec<Translation>> = GlobalSignal::new(Vec::new);
/// English translations only (filtered for UI)
pub static ENGLISH_TRANSLATIONS: GlobalSignal<Vec<Translation>> = GlobalSignal::new(
    Vec::new,
);
/// Currently selected translation
pub static CURRENT_TRANSLATION: GlobalSignal<String> = GlobalSignal::new(|| {
    DEFAULT_TRANSLATION.to_string()
});
/// Books for current translation
pub static CURRENT_BOOKS: GlobalSignal<Vec<Book>> = GlobalSignal::new(Vec::new);
/// Chapter cache (keyed by "translation:book:chapter")
pub static CHAPTER_CACHE: GlobalSignal<LruCache<String, CachedChapter>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(CHAPTER_CACHE_SIZE).unwrap()));
/// User's own highlights
pub static USER_HIGHLIGHTS: GlobalSignal<Vec<BibleHighlight>> = GlobalSignal::new(
    Vec::new,
);
/// Highlights for current chapter (from all users)
pub static CURRENT_CHAPTER_HIGHLIGHTS: GlobalSignal<Vec<BibleHighlight>> = GlobalSignal::new(
    Vec::new,
);
/// Loading states
pub static LOADING_TRANSLATIONS: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_BOOKS: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_CHAPTER: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_HIGHLIGHTS: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Store initialization flag
pub static BIBLE_STORE_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Last viewed position (for "Continue Reading" feature)
/// Tuple: (translation, book_id, book_common_name, chapter)
pub static LAST_POSITION: GlobalSignal<Option<(String, String, String, u32)>> = GlobalSignal::new(||
None);
/// Latest requested translation for race guard (prevents stale data on fast navigation)
static LATEST_REQUESTED_TRANSLATION: GlobalSignal<String> = GlobalSignal::new(
    String::new,
);
/// Latest requested chapter key for race guard (prevents stale data on fast navigation)
static LATEST_REQUESTED_CHAPTER: GlobalSignal<String> = GlobalSignal::new(String::new);
/// Latest requested highlight URL for race guard (prevents stale data on fast navigation)
static LATEST_REQUESTED_HIGHLIGHT_URL: GlobalSignal<String> = GlobalSignal::new(
    String::new,
);
/// Generate cache key for a chapter
fn chapter_cache_key(translation: &str, book: &str, chapter: u32) -> String {
    format!("{}:{}:{}", translation, book, chapter)
}
/// Get a chapter from cache.
/// Uses write() + get() to properly update LRU access order.
pub fn get_cached_chapter(
    translation: &str,
    book: &str,
    chapter: u32,
) -> Option<CachedChapter> {
    let key = chapter_cache_key(translation, book, chapter);
    CHAPTER_CACHE.write().get(&key).cloned()
}
/// Cache a chapter
pub fn cache_chapter(
    translation: &str,
    book: &str,
    chapter: u32,
    response: ChapterResponse,
) {
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
/// Initialize the Bible store - fetch translations and default books
/// Only marks as initialized after ALL setup completes successfully
pub async fn initialize() -> StdResult<(), String> {
    if *BIBLE_STORE_INITIALIZED.read() {
        return Ok(());
    }
    *LOADING_TRANSLATIONS.write() = true;
    let result = async {
        let translations = fetch_translations().await?;
        let english = filter_english_translations(&translations);
        let sorted_english = sort_translations_by_priority(english);
        *ENGLISH_TRANSLATIONS.write() = sorted_english;
        *TRANSLATIONS.write() = translations;
        load_books(DEFAULT_TRANSLATION).await?;
        Ok(())
    }
        .await;
    *LOADING_TRANSLATIONS.write() = false;
    if result.is_ok() {
        *BIBLE_STORE_INITIALIZED.write() = true;
    }
    result
}
/// Load books for a translation
pub async fn load_books(translation: &str) -> StdResult<Vec<Book>, String> {
    *LATEST_REQUESTED_TRANSLATION.write() = translation.to_string();
    *LOADING_BOOKS.write() = true;
    match fetch_books(translation).await {
        Ok(books) => {
            if *LATEST_REQUESTED_TRANSLATION.read() == translation {
                *CURRENT_BOOKS.write() = books.clone();
                *CURRENT_TRANSLATION.write() = translation.to_string();
                *LOADING_BOOKS.write() = false;
            }
            Ok(books)
        }
        Err(e) => {
            if *LATEST_REQUESTED_TRANSLATION.read() == translation {
                *LOADING_BOOKS.write() = false;
            }
            Err(e)
        }
    }
}
/// Load a chapter (with caching)
pub async fn load_chapter(
    translation: &str,
    book: &str,
    chapter: u32,
) -> StdResult<ChapterResponse, String> {
    if let Some(cached) = get_cached_chapter(translation, book, chapter) {
        *LAST_POSITION.write() = Some((
            translation.to_string(),
            book.to_string(),
            cached.response.book.common_name.clone(),
            chapter,
        ));
        return Ok(cached.response);
    }
    let chapter_key = chapter_cache_key(translation, book, chapter);
    *LATEST_REQUESTED_CHAPTER.write() = chapter_key.clone();
    *LOADING_CHAPTER.write() = true;
    match fetch_chapter(translation, book, chapter).await {
        Ok(response) => {
            cache_chapter(translation, book, chapter, response.clone());
            if *LATEST_REQUESTED_CHAPTER.read() == chapter_key {
                *LOADING_CHAPTER.write() = false;
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
            if *LATEST_REQUESTED_CHAPTER.read() == chapter_key {
                *LOADING_CHAPTER.write() = false;
            }
            Err(e)
        }
    }
}
/// Create a highlight event for verse(s)
/// Delegates to centralized nip84::create_highlight with Bible-specific defaults.
pub async fn create_highlight(
    verse_text: &str,
    reference: &str,
    translation: &str,
    book: &str,
    chapter: u32,
    comment: Option<&str>,
) -> StdResult<EventId, String> {
    let source_url = get_nostr_blue_bible_url(translation, book, chapter);
    let event_id = nip84::create_highlight(
            verse_text,
            HighlightSource::Url(source_url),
            Some(reference),
            comment,
            vec!["bible"],
        )
        .await?;
    log::info!("Bible highlight published: {}", event_id.to_hex());
    if let Ok(pubkey) = crate::stores::nostr_client::get_cached_pubkey() {
        if let Err(e) = fetch_user_highlights(&pubkey).await {
            log::warn!("Failed to refresh highlights after publish: {}", e);
        }
    } else {
        log::warn!("Could not get pubkey to refresh highlights");
    }
    Ok(event_id)
}
/// Fetch user's Bible highlights
/// Uses centralized nip84 module and filters for Bible-specific highlights.
pub async fn fetch_user_highlights(
    pubkey: &PublicKey,
) -> StdResult<Vec<BibleHighlight>, String> {
    *LOADING_HIGHLIGHTS.write() = true;
    match nip84::fetch_user_highlights(pubkey).await {
        Ok(all_highlights) => {
            let highlights = nip84::filter_bible_highlights(all_highlights);
            *USER_HIGHLIGHTS.write() = highlights.clone();
            *LOADING_HIGHLIGHTS.write() = false;
            log::info!("Fetched {} user Bible highlights", highlights.len());
            Ok(highlights)
        }
        Err(e) => {
            *LOADING_HIGHLIGHTS.write() = false;
            Err(e)
        }
    }
}
/// Fetch all highlights for a specific chapter (from all users)
/// Uses centralized nip84 module for URL-based highlight fetching.
pub async fn fetch_chapter_highlights(
    translation: &str,
    book: &str,
    chapter: u32,
) -> StdResult<Vec<BibleHighlight>, String> {
    let bible_url = get_nostr_blue_bible_url(translation, book, chapter);
    *LATEST_REQUESTED_HIGHLIGHT_URL.write() = bible_url.clone();
    CURRENT_CHAPTER_HIGHLIGHTS.write().clear();
    match nip84::fetch_highlights_by_url(&bible_url).await {
        Ok(highlights) => {
            if *LATEST_REQUESTED_HIGHLIGHT_URL.read() == bible_url {
                *CURRENT_CHAPTER_HIGHLIGHTS.write() = highlights.clone();
            }
            log::info!(
                "Fetched {} chapter highlights for {}", highlights.len(), bible_url
            );
            Ok(highlights)
        }
        Err(e) => Err(format!("Failed to fetch chapter highlights: {}", e)),
    }
}
/// Extract verse number(s) from a reference string like "John 3:16 (BSB)" or "John 3:16-18 (BSB)".
/// Returns (start_verse, end_verse) where start == end for single verses.
fn extract_verses_from_reference(reference: &str) -> Option<(u32, u32)> {
    let after_colon = reference.split(':').nth(1)?;
    let verse_part = after_colon.split([' ', '(']).next()?.trim();
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
/// Helper to check if a highlight matches a Bible URL
fn highlight_matches_url(highlight: &Highlight, url: &str) -> bool {
    highlight.source.as_url().map(|u| u == url).unwrap_or(false)
}
/// Helper to get the reference string from a highlight (stored in context field)
fn get_highlight_reference(highlight: &Highlight) -> Option<&str> {
    highlight.context.as_deref()
}
/// Check if a verse is highlighted by the current user
pub fn is_verse_highlighted(
    translation: &str,
    book: &str,
    chapter: u32,
    verse: u32,
) -> bool {
    let bible_url = get_nostr_blue_bible_url(translation, book, chapter);
    let user_highlights = USER_HIGHLIGHTS.read();
    user_highlights
        .iter()
        .any(|h| {
            highlight_matches_url(h, &bible_url)
                && get_highlight_reference(h)
                    .map(|r| verse_matches_reference(r, verse))
                    .unwrap_or(false)
        })
}
/// Get highlight count for a verse from all users
pub fn get_verse_highlight_count(verse: u32) -> usize {
    let highlights = CURRENT_CHAPTER_HIGHLIGHTS.read();
    highlights
        .iter()
        .filter(|h| {
            get_highlight_reference(h)
                .map(|r| verse_matches_reference(r, verse))
                .unwrap_or(false)
        })
        .count()
}
/// Get highlight stats for the current chapter
pub fn get_chapter_highlight_stats() -> ChapterHighlightStats {
    let highlights = CURRENT_CHAPTER_HIGHLIGHTS.read();
    let mut stats = ChapterHighlightStats::default();
    for h in highlights.iter() {
        stats.total += 1;
        if let Some(reference) = get_highlight_reference(h) {
            if let Some((start, end)) = extract_verses_from_reference(reference) {
                for verse in start..=end {
                    *stats.verse_counts.entry(verse).or_insert(0) += 1;
                }
            }
        }
    }
    stats
}
/// Get user's highlight for a specific verse (if any)
pub fn get_user_highlight_for_verse(
    translation: &str,
    book: &str,
    chapter: u32,
    verse: u32,
) -> Option<BibleHighlight> {
    let bible_url = get_nostr_blue_bible_url(translation, book, chapter);
    let user_highlights = USER_HIGHLIGHTS.read();
    user_highlights
        .iter()
        .find(|h| {
            highlight_matches_url(h, &bible_url)
                && get_highlight_reference(h)
                    .map(|r| verse_matches_reference(r, verse))
                    .unwrap_or(false)
        })
        .cloned()
}
/// Get all highlights for a specific verse (from all users)
pub fn get_highlights_for_verse(verse: u32) -> Vec<BibleHighlight> {
    let highlights = CURRENT_CHAPTER_HIGHLIGHTS.read();
    highlights
        .iter()
        .filter(|h| {
            get_highlight_reference(h)
                .map(|r| verse_matches_reference(r, verse))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}
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
    let query = query.trim();
    if query.chars().count() < 3 {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    let cache = CHAPTER_CACHE.read();
    let mut results = Vec::new();
    for (key, cached) in cache.iter() {
        let parts: Vec<&str> = key.rsplitn(3, ':').collect();
        if parts.len() != 3 {
            continue;
        }
        let translation = parts[2];
        let book = parts[1];
        let chapter: u32 = match parts[0].parse() {
            Ok(c) => c,
            Err(_) => continue,
        };
        let book_name = &cached.response.book.common_name;
        for content in &cached.response.chapter.content {
            if let ChapterContent::Verse { number, content: verse_content } = content {
                let text = verse_to_plain_text(verse_content);
                if text.to_lowercase().contains(&query_lower) {
                    results
                        .push(BibleSearchResult {
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
        "GEN",
        "EXO",
        "LEV",
        "NUM",
        "DEU",
        "JOS",
        "JDG",
        "RUT",
        "1SA",
        "2SA",
        "1KI",
        "2KI",
        "1CH",
        "2CH",
        "EZR",
        "NEH",
        "EST",
        "JOB",
        "PSA",
        "PRO",
        "ECC",
        "SNG",
        "ISA",
        "JER",
        "LAM",
        "EZK",
        "DAN",
        "HOS",
        "JOL",
        "AMO",
        "OBA",
        "JON",
        "MIC",
        "NAM",
        "HAB",
        "ZEP",
        "HAG",
        "ZEC",
        "MAL",
    ];
    let old_testament: Vec<Book> = books
        .iter()
        .filter(|b| ot_books.contains(&b.id.as_str()))
        .cloned()
        .collect();
    let new_testament: Vec<Book> = books
        .iter()
        .filter(|b| !ot_books.contains(&b.id.as_str()) && b.is_apocryphal != Some(true))
        .cloned()
        .collect();
    (old_testament, new_testament)
}
/// Format chapter navigation URL (relative path for internal routing)
pub fn format_bible_url(translation: &str, book: &str, chapter: u32) -> String {
    format!("/bible/{}/{}/{}", translation, book, chapter)
}
/// Generate the canonical nostr.blue Bible URL for NIP-84 highlights.
/// This URL is used in r-tags to link back to the source content.
/// URL-encodes translation and book to handle spaces/special chars (e.g., "1 Samuel").
pub fn get_nostr_blue_bible_url(translation: &str, book: &str, chapter: u32) -> String {
    format!(
        "https://nostr.blue/bible/{}/{}/{}",
        urlencoding::encode(translation),
        urlencoding::encode(book),
        chapter,
    )
}
/// Clear all store state
pub fn clear_store() {
    CHAPTER_CACHE.write().clear();
    *USER_HIGHLIGHTS.write() = Vec::new();
    *CURRENT_CHAPTER_HIGHLIGHTS.write() = Vec::new();
    *CURRENT_BOOKS.write() = Vec::new();
    *LAST_POSITION.write() = None;
    *TRANSLATIONS.write() = Vec::new();
    *ENGLISH_TRANSLATIONS.write() = Vec::new();
    *CURRENT_TRANSLATION.write() = DEFAULT_TRANSLATION.to_string();
    *LOADING_TRANSLATIONS.write() = false;
    *LOADING_BOOKS.write() = false;
    *LOADING_CHAPTER.write() = false;
    *LOADING_HIGHLIGHTS.write() = false;
    *LATEST_REQUESTED_TRANSLATION.write() = String::new();
    *LATEST_REQUESTED_CHAPTER.write() = String::new();
    *LATEST_REQUESTED_HIGHLIGHT_URL.write() = String::new();
    *BIBLE_STORE_INITIALIZED.write() = false;
}
