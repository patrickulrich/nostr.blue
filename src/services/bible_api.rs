//! Bible API client for fetching translations, books, and chapters.
//!
//! Uses reqwest for cross-platform HTTP (works on web, desktop, and mobile).
use crate::platform::http::http_client;
use serde::{Deserialize, Serialize};

/// HelloAO Bible API base URL
const BIBLE_API_BASE: &str = "https://bible.helloao.org/api";
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
    pub text_direction: String,
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
/// 6. `Unknown` - Fallback for any unrecognized JSON value (prevents deserialization failures)
///
/// Each struct variant has distinct required fields with different types, so matching
/// should be unambiguous. The HelloAO Bible API sends objects with distinct shapes.
/// The Unknown variant ensures forward compatibility with new API response shapes.
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
    /// Unknown/unrecognized content - fallback to prevent deserialization failures
    Unknown(serde_json::Value),
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
/// Helper to perform HTTP GET request with timeout
/// Reduces code duplication across fetch_translations, fetch_books, fetch_chapter
async fn fetch_with_timeout(url: &str, error_context: &str) -> Result<reqwest::Response, String> {
    #[cfg(feature = "web")]
    let response = {
        use futures::FutureExt;
        let request = http_client()
            .map_err(|e| format!("HTTP client init failed: {}", e))?
            .get(url)
            .send()
            .fuse();
        let timeout = gloo_timers::future::TimeoutFuture::new(15_000).fuse();
        futures::pin_mut!(request, timeout);
        futures::select! {
            resp = request => resp,
            _ = timeout => return Err("Request timeout".to_string()),
        }
    };
    #[cfg(not(feature = "web"))]
    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(url)
        .send()
        .await;
    let response = response.map_err(|e| {
        if e.is_timeout() {
            "Request timeout".to_string()
        } else {
            format!("Failed to {}: {}", error_context, e)
        }
    })?;
    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }
    Ok(response)
}
/// Fetch all available Bible translations
pub async fn fetch_translations() -> Result<Vec<Translation>, String> {
    let url = format!("{}/available_translations.json", BIBLE_API_BASE);
    let response = fetch_with_timeout(&url, "fetch translations").await?;
    let data: AvailableTranslationsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse translations: {}", e))?;
    Ok(data.translations)
}
/// Fetch books for a specific translation
pub async fn fetch_books(translation: &str) -> Result<Vec<Book>, String> {
    let url = format!(
        "{}/{}/books.json",
        BIBLE_API_BASE,
        urlencoding::encode(translation),
    );
    let response = fetch_with_timeout(&url, "fetch books").await?;
    let data: TranslationBooksResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse books: {}", e))?;
    Ok(data.books)
}
/// Fetch a specific chapter
pub async fn fetch_chapter(
    translation: &str,
    book: &str,
    chapter: u32,
) -> Result<ChapterResponse, String> {
    let url = format!(
        "{}/{}/{}/{}.json",
        BIBLE_API_BASE,
        urlencoding::encode(translation),
        urlencoding::encode(book),
        chapter,
    );
    let response = fetch_with_timeout(&url, "fetch chapter").await?;
    let data: ChapterResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse chapter: {}", e))?;
    Ok(data)
}
/// Get the API URL for a specific chapter (useful for NIP-84 r tag)
pub fn get_chapter_api_url(translation: &str, book: &str, chapter: u32) -> String {
    format!(
        "{}/{}/{}/{}.json",
        BIBLE_API_BASE,
        urlencoding::encode(translation),
        urlencoding::encode(book),
        chapter,
    )
}
/// Get the display reference for a verse (e.g., "John 3:16 (BSB)")
#[allow(dead_code)]
pub fn format_verse_reference(
    book_name: &str,
    chapter: u32,
    verse: u32,
    translation: &str,
) -> String {
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
        format!(
            "{} {}:{}-{} ({})",
            book_name, chapter, start_verse, end_verse, translation,
        )
    }
}

/// Get the display reference for an arbitrary verse selection.
#[allow(dead_code)]
pub fn format_selected_verses_reference(
    book_name: &str,
    chapter: u32,
    verses: &[u32],
    translation: &str,
) -> String {
    let mut verses = verses.to_vec();
    verses.sort_unstable();
    verses.dedup();
    match verses.as_slice() {
        [] => format!("{} {} ({})", book_name, chapter, translation),
        [verse] => format_verse_reference(book_name, chapter, *verse, translation),
        _ => {
            let mut segments = Vec::new();
            let mut start = verses[0];
            let mut end = verses[0];
            for verse in verses.iter().copied().skip(1) {
                if verse == end + 1 {
                    end = verse;
                    continue;
                }
                if start == end {
                    segments.push(start.to_string());
                } else {
                    segments.push(format!("{}-{}", start, end));
                }
                start = verse;
                end = verse;
            }
            if start == end {
                segments.push(start.to_string());
            } else {
                segments.push(format!("{}-{}", start, end));
            }
            format!(
                "{} {}:{} ({})",
                book_name,
                chapter,
                segments.join(","),
                translation
            )
        }
    }
}
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

#[cfg(test)]
mod tests {
    use super::format_selected_verses_reference;

    #[test]
    fn formats_single_selected_verse() {
        assert_eq!(
            format_selected_verses_reference("John", 3, &[16], "BSB"),
            "John 3:16 (BSB)"
        );
    }

    #[test]
    fn formats_contiguous_selected_verses() {
        assert_eq!(
            format_selected_verses_reference("John", 3, &[16, 17, 18], "BSB"),
            "John 3:16-18 (BSB)"
        );
    }

    #[test]
    fn formats_non_contiguous_selected_verses() {
        assert_eq!(
            format_selected_verses_reference("John", 3, &[16, 18, 19, 21], "BSB"),
            "John 3:16,18-19,21 (BSB)"
        );
    }

    #[test]
    fn normalizes_unsorted_duplicate_selected_verses() {
        assert_eq!(
            format_selected_verses_reference("John", 3, &[19, 16, 18, 18, 17], "BSB"),
            "John 3:16-19 (BSB)"
        );
    }
}
