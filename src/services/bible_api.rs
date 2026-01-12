use serde::{Deserialize, Serialize};
use gloo_net::http::Request;
use gloo_timers::callback::Timeout;

/// HelloAO Bible API base URL
const BIBLE_API_BASE: &str = "https://bible.helloao.org/api";

/// API request timeout in milliseconds (10 seconds)
const API_TIMEOUT_MS: u32 = 10_000;

// =============================================================================
// Translation Types
// =============================================================================

/// A Bible translation available in the API
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub id: String,
    pub name: String,
    pub english_name: String,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub license_url: Option<String>,
    pub short_name: String,
    pub language: String,
    #[serde(default)]
    pub language_name: Option<String>,
    #[serde(default)]
    pub language_english_name: Option<String>,
    pub text_direction: String, // "ltr" or "rtl"
    pub available_formats: Vec<String>,
    pub list_of_books_api_link: String,
    pub number_of_books: u32,
    pub total_number_of_chapters: u32,
    pub total_number_of_verses: u32,
    #[serde(default)]
    pub number_of_apocryphal_books: Option<u32>,
}

/// Response wrapper for available translations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableTranslationsResponse {
    pub translations: Vec<Translation>,
}

// =============================================================================
// Book Types
// =============================================================================

/// A book within a translation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub id: String,
    pub name: String,
    pub common_name: String,
    #[serde(default)]
    pub title: Option<String>,
    pub order: u32,
    pub number_of_chapters: u32,
    pub first_chapter_number: u32,
    pub first_chapter_api_link: String,
    pub last_chapter_number: u32,
    pub last_chapter_api_link: String,
    pub total_number_of_verses: u32,
    #[serde(default)]
    pub is_apocryphal: Option<bool>,
}

/// Response wrapper for books in a translation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationBooksResponse {
    pub translation: Translation,
    pub books: Vec<Book>,
}

// =============================================================================
// Chapter Types
// =============================================================================

/// Full chapter response from the API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterResponse {
    pub translation: Translation,
    pub book: Book,
    pub this_chapter_link: String,
    #[serde(default)]
    pub this_chapter_audio_links: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub next_chapter_api_link: Option<String>,
    #[serde(default)]
    pub previous_chapter_api_link: Option<String>,
    pub number_of_verses: u32,
    pub chapter: ChapterData,
}

/// Chapter data containing verses and footnotes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterData {
    pub number: u32,
    pub content: Vec<ChapterContent>,
    #[serde(default)]
    pub footnotes: Vec<ChapterFootnote>,
}

/// Union type for chapter content items
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChapterContent {
    Heading {
        content: Vec<String>,
    },
    LineBreak,
    Verse {
        number: u32,
        content: Vec<VerseContent>,
    },
    HebrewSubtitle {
        content: Vec<VerseContent>,
    },
}

/// Verse content can be plain text, formatted text, or special elements.
///
/// # Variant Order (Important for `#[serde(untagged)]`)
///
/// This enum uses `#[serde(untagged)]` deserialization, which means variant order matters!
/// Serde will try each variant in declaration order until one successfully deserializes.
///
/// The variants are ordered to ensure correct matching:
/// 1. `Plain(String)` - Only matches JSON strings, never objects
/// 2. `Formatted` - Matches objects with required `text` field (+ optional `poem`, `words_of_jesus`)
/// 3. `FootnoteRef` - Matches objects with required `note_id` field (u32)
/// 4. `InlineHeading` - Matches objects with required `heading` field (String)
/// 5. `InlineLineBreak` - Matches objects with required `line_break` field (bool)
///
/// Each struct variant has distinct required fields with different types, so matching
/// should be unambiguous. The HelloAO Bible API sends objects with distinct shapes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum VerseContent {
    /// Plain text - only matches JSON strings
    Plain(String),
    /// Formatted text with optional styling - matches objects with `text` field
    Formatted(FormattedText),
    /// Footnote reference - matches objects with `note_id` field
    FootnoteRef(FootnoteReference),
    /// Inline heading - matches objects with `heading` field
    InlineHeading(InlineHeadingContent),
    /// Inline line break - matches objects with `line_break` field
    InlineLineBreak(InlineLineBreakContent),
}

