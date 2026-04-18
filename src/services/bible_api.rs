//! Bible API client for fetching translations, books, and chapters.
//!
//! Uses reqwest for cross-platform HTTP (works on web, desktop, and mobile).
use crate::platform::http::http_client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    #[serde(default)]
    pub complete_translation_api_link: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationComplete {
    pub translation: Translation,
    pub books: Vec<CompleteBook>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompleteBook {
    pub id: String,
    pub name: String,
    pub common_name: String,
    #[serde(default)]
    pub title: Option<String>,
    pub order: u32,
    pub number_of_chapters: u32,
    pub total_number_of_verses: u32,
    #[serde(default)]
    pub is_apocryphal: Option<bool>,
    pub chapters: Vec<CompleteChapterData>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompleteChapterData {
    pub number_of_verses: u32,
    #[serde(default)]
    pub this_chapter_audio_links: Option<std::collections::HashMap<String, String>>,
    pub chapter: ChapterData,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commentary {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub license_url: Option<String>,
    #[serde(default)]
    pub license_notes: Option<String>,
    pub english_name: String,
    pub language: String,
    pub text_direction: String,
    #[serde(default)]
    pub number_of_books: u32,
    #[serde(default)]
    pub total_number_of_chapters: u32,
    #[serde(default)]
    pub total_number_of_verses: u32,
    #[serde(default)]
    pub total_number_of_profiles: Option<u32>,
    #[serde(default)]
    pub list_of_profiles_api_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct AvailableCommentariesResponse {
    pub commentaries: Vec<Commentary>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentaryBook {
    pub id: String,
    #[serde(default)]
    pub commentary_id: Option<String>,
    pub name: String,
    pub common_name: String,
    #[serde(default)]
    pub introduction: Option<String>,
    pub order: u32,
    #[serde(default)]
    pub number_of_chapters: u32,
    #[serde(default)]
    pub total_number_of_verses: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentaryChapterResponse {
    #[serde(default)]
    pub commentary: Commentary,
    pub book: CommentaryBook,
    #[serde(default)]
    pub next_chapter_api_link: Option<String>,
    #[serde(default)]
    pub previous_chapter_api_link: Option<String>,
    #[serde(default)]
    pub number_of_verses: u32,
    pub chapter: CommentaryChapterData,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentaryChapterData {
    pub number: u32,
    #[serde(default)]
    pub introduction: Option<String>,
    pub content: Vec<CommentaryVerse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentaryVerse {
    pub number: u32,
    pub content: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossRefChapterResponse {
    #[serde(default)]
    pub number_of_references: Option<u32>,
    pub chapter: CrossRefChapterData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossRefChapterData {
    pub number: u32,
    pub content: Vec<CrossRefVerse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossRefVerse {
    pub verse: u32,
    pub references: Vec<CrossRefReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossRefReference {
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    #[serde(default)]
    pub end_verse: Option<u32>,
    #[serde(default)]
    pub score: Option<i32>,
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
pub async fn fetch_complete_translation(
    translation: &str,
) -> Result<TranslationComplete, String> {
    let url = format!(
        "{}/{}/complete.json",
        BIBLE_API_BASE,
        urlencoding::encode(translation),
    );
    let response = fetch_with_timeout(&url, "fetch complete translation").await?;
    let data: TranslationComplete = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse complete translation: {}", e))?;
    Ok(data)
}
#[allow(dead_code)]
pub async fn fetch_commentaries() -> Result<Vec<Commentary>, String> {
    let url = format!("{}/available_commentaries.json", BIBLE_API_BASE);
    let response = fetch_with_timeout(&url, "fetch commentaries").await?;
    let data: AvailableCommentariesResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse commentaries: {}", e))?;
    Ok(data.commentaries)
}
pub async fn fetch_commentary_chapter(
    commentary: &str,
    book: &str,
    chapter: u32,
) -> Result<CommentaryChapterResponse, String> {
    let url = format!(
        "{}/c/{}/{}/{}.json",
        BIBLE_API_BASE,
        urlencoding::encode(commentary),
        urlencoding::encode(book),
        chapter,
    );
    let response = fetch_with_timeout(&url, "fetch commentary chapter").await?;
    let data: CommentaryChapterResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse commentary chapter: {}", e))?;
    Ok(data)
}
pub async fn fetch_cross_references(
    book: &str,
    chapter: u32,
) -> Result<CrossRefChapterResponse, String> {
    let url = format!(
        "{}/d/open-cross-ref/{}/{}.json",
        BIBLE_API_BASE,
        urlencoding::encode(book),
        chapter,
    );
    let response = fetch_with_timeout(&url, "fetch cross references").await?;
    let data: CrossRefChapterResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse cross references: {}", e))?;
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
/// Common English translations to prioritize
pub const RECOMMENDED_TRANSLATIONS: &[&str] = &["BSB", "eng_kjv", "eng_asv", "ENGWEBP"];
/// Filter and sort translations, prioritizing recommended ones
pub fn sort_translations_by_priority(
    translations: Vec<Translation>,
    favorites: &[String],
) -> Vec<Translation> {
    let mut result = translations;
    result.sort_by(|a, b| {
        let a_fav = favorites.iter().position(|f| f == &a.id);
        let b_fav = favorites.iter().position(|f| f == &b.id);
        let a_rec = RECOMMENDED_TRANSLATIONS.iter().position(|&x| x == a.id);
        let b_rec = RECOMMENDED_TRANSLATIONS.iter().position(|&x| x == b.id);
        let a_rank = a_fav
            .map(|p| (0, p))
            .or_else(|| a_rec.map(|p| (1, p)));
        let b_rank = b_fav
            .map(|p| (0, p))
            .or_else(|| b_rec.map(|p| (1, p)));
        match (a_rank, b_rank) {
            (Some(ar), Some(br)) => ar.cmp(&br),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => {
                let a_lang = a.language_english_name.as_deref().unwrap_or("");
                let b_lang = b.language_english_name.as_deref().unwrap_or("");
                a_lang.cmp(b_lang).then_with(|| a.english_name.cmp(&b.english_name))
            }
        }
    });
    result
}
pub fn group_by_language(
    translations: &[Translation],
) -> BTreeMap<String, Vec<Translation>> {
    let mut groups: BTreeMap<String, Vec<Translation>> = BTreeMap::new();
    for t in translations {
        let lang = t
            .language_english_name
            .as_deref()
            .unwrap_or("Unknown")
            .to_string();
        groups.entry(lang).or_default().push(t.clone());
    }
    groups
}

pub fn build_chapter_response_from_offline(
    complete: &TranslationComplete,
    book_id: &str,
    chapter_num: u32,
) -> Option<ChapterResponse> {
    let book = complete.books.iter().find(|b| b.id == book_id)?;
    let chapter_data = book
        .chapters
        .iter()
        .find(|c| c.chapter.number == chapter_num)?;

    let translation_id = &complete.translation.id;
    let (prev_link, next_link) = compute_adjacent_chapter_links(
        &complete.books,
        translation_id,
        book_id,
        chapter_num,
    );

    Some(ChapterResponse {
        translation: complete.translation.clone(),
        book: Book {
            id: book.id.clone(),
            name: book.name.clone(),
            common_name: book.common_name.clone(),
            title: book.title.clone(),
            order: book.order,
            number_of_chapters: book.number_of_chapters,
            first_chapter_number: 1,
            first_chapter_api_link: format!(
                "/api/{}/{}/1.json",
                translation_id, book.id
            ),
            last_chapter_number: book.number_of_chapters,
            last_chapter_api_link: format!(
                "/api/{}/{}/{}.json",
                translation_id, book.id, book.number_of_chapters
            ),
            total_number_of_verses: book.total_number_of_verses,
            is_apocryphal: book.is_apocryphal,
        },
        this_chapter_link: format!(
            "/api/{}/{}/{}.json",
            translation_id, book_id, chapter_num
        ),
        this_chapter_audio_links: chapter_data.this_chapter_audio_links.clone(),
        next_chapter_api_link: next_link,
        previous_chapter_api_link: prev_link,
        number_of_verses: chapter_data.number_of_verses,
        chapter: chapter_data.chapter.clone(),
    })
}

fn compute_adjacent_chapter_links(
    books: &[CompleteBook],
    translation_id: &str,
    book_id: &str,
    chapter_num: u32,
) -> (Option<String>, Option<String>) {
    let book_index = books.iter().position(|b| b.id == book_id);
    let book = match book_index {
        Some(idx) => &books[idx],
        None => return (None, None),
    };

    let prev_link = if chapter_num > 1 {
        Some(format!(
            "/api/{}/{}/{}.json",
            translation_id, book_id, chapter_num - 1
        ))
    } else if let Some(idx) = book_index {
        if idx > 0 {
            let prev_book = &books[idx - 1];
            Some(format!(
                "/api/{}/{}/{}.json",
                translation_id,
                prev_book.id,
                prev_book.number_of_chapters
            ))
        } else {
            None
        }
    } else {
        None
    };

    let next_link = if chapter_num < book.number_of_chapters {
        Some(format!(
            "/api/{}/{}/{}.json",
            translation_id, book_id, chapter_num + 1
        ))
    } else if let Some(idx) = book_index {
        if idx + 1 < books.len() {
            let next_book = &books[idx + 1];
            Some(format!(
                "/api/{}/{}/1.json",
                translation_id, next_book.id
            ))
        } else {
            None
        }
    } else {
        None
    };

    (prev_link, next_link)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn make_chapter_data(number: u32) -> CompleteChapterData {
        CompleteChapterData {
            number_of_verses: 25,
            this_chapter_audio_links: None,
            chapter: ChapterData {
                number,
                content: vec![],
                footnotes: vec![],
            },
        }
    }

    fn make_test_translation() -> TranslationComplete {
        TranslationComplete {
            translation: Translation {
                id: "BSB".to_string(),
                name: "Berean Standard Bible".to_string(),
                english_name: "Berean Standard Bible".to_string(),
                website: None,
                license_url: None,
                short_name: "BSB".to_string(),
                language: "en".to_string(),
                language_name: None,
                language_english_name: None,
                text_direction: "LTR".to_string(),
                available_formats: vec![],
                list_of_books_api_link: String::new(),
                number_of_books: 3,
                total_number_of_chapters: 153,
                total_number_of_verses: 4000,
                number_of_apocryphal_books: None,
                complete_translation_api_link: None,
                sha256: None,
            },
            books: vec![
                CompleteBook {
                    id: "GEN".to_string(),
                    name: "Genesis".to_string(),
                    common_name: "Genesis".to_string(),
                    title: None,
                    order: 1,
                    number_of_chapters: 50,
                    total_number_of_verses: 1533,
                    is_apocryphal: None,
                    chapters: (1..=50).map(make_chapter_data).collect(),
                },
                CompleteBook {
                    id: "EXO".to_string(),
                    name: "Exodus".to_string(),
                    common_name: "Exodus".to_string(),
                    title: None,
                    order: 2,
                    number_of_chapters: 40,
                    total_number_of_verses: 1213,
                    is_apocryphal: None,
                    chapters: (1..=40).map(make_chapter_data).collect(),
                },
                CompleteBook {
                    id: "LEV".to_string(),
                    name: "Leviticus".to_string(),
                    common_name: "Leviticus".to_string(),
                    title: None,
                    order: 3,
                    number_of_chapters: 27,
                    total_number_of_verses: 859,
                    is_apocryphal: None,
                    chapters: (1..=27).map(make_chapter_data).collect(),
                },
            ],
        }
    }

    #[test]
    fn adjacent_links_within_same_book() {
        let complete = make_test_translation();
        let resp = build_chapter_response_from_offline(&complete, "EXO", 5).unwrap();
        assert_eq!(
            resp.previous_chapter_api_link,
            Some("/api/BSB/EXO/4.json".to_string())
        );
        assert_eq!(
            resp.next_chapter_api_link,
            Some("/api/BSB/EXO/6.json".to_string())
        );
    }

    #[test]
    fn next_link_crosses_to_next_book() {
        let complete = make_test_translation();
        let resp = build_chapter_response_from_offline(&complete, "GEN", 50).unwrap();
        assert_eq!(
            resp.previous_chapter_api_link,
            Some("/api/BSB/GEN/49.json".to_string())
        );
        assert_eq!(
            resp.next_chapter_api_link,
            Some("/api/BSB/EXO/1.json".to_string())
        );
    }

    #[test]
    fn prev_link_crosses_to_previous_book() {
        let complete = make_test_translation();
        let resp = build_chapter_response_from_offline(&complete, "LEV", 1).unwrap();
        assert_eq!(
            resp.previous_chapter_api_link,
            Some("/api/BSB/EXO/40.json".to_string())
        );
        assert_eq!(
            resp.next_chapter_api_link,
            Some("/api/BSB/LEV/2.json".to_string())
        );
    }

    #[test]
    fn first_chapter_of_first_book_has_no_prev() {
        let complete = make_test_translation();
        let resp = build_chapter_response_from_offline(&complete, "GEN", 1).unwrap();
        assert_eq!(resp.previous_chapter_api_link, None);
        assert_eq!(
            resp.next_chapter_api_link,
            Some("/api/BSB/GEN/2.json".to_string())
        );
    }

    #[test]
    fn last_chapter_of_last_book_has_no_next() {
        let complete = make_test_translation();
        let resp = build_chapter_response_from_offline(&complete, "LEV", 27).unwrap();
        assert_eq!(
            resp.previous_chapter_api_link,
            Some("/api/BSB/LEV/26.json".to_string())
        );
        assert_eq!(resp.next_chapter_api_link, None);
    }

    #[test]
    fn returns_none_for_unknown_book() {
        let complete = make_test_translation();
        assert!(build_chapter_response_from_offline(&complete, "REV", 1).is_none());
    }

    #[test]
    fn returns_none_for_invalid_chapter() {
        let complete = make_test_translation();
        assert!(build_chapter_response_from_offline(&complete, "GEN", 51).is_none());
    }

    #[test]
    fn offline_response_includes_this_chapter_link() {
        let complete = make_test_translation();
        let resp = build_chapter_response_from_offline(&complete, "EXO", 3).unwrap();
        assert_eq!(resp.this_chapter_link, "/api/BSB/EXO/3.json");
        assert_eq!(
            resp.book.first_chapter_api_link,
            "/api/BSB/EXO/1.json"
        );
        assert_eq!(
            resp.book.last_chapter_api_link,
            "/api/BSB/EXO/40.json"
        );
    }
}
