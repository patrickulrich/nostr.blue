//! OpenLibrary API Client
//!
//! Fetches book metadata and covers from OpenLibrary.org.
//! No API key required. Covers can be fetched via direct URL construction.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
/// Base URL for OpenLibrary covers
const COVERS_BASE_URL: &str = "https://covers.openlibrary.org/b";
/// Base URL for OpenLibrary API
const API_BASE_URL: &str = "https://openlibrary.org";
/// Book information from OpenLibrary
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Book {
    /// Book title
    pub title: String,
    /// Authors
    pub authors: Vec<Author>,
    /// Publishers
    pub publishers: Option<Vec<Publisher>>,
    /// Publication date
    pub publish_date: Option<String>,
    /// Number of pages
    pub number_of_pages: Option<u32>,
    /// Cover URLs
    pub cover: Option<CoverUrls>,
    /// OpenLibrary info page URL
    pub url: String,
    /// OpenLibrary key (e.g., "/books/OL7353617M")
    pub key: Option<String>,
    /// Subject headings
    pub subjects: Option<Vec<Subject>>,
    /// ISBN-10
    pub isbn_10: Option<Vec<String>>,
    /// ISBN-13
    pub isbn_13: Option<Vec<String>>,
}
/// Author information
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Author {
    /// Author name
    pub name: String,
    /// OpenLibrary author page URL
    pub url: Option<String>,
}
/// Publisher information
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Publisher {
    /// Publisher name
    pub name: String,
}
/// Cover image URLs
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverUrls {
    /// Small cover (42px height)
    pub small: Option<String>,
    /// Medium cover (180px height)
    pub medium: Option<String>,
    /// Large cover (300px height)
    pub large: Option<String>,
}
/// Subject heading
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Subject {
    /// Subject name
    pub name: String,
    /// OpenLibrary subject page URL
    pub url: Option<String>,
}
/// Raw book data from API
#[derive(Debug, Deserialize)]
struct BookApiData {
    title: String,
    authors: Option<Vec<AuthorApiData>>,
    publishers: Option<Vec<PublisherApiData>>,
    publish_date: Option<String>,
    number_of_pages: Option<u32>,
    cover: Option<CoverApiData>,
    url: String,
    key: Option<String>,
    subjects: Option<Vec<SubjectApiData>>,
    #[serde(default)]
    identifiers: IdentifiersApiData,
}
#[derive(Debug, Deserialize)]
struct AuthorApiData {
    name: String,
    url: Option<String>,
}
#[derive(Debug, Deserialize)]
struct PublisherApiData {
    name: String,
}
#[derive(Debug, Deserialize)]
struct CoverApiData {
    small: Option<String>,
    medium: Option<String>,
    large: Option<String>,
}
#[derive(Debug, Deserialize)]
struct SubjectApiData {
    name: String,
    url: Option<String>,
}
#[derive(Debug, Default, Deserialize)]
struct IdentifiersApiData {
    isbn_10: Option<Vec<String>>,
    isbn_13: Option<Vec<String>>,
}
/// Cover image size options
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CoverSize {
    /// Small (~42px height)
    Small,
    /// Medium (~180px height)
    Medium,
}
impl CoverSize {
    fn as_str(&self) -> &'static str {
        match self {
            CoverSize::Small => "S",
            CoverSize::Medium => "M",
        }
    }
}
/// Get book cover URL by ISBN
///
/// This is a direct URL construction - no API call needed.
/// Returns a URL that may return a 1x1 pixel image if cover doesn't exist.
pub fn get_cover_url(isbn: &str, size: CoverSize) -> String {
    format!("{}/isbn/{}-{}.jpg", COVERS_BASE_URL, clean_isbn(isbn), size.as_str())
}
/// Fetch book metadata by ISBN
pub async fn get_book_by_isbn(isbn: &str) -> Result<Book, String> {
    let clean = clean_isbn(isbn);
    let bibkey = format!("ISBN:{}", clean);
    let url = format!(
        "{}/api/books?bibkeys={}&format=json&jscmd=data",
        API_BASE_URL,
        bibkey,
    );
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let data: HashMap<String, BookApiData> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    let book_data = data
        .get(&bibkey)
        .ok_or_else(|| format!("Book not found for ISBN: {}", isbn))?;
    Ok(convert_book_data(book_data, &clean))
}
/// Clean an ISBN (remove hyphens and spaces)
pub fn clean_isbn(isbn: &str) -> String {
    isbn.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}
fn convert_book_data(data: &BookApiData, isbn: &str) -> Book {
    Book {
        title: data.title.clone(),
        authors: data
            .authors
            .as_ref()
            .map(|authors| {
                authors
                    .iter()
                    .map(|a| Author {
                        name: a.name.clone(),
                        url: a.url.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        publishers: data
            .publishers
            .as_ref()
            .map(|pubs| {
                pubs.iter().map(|p| Publisher { name: p.name.clone() }).collect()
            }),
        publish_date: data.publish_date.clone(),
        number_of_pages: data.number_of_pages,
        cover: data
            .cover
            .as_ref()
            .map(|c| CoverUrls {
                small: c.small.clone(),
                medium: c.medium.clone(),
                large: c.large.clone(),
            }),
        url: data.url.clone(),
        key: data.key.clone(),
        subjects: data
            .subjects
            .as_ref()
            .map(|subs| {
                subs.iter()
                    .map(|s| Subject {
                        name: s.name.clone(),
                        url: s.url.clone(),
                    })
                    .collect()
            }),
        isbn_10: data.identifiers.isbn_10.clone(),
        isbn_13: data
            .identifiers
            .isbn_13
            .clone()
            .or_else(|| {
                if isbn.len() == 13 { Some(vec![isbn.to_string()]) } else { None }
            }),
    }
}