/// Formatted text with optional poem indentation or Words of Jesus marking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FormattedText {
    pub text: String,
    #[serde(default)]
    pub poem: Option<u8>,
    #[serde(default)]
    pub words_of_jesus: Option<bool>,
}

/// Reference to a footnote
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FootnoteReference {
    pub note_id: u32,
}

/// Inline heading within a verse
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InlineHeadingContent {
    pub heading: String,
}

/// Inline line break within a verse
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InlineLineBreakContent {
    pub line_break: bool,
}

/// Footnote information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChapterFootnote {
    pub note_id: u32,
    pub text: String,
    #[serde(default)]
    pub reference: Option<FootnoteVerseReference>,
    #[serde(default)]
    pub caller: Option<String>,
}

/// Verse reference for a footnote
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FootnoteVerseReference {
    pub chapter: u32,
    pub verse: u32,
}

// =============================================================================
// API Functions
// =============================================================================

/// Fetch all available Bible translations
pub async fn fetch_translations() -> Result<Vec<Translation>, String> {
    // Create AbortController for proper request cancellation on timeout
    let controller = web_sys::AbortController::new()
        .map_err(|_| "Failed to create AbortController".to_string())?;
    let signal = controller.signal();

    // Set up abort timeout
    let controller_for_timeout = controller.clone();
    let _timeout = Timeout::new(API_TIMEOUT_MS, move || {
        controller_for_timeout.abort();
    });

    let url = format!("{}/available_translations.json", BIBLE_API_BASE);

    let response = Request::get(&url)
        .abort_signal(Some(&signal))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch translations: {}", e))?;

    if !response.ok() {
        return Err(format!("API error: {}", response.status()));
    }

    let data: AvailableTranslationsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse translations: {}", e))?;

    Ok(data.translations)
}

/// Fetch books for a specific translation
pub async fn fetch_books(translation: &str) -> Result<Vec<Book>, String> {
    // Create AbortController for proper request cancellation on timeout
    let controller = web_sys::AbortController::new()
        .map_err(|_| "Failed to create AbortController".to_string())?;
    let signal = controller.signal();

    // Set up abort timeout
    let controller_for_timeout = controller.clone();
    let _timeout = Timeout::new(API_TIMEOUT_MS, move || {
        controller_for_timeout.abort();
    });

    // Percent-encode path segments to prevent URL corruption
    let url = format!("{}/{}/books.json", BIBLE_API_BASE, urlencoding::encode(translation));

    let response = Request::get(&url)
        .abort_signal(Some(&signal))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch books: {}", e))?;

    if !response.ok() {
        return Err(format!("API error: {}", response.status()));
    }

    let data: TranslationBooksResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse books: {}", e))?;

    Ok(data.books)
}

/// Fetch a specific chapter
pub async fn fetch_chapter(translation: &str, book: &str, chapter: u32) -> Result<ChapterResponse, String> {
    // Create AbortController for proper request cancellation on timeout
    let controller = web_sys::AbortController::new()
        .map_err(|_| "Failed to create AbortController".to_string())?;
    let signal = controller.signal();

    // Set up abort timeout
    let controller_for_timeout = controller.clone();
    let _timeout = Timeout::new(API_TIMEOUT_MS, move || {
        controller_for_timeout.abort();
    });

    // Percent-encode path segments to prevent URL corruption
    let url = format!(
        "{}/{}/{}/{}.json",
        BIBLE_API_BASE,
        urlencoding::encode(translation),
        urlencoding::encode(book),
        chapter
    );

    let response = Request::get(&url)
        .abort_signal(Some(&signal))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch chapter: {}", e))?;

    if !response.ok() {
        return Err(format!("API error: {}", response.status()));
    }

    let data: ChapterResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse chapter: {}", e))?;

    Ok(data)
}

/// Get the API URL for a specific chapter (useful for NIP-84 r tag)
pub fn get_chapter_api_url(translation: &str, book: &str, chapter: u32) -> String {
    // Percent-encode path segments to prevent URL corruption
    format!(
        "{}/{}/{}/{}.json",
        BIBLE_API_BASE,
        urlencoding::encode(translation),
        urlencoding::encode(book),
        chapter
    )
}

