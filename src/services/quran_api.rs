//! Quran API client for fetching editions, surahs, and ayahs.
//!
//! Uses the Al Quran Cloud API (https://api.alquran.cloud/v1).
//! Free, no authentication required, MIT licensed.
use crate::platform::http::http_client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const QURAN_API_BASE: &str = "https://api.alquran.cloud/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Edition {
    pub identifier: String,
    pub language: String,
    pub name: String,
    #[serde(rename = "englishName")]
    pub english_name: String,
    pub format: String,
    #[serde(rename = "type")]
    pub edition_type: String,
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SurahRef {
    pub number: u32,
    pub name: String,
    #[serde(rename = "englishName")]
    pub english_name: String,
    #[serde(rename = "englishNameTranslation")]
    pub english_name_translation: String,
    #[serde(rename = "numberOfAyahs")]
    pub number_of_ayahs: u32,
    #[serde(rename = "revelationType")]
    pub revelation_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ayah {
    pub number: u32,
    pub text: String,
    #[serde(rename = "numberInSurah")]
    pub number_in_surah: u32,
    #[serde(default)]
    pub juz: u32,
    #[serde(default)]
    pub manzil: u32,
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub ruku: u32,
    #[serde(rename = "hizbQuarter", default)]
    pub hizb_quarter: u32,
    #[serde(rename = "sajda")]
    pub sajda: SajdaInfo,
    #[serde(default)]
    pub audio: Option<String>,
    #[serde(rename = "audioSecondary", default)]
    pub audio_secondary: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    #[serde(untagged)]
pub enum SajdaInfo {
    Bool(bool),
    Object { id: u32, recommended: bool, obligatory: bool },
}

impl SajdaInfo {
    pub fn is_sajda(&self) -> bool {
        match self {
            SajdaInfo::Bool(b) => *b,
            SajdaInfo::Object { .. } => true,
        }
    }

    #[allow(dead_code)]
    pub fn is_obligatory(&self) -> bool {
        match self {
            SajdaInfo::Bool(_) => false,
            SajdaInfo::Object { obligatory, .. } => *obligatory,
        }
    }
}

impl Default for SajdaInfo {
    fn default() -> Self {
        SajdaInfo::Bool(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SurahData {
    pub number: u32,
    pub name: String,
    #[serde(rename = "englishName")]
    pub english_name: String,
    #[serde(rename = "englishNameTranslation")]
    pub english_name_translation: String,
    #[serde(rename = "revelationType")]
    pub revelation_type: String,
    #[serde(rename = "numberOfAyahs")]
    pub number_of_ayahs: u32,
    pub ayahs: Vec<Ayah>,
    pub edition: Edition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SurahRefResponse {
    pub data: Vec<SurahRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SurahSingleResponse {
    pub data: SurahData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SurahMultiResponse {
    pub data: Vec<SurahData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct EditionListResponse {
    pub data: Vec<Edition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SearchResponse {
    pub data: SearchData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchData {
    pub count: u32,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchMatch {
    pub number: u32,
    pub text: String,
    #[serde(rename = "numberInSurah")]
    pub number_in_surah: u32,
    pub surah: SurahRef,
    pub edition: Edition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteQuranData {
    pub number: u32,
    pub surahs: Vec<SurahData>,
    pub edition: Edition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct CompleteQuranResponse {
    pub data: CompleteQuranData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct MetaResponse {
    pub data: MetaData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct MetaData {
    pub ayahs: MetaCount,
    pub surahs: MetaSurahs,
    pub juzs: MetaReferences,
    pub pages: MetaReferences,
    pub manzils: MetaReferences,
    pub rukus: MetaReferences,
    #[serde(rename = "hizbQuarters")]
    pub hizb_quarters: MetaReferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct MetaCount {
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct MetaSurahs {
    pub count: u32,
    pub references: Vec<SurahRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct MetaReferences {
    pub count: u32,
    pub references: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct JuzResponse {
    pub data: JuzData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct JuzData {
    pub number: u32,
    pub ayahs: Vec<Ayah>,
    pub edition: Edition,
}

async fn fetch_json<T: serde::de::DeserializeOwned>(
    url: &str,
    error_context: &str,
) -> Result<T, String> {
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
    let wrapper: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    let code = wrapper
        .get("code")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if code != 200 {
        let status = wrapper
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error");
        return Err(format!("API error ({}): {}", code, status));
    }
    let data_field = wrapper.get("data").cloned().unwrap_or(wrapper);
    serde_json::from_value(data_field)
        .map_err(|e| format!("Failed to parse {}: {}", error_context, e))
}

pub async fn fetch_editions() -> Result<Vec<Edition>, String> {
    fetch_json(
        &format!("{}/edition?format=text", QURAN_API_BASE),
        "editions",
    )
    .await
}

pub async fn fetch_audio_editions() -> Result<Vec<Edition>, String> {
    fetch_json(
        &format!("{}/edition?format=audio", QURAN_API_BASE),
        "audio editions",
    )
    .await
}

pub async fn fetch_surah_list() -> Result<Vec<SurahRef>, String> {
    fetch_json(&format!("{}/surah", QURAN_API_BASE), "surah list").await
}

#[allow(dead_code)]
pub async fn fetch_surah(
    number: u32,
    edition: &str,
) -> Result<SurahData, String> {
    let url = format!(
        "{}/surah/{}/{}",
        QURAN_API_BASE,
        number,
        urlencoding::encode(edition),
    );
    fetch_json(&url, "surah").await
}

pub async fn fetch_surah_multi(
    number: u32,
    editions: &[&str],
) -> Result<Vec<SurahData>, String> {
    let editions_str = editions.join(",");
    let url = format!(
        "{}/surah/{}/editions/{}",
        QURAN_API_BASE,
        number,
        urlencoding::encode(&editions_str),
    );
    fetch_json(&url, "surah multi-edition").await
}

pub async fn fetch_search(
    query: &str,
    surah: &str,
    edition: &str,
) -> Result<SearchData, String> {
    let url = format!(
        "{}/search/{}/{}/{}",
        QURAN_API_BASE,
        urlencoding::encode(query),
        surah,
        urlencoding::encode(edition),
    );
    fetch_json(&url, "search").await
}

pub async fn fetch_complete_quran(edition: &str) -> Result<CompleteQuranData, String> {
    let url = format!(
        "{}/quran/{}",
        QURAN_API_BASE,
        urlencoding::encode(edition),
    );
    fetch_json(&url, "complete quran").await
}

#[allow(dead_code)]
pub async fn fetch_juz(juz: u32, edition: &str) -> Result<JuzData, String> {
    let url = format!(
        "{}/juz/{}/{}",
        QURAN_API_BASE,
        juz,
        urlencoding::encode(edition),
    );
    fetch_json(&url, "juz").await
}

pub fn get_audio_url(reciter: &str, ayah_number: u32) -> String {
    format!(
        "https://cdn.islamic.network/quran/audio/128/{}/{}.mp3",
        reciter, ayah_number
    )
}

#[allow(dead_code)]
pub fn get_surah_api_url(surah: u32, edition: &str) -> String {
    format!(
        "{}/surah/{}/{}",
        QURAN_API_BASE,
        surah,
        urlencoding::encode(edition),
    )
}

pub fn format_ayah_reference(
    surah_name: &str,
    ayah_in_surah: u32,
    edition: &str,
) -> String {
    format!("{} {} ({})", surah_name, ayah_in_surah, edition)
}

pub fn get_nostr_blue_quran_url(surah: u32, ayah: u32) -> String {
    format!("https://nostr.blue/quran/{}/{}", surah, ayah)
}

pub const DEFAULT_ARABIC_EDITION: &str = "quran-uthmani";
pub const DEFAULT_TRANSLATION_EDITION: &str = "en.asad";
#[allow(dead_code)]
pub const DEFAULT_AUDIO_RECITER: &str = "ar.alafasy";

pub const RECOMMENDED_TRANSLATIONS: &[&str] = &[
    "en.asad",
    "en.sahih",
    "en.pickthall",
    "en.yusufali",
    "en.hilali",
];

pub fn sort_editions_by_priority(
    editions: Vec<Edition>,
    favorites: &[String],
) -> Vec<Edition> {
    let mut result = editions;
    result.sort_by(|a, b| {
        let a_fav = favorites.iter().position(|f| f == &a.identifier);
        let b_fav = favorites.iter().position(|f| f == &b.identifier);
        let a_rec = RECOMMENDED_TRANSLATIONS
            .iter()
            .position(|&x| x == a.identifier);
        let b_rec = RECOMMENDED_TRANSLATIONS
            .iter()
            .position(|&x| x == b.identifier);
        let a_rank = a_fav.map(|p| (0, p)).or_else(|| a_rec.map(|p| (1, p)));
        let b_rank = b_fav.map(|p| (0, p)).or_else(|| b_rec.map(|p| (1, p)));
        match (a_rank, b_rank) {
            (Some(ar), Some(br)) => ar.cmp(&br),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.english_name.cmp(&b.english_name),
        }
    });
    result
}

pub fn group_editions_by_language(editions: &[Edition]) -> BTreeMap<String, Vec<Edition>> {
    let mut groups: BTreeMap<String, Vec<Edition>> = BTreeMap::new();
    for e in editions {
        let lang = e.language.clone();
        groups.entry(lang).or_default().push(e.clone());
    }
    groups
}

pub fn filter_translations(editions: &[Edition]) -> Vec<Edition> {
    editions
        .iter()
        .filter(|e| e.edition_type == "translation" && e.format == "text")
        .cloned()
        .collect()
}

pub fn build_surah_from_offline(
    complete: &CompleteQuranData,
    surah_number: u32,
) -> Option<SurahData> {
    complete.surahs.iter().find(|s| s.number == surah_number).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sajda_bool_false() {
        let info = SajdaInfo::Bool(false);
        assert!(!info.is_sajda());
        assert!(!info.is_obligatory());
    }

    #[test]
    fn sajda_bool_true() {
        let info = SajdaInfo::Bool(true);
        assert!(info.is_sajda());
        assert!(!info.is_obligatory());
    }

    #[test]
    fn sajda_object_obligatory() {
        let info = SajdaInfo::Object {
            id: 10,
            recommended: false,
            obligatory: true,
        };
        assert!(info.is_sajda());
        assert!(info.is_obligatory());
    }

    #[test]
    fn audio_url_format() {
        let url = get_audio_url("ar.alafasy", 262);
        assert_eq!(
            url,
            "https://cdn.islamic.network/quran/audio/128/ar.alafasy/262.mp3"
        );
    }

    #[test]
    fn nostr_blue_url_format() {
        let url = get_nostr_blue_quran_url(2, 255);
        assert_eq!(url, "https://nostr.blue/quran/2/255");
    }

    #[test]
    fn filter_translations_excludes_quran_type() {
        let editions = vec![
            Edition {
                identifier: "quran-uthmani".into(),
                language: "ar".into(),
                name: "Uthmani".into(),
                english_name: "Uthmani".into(),
                format: "text".into(),
                edition_type: "quran".into(),
                direction: Some("rtl".into()),
            },
            Edition {
                identifier: "en.asad".into(),
                language: "en".into(),
                name: "Asad".into(),
                english_name: "Muhammad Asad".into(),
                format: "text".into(),
                edition_type: "translation".into(),
                direction: Some("ltr".into()),
            },
        ];
        let filtered = filter_translations(&editions);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].identifier, "en.asad");
    }
}