/// Get the display reference for a verse (e.g., "John 3:16 (BSB)")
#[allow(dead_code)]
pub fn format_verse_reference(book_name: &str, chapter: u32, verse: u32, translation: &str) -> String {
    format!("{} {}:{} ({})", book_name, chapter, verse, translation)
}

/// Get the display reference for multiple verses (e.g., "John 3:16-18 (BSB)")
#[allow(dead_code)]
pub fn format_verse_range_reference(
    book_name: &str,
    chapter: u32,
    start_verse: u32,
    end_verse: u32,
    translation: &str,
) -> String {
    if start_verse == end_verse {
        format_verse_reference(book_name, chapter, start_verse, translation)
    } else {
        format!("{} {}:{}-{} ({})", book_name, chapter, start_verse, end_verse, translation)
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

#[allow(dead_code)]
impl VerseContent {
    /// Extract plain text from verse content
    pub fn as_text(&self) -> Option<&str> {
        match self {
            VerseContent::Plain(s) => Some(s),
            VerseContent::Formatted(f) => Some(&f.text),
            _ => None,
        }
    }

    /// Check if this content is Words of Jesus
    pub fn is_words_of_jesus(&self) -> bool {
        match self {
            VerseContent::Formatted(f) => f.words_of_jesus.unwrap_or(false),
            _ => false,
        }
    }

    /// Get poem indent level (if any)
    pub fn poem_level(&self) -> Option<u8> {
        match self {
            VerseContent::Formatted(f) => f.poem,
            _ => None,
        }
    }
}

#[allow(dead_code)]
impl ChapterContent {
    /// Get verse number if this is a verse
    pub fn verse_number(&self) -> Option<u32> {
        match self {
            ChapterContent::Verse { number, .. } => Some(*number),
            _ => None,
        }
    }

    /// Get verse content if this is a verse
    pub fn verse_content(&self) -> Option<&Vec<VerseContent>> {
        match self {
            ChapterContent::Verse { content, .. } => Some(content),
            _ => None,
        }
    }

    /// Check if this is a verse
    pub fn is_verse(&self) -> bool {
        matches!(self, ChapterContent::Verse { .. })
    }

    /// Check if this is a heading
    pub fn is_heading(&self) -> bool {
        matches!(self, ChapterContent::Heading { .. })
    }

    /// Get heading text if this is a heading
    pub fn heading_text(&self) -> Option<String> {
        match self {
            ChapterContent::Heading { content } => Some(content.join(" ")),
            _ => None,
        }
    }
}

/// Extract plain text from a verse's content
/// Uses fold to avoid intermediate Vec allocation and adds spaces between pieces
pub fn verse_to_plain_text(content: &[VerseContent]) -> String {
    content
        .iter()
        .filter_map(|c| c.as_text())
        .fold(String::new(), |mut acc, t| {
            if !acc.is_empty() {
                acc.push(' ');
            }
            acc.push_str(t);
            acc
        })
}

/// Filter to get only English translations
pub fn filter_english_translations(translations: &[Translation]) -> Vec<Translation> {
    translations
        .iter()
        .filter(|t| t.language == "eng")
        .cloned()
        .collect()
}

/// Common English translations to prioritize
pub const RECOMMENDED_TRANSLATIONS: &[&str] = &["BSB", "KJV", "ASV", "WEB", "NASB", "ESV", "NIV"];

/// Filter and sort translations, prioritizing recommended ones
pub fn sort_translations_by_priority(translations: Vec<Translation>) -> Vec<Translation> {
    let mut result = translations;
    result.sort_by(|a, b| {
        let a_priority = RECOMMENDED_TRANSLATIONS.iter().position(|&x| x == a.id);
        let b_priority = RECOMMENDED_TRANSLATIONS.iter().position(|&x| x == b.id);

        match (a_priority, b_priority) {
            (Some(ap), Some(bp)) => ap.cmp(&bp),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.english_name.cmp(&b.english_name),
        }
    });
    result
}
