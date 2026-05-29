//! Quran Store
//! Handles Quran reading state, surah caching, and NIP-84 highlights (Kind 9802)
//!
//! Uses Al Quran Cloud API for content and Nostr for social highlighting features.
#![allow(dead_code)]
pub use crate::services::quran_api::{
    fetch_editions, fetch_surah_list, fetch_surah_multi,
    filter_translations, get_nostr_blue_quran_url,
    group_editions_by_language, sort_editions_by_priority, Edition,
    SurahData, SurahRef, DEFAULT_ARABIC_EDITION,
    DEFAULT_TRANSLATION_EDITION, RECOMMENDED_TRANSLATIONS,
};
pub use crate::utils::nip84::Highlight as QuranHighlight;
use crate::utils::nip84::{self, HighlightSource};
use dioxus::prelude::*;
use lru::LruCache;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::collections::BTreeMap;

type StdResult<T, E> = std::result::Result<T, E>;

const SURAH_CACHE_SIZE: usize = 114;

#[derive(Clone, Debug)]
pub struct CachedSurah {
    pub arabic: SurahData,
    pub translation: Option<SurahData>,
    pub fetched_at: u64,
}

#[derive(Clone, Debug, Default)]
pub struct SurahHighlightStats {
    pub ayah_counts: HashMap<u32, usize>,
    pub total: usize,
}

pub static SURAH_LIST: GlobalSignal<Vec<SurahRef>> = GlobalSignal::new(Vec::new);
pub static ALL_EDITIONS: GlobalSignal<Vec<Edition>> = GlobalSignal::new(Vec::new);
pub static TRANSLATION_EDITIONS: GlobalSignal<Vec<Edition>> = GlobalSignal::new(Vec::new);
pub static AUDIO_EDITIONS: GlobalSignal<Vec<Edition>> = GlobalSignal::new(Vec::new);
pub static GROUPED_EDITIONS: GlobalSignal<BTreeMap<String, Vec<Edition>>> =
    GlobalSignal::new(BTreeMap::new);
pub static CURRENT_TRANSLATION: GlobalSignal<String> =
    GlobalSignal::new(|| DEFAULT_TRANSLATION_EDITION.to_string());
pub static SURAH_CACHE: GlobalSignal<LruCache<String, CachedSurah>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(SURAH_CACHE_SIZE).unwrap()));
pub static USER_HIGHLIGHTS: GlobalSignal<Vec<QuranHighlight>> = GlobalSignal::new(Vec::new);
pub static CURRENT_SURAH_HIGHLIGHTS: GlobalSignal<Vec<QuranHighlight>> =
    GlobalSignal::new(Vec::new);
pub static LOADING_EDITIONS: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_SURAH: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_HIGHLIGHTS: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static QURAN_STORE_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static FAVORITE_EDITIONS: GlobalSignal<Vec<String>> = Signal::global(|| {
    crate::platform::storage::get::<Vec<String>>("nostr_blue_quran_favorite_editions")
        .unwrap_or_default()
});
pub static DOWNLOADED_EDITIONS: GlobalSignal<Vec<String>> = GlobalSignal::new(Vec::new);
pub static DOWNLOAD_IN_PROGRESS: GlobalSignal<Option<String>> = GlobalSignal::new(|| None);
pub static LAST_POSITION: GlobalSignal<Option<(u32, String)>> = GlobalSignal::new(|| None);
static LATEST_REQUESTED_SURAH: GlobalSignal<String> = GlobalSignal::new(String::new);

fn surah_cache_key(surah: u32, translation: &str) -> String {
    format!("{}:{}", surah, translation)
}

pub fn get_cached_surah(surah: u32, translation: &str) -> Option<CachedSurah> {
    let key = surah_cache_key(surah, translation);
    SURAH_CACHE.write().get(&key).cloned()
}

pub fn cache_surah(surah: u32, translation: &str, cached: CachedSurah) {
    let key = surah_cache_key(surah, translation);
    SURAH_CACHE.write().put(key, cached);
}

pub fn get_all_cached_surahs() -> Vec<CachedSurah> {
    SURAH_CACHE
        .read()
        .iter()
        .map(|(_, c)| c.clone())
        .collect()
}

pub fn cached_surah_count() -> usize {
    SURAH_CACHE.read().len()
}

pub fn clear_surah_cache() {
    SURAH_CACHE.write().clear();
}

pub async fn initialize() -> StdResult<(), String> {
    if *QURAN_STORE_INITIALIZED.read() {
        return Ok(());
    }
    *LOADING_EDITIONS.write() = true;
    let result = async {
        let surahs = fetch_surah_list().await?;
        *SURAH_LIST.write() = surahs;
        let all = fetch_editions().await?;
        let translations = filter_translations(&all);
        let favorites = FAVORITE_EDITIONS.read().clone();
        let sorted = sort_editions_by_priority(translations.clone(), &favorites);
        let grouped = group_editions_by_language(&sorted);
        *ALL_EDITIONS.write() = all;
        *TRANSLATION_EDITIONS.write() = sorted;
        *GROUPED_EDITIONS.write() = grouped;
        let audio = crate::services::quran_api::fetch_audio_editions().await?;
        *AUDIO_EDITIONS.write() = audio;
        let storage = crate::services::quran_offline::offline_storage();
        *DOWNLOADED_EDITIONS.write() = storage.list_downloaded().await;
        Ok(())
    }
    .await;
    *LOADING_EDITIONS.write() = false;
    if result.is_ok() {
        *QURAN_STORE_INITIALIZED.write() = true;
    }
    result
}

pub async fn load_surah(
    surah: u32,
    translation: &str,
) -> StdResult<CachedSurah, String> {
    if let Some(cached) = get_cached_surah(surah, translation) {
        *LAST_POSITION.write() = Some((surah, translation.to_string()));
        return Ok(cached);
    }
    if is_offline_available(translation) || is_offline_available(DEFAULT_ARABIC_EDITION) {
        let storage = crate::services::quran_offline::offline_storage();
        let arabic_edition = if is_offline_available(DEFAULT_ARABIC_EDITION) {
            DEFAULT_ARABIC_EDITION
        } else {
            translation
        };
        if let Some(arabic) = storage.load_surah(arabic_edition, surah).await {
            let translation_data = if translation != DEFAULT_ARABIC_EDITION
                && is_offline_available(translation)
            {
                storage.load_surah(translation, surah).await
            } else {
                None
            };
            let cached = CachedSurah {
                arabic,
                translation: translation_data,
                fetched_at: crate::platform::timestamp::now_secs(),
            };
            cache_surah(surah, translation, cached.clone());
            *LAST_POSITION.write() = Some((surah, translation.to_string()));
            return Ok(cached);
        }
    }
    let surah_key = surah_cache_key(surah, translation);
    *LATEST_REQUESTED_SURAH.write() = surah_key.clone();
    *LOADING_SURAH.write() = true;
    let result = async {
        let editions = vec![DEFAULT_ARABIC_EDITION, translation];
        let multi = fetch_surah_multi(surah, &editions).await?;
        let arabic = multi
            .iter()
            .find(|s| s.edition.identifier == DEFAULT_ARABIC_EDITION)
            .cloned()
            .ok_or("Arabic edition not found in response")?;
        let translation_data = multi
            .iter()
            .find(|s| s.edition.identifier == translation)
            .cloned();
        Ok(CachedSurah {
            arabic,
            translation: translation_data,
            fetched_at: nostr_sdk::Timestamp::now().as_secs(),
        })
    }
    .await;
    match result {
        Ok(cached) => {
            cache_surah(surah, translation, cached.clone());
            if *LATEST_REQUESTED_SURAH.read() == surah_key {
                *LOADING_SURAH.write() = false;
                *LAST_POSITION.write() = Some((surah, translation.to_string()));
            }
            Ok(cached)
        }
        Err(e) => {
            if *LATEST_REQUESTED_SURAH.read() == surah_key {
                *LOADING_SURAH.write() = false;
            }
            Err(e)
        }
    }
}

pub async fn create_highlight(
    ayah_text: &str,
    reference: &str,
    surah: u32,
    ayah: u32,
    comment: Option<&str>,
) -> StdResult<nostr_sdk::prelude::EventId, String> {
    let source_url = get_nostr_blue_quran_url(surah, ayah);
    let event_id = nip84::create_highlight(
        ayah_text,
        HighlightSource::Url(source_url),
        Some(reference),
        comment,
        vec!["quran"],
    )
    .await?;
    log::info!("Quran highlight published: {}", event_id.to_hex());
    if let Ok(pubkey) = crate::stores::nostr_client::get_cached_pubkey() {
        let _ = fetch_user_highlights(&pubkey).await;
    }
    Ok(event_id)
}

pub async fn fetch_user_highlights(
    pubkey: &nostr_sdk::prelude::PublicKey,
) -> StdResult<Vec<QuranHighlight>, String> {
    *LOADING_HIGHLIGHTS.write() = true;
    match nip84::fetch_user_highlights(pubkey).await {
        Ok(all_highlights) => {
            let highlights = nip84::filter_quran_highlights(all_highlights);
            *USER_HIGHLIGHTS.write() = highlights.clone();
            *LOADING_HIGHLIGHTS.write() = false;
            Ok(highlights)
        }
        Err(e) => {
            *LOADING_HIGHLIGHTS.write() = false;
            Err(e)
        }
    }
}

pub async fn fetch_surah_highlights(
    surah: u32,
) -> StdResult<Vec<QuranHighlight>, String> {
    let quran_url = format!("https://nostr.blue/quran/{}/", surah);
    CURRENT_SURAH_HIGHLIGHTS.write().clear();
    match nip84::fetch_highlights_by_url(&quran_url).await {
        Ok(highlights) => {
            *CURRENT_SURAH_HIGHLIGHTS.write() = highlights.clone();
            Ok(highlights)
        }
        Err(e) => Err(format!("Failed to fetch surah highlights: {}", e)),
    }
}

fn extract_ayah_from_reference(reference: &str) -> Option<u32> {
    let parts: Vec<&str> = reference.split(' ').collect();
    if parts.len() >= 2 {
        parts[1].parse().ok()
    } else {
        None
    }
}

pub fn is_ayah_highlighted(surah: u32, ayah: u32) -> bool {
    let quran_url = get_nostr_blue_quran_url(surah, ayah);
    let user_highlights = USER_HIGHLIGHTS.read();
    user_highlights.iter().any(|h| {
        h.source.as_url().map(|u| u == quran_url).unwrap_or(false)
    })
}

pub fn get_ayah_highlight_count(ayah: u32) -> usize {
    let highlights = CURRENT_SURAH_HIGHLIGHTS.read();
    highlights
        .iter()
        .filter(|h| {
            h.context
                .as_deref()
                .and_then(extract_ayah_from_reference)
                .map(|a| a == ayah)
                .unwrap_or(false)
        })
        .count()
}

pub fn get_user_highlight_for_ayah(surah: u32, ayah: u32) -> Option<QuranHighlight> {
    let quran_url = get_nostr_blue_quran_url(surah, ayah);
    let user_highlights = USER_HIGHLIGHTS.read();
    user_highlights
        .iter()
        .find(|h| h.source.as_url().map(|u| u == quran_url).unwrap_or(false))
        .cloned()
}

pub fn get_highlights_for_ayah(ayah: u32) -> Vec<QuranHighlight> {
    let highlights = CURRENT_SURAH_HIGHLIGHTS.read();
    highlights
        .iter()
        .filter(|h| {
            h.context
                .as_deref()
                .and_then(extract_ayah_from_reference)
                .map(|a| a == ayah)
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuranSearchResult {
    pub surah: u32,
    pub surah_name: String,
    pub ayah: u32,
    pub ayah_in_surah: u32,
    pub text: String,
    pub edition: String,
}

pub fn search_cached_ayahs(query: &str, limit: usize) -> Vec<QuranSearchResult> {
    let query = query.trim();
    if query.chars().count() < 3 {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    let cache = SURAH_CACHE.read();
    let mut results = Vec::new();
    for (key, cached) in cache.iter() {
        let surah_num = cached.arabic.number;
        let surah_name = cached.arabic.english_name.clone();
        let edition = key.split(':').nth(1).unwrap_or("").to_string();
        let data = cached.translation.as_ref().unwrap_or(&cached.arabic);
        for ayah in &data.ayahs {
            if ayah.text.to_lowercase().contains(&query_lower) {
                results.push(QuranSearchResult {
                    surah: surah_num,
                    surah_name: surah_name.clone(),
                    ayah: ayah.number,
                    ayah_in_surah: ayah.number_in_surah,
                    text: ayah.text.clone(),
                    edition: edition.clone(),
                });
                if results.len() >= limit {
                    return results;
                }
            }
        }
    }
    results
}

pub fn get_surah_ref(number: u32) -> Option<SurahRef> {
    SURAH_LIST
        .read()
        .iter()
        .find(|s| s.number == number)
        .cloned()
}

pub fn toggle_favorite(id: &str) {
    let mut favs = FAVORITE_EDITIONS.write();
    if let Some(pos) = favs.iter().position(|f| f == id) {
        favs.remove(pos);
    } else {
        favs.push(id.to_string());
    }
    let _ = crate::platform::storage::set("nostr_blue_quran_favorite_editions", &*favs);
}

pub fn is_favorite(id: &str) -> bool {
    FAVORITE_EDITIONS.read().contains(&id.to_string())
}

pub fn is_offline_available(id: &str) -> bool {
    DOWNLOADED_EDITIONS.read().contains(&id.to_string())
}

pub async fn download_edition(id: &str) -> StdResult<(), String> {
    *DOWNLOAD_IN_PROGRESS.write() = Some(id.to_string());
    let result = async {
        let complete = crate::services::quran_api::fetch_complete_quran(id).await?;
        let storage = crate::services::quran_offline::offline_storage();
        storage.save_complete_quran(id, &complete).await?;
        Ok(())
    }
    .await;
    if result.is_ok() {
        let mut downloaded = DOWNLOADED_EDITIONS.write();
        if !downloaded.contains(&id.to_string()) {
            downloaded.push(id.to_string());
        }
    }
    *DOWNLOAD_IN_PROGRESS.write() = None;
    result
}

pub async fn remove_offline_edition(id: &str) -> StdResult<(), String> {
    let storage = crate::services::quran_offline::offline_storage();
    storage.delete_edition(id).await?;
    DOWNLOADED_EDITIONS.write().retain(|d| d != id);
    Ok(())
}

pub fn split_surahs_by_revelation(surahs: &[SurahRef]) -> (Vec<SurahRef>, Vec<SurahRef>) {
    let meccan: Vec<SurahRef> = surahs
        .iter()
        .filter(|s| s.revelation_type == "Meccan")
        .cloned()
        .collect();
    let medinan: Vec<SurahRef> = surahs
        .iter()
        .filter(|s| s.revelation_type == "Medinan")
        .cloned()
        .collect();
    (meccan, medinan)
}

pub fn get_juz_surahs(juz: u32) -> Vec<(u32, u32, u32)> {
    let juz_starts: Vec<(u32, u32)> = vec![
        (1, 1), (2, 142), (2, 253), (3, 93), (4, 24), (4, 148),
        (5, 82), (6, 111), (7, 88), (8, 41), (9, 93), (11, 6),
        (12, 53), (15, 1), (17, 1), (18, 75), (21, 1), (23, 1),
        (25, 21), (27, 56), (29, 26), (33, 31), (36, 28), (39, 32),
        (41, 47), (46, 1), (51, 31), (58, 1), (67, 1), (78, 1),
    ];
    let idx = (juz as usize).saturating_sub(1);
    if idx >= juz_starts.len() {
        return Vec::new();
    }
    let (start_surah, _start_ayah) = juz_starts[idx];
    let end_surah = if idx + 1 < juz_starts.len() {
        juz_starts[idx + 1].0
    } else {
        114
    };
    let mut result = Vec::new();
    let surahs = SURAH_LIST.read();
    for s in surahs.iter() {
        if s.number >= start_surah && s.number <= end_surah {
            result.push((s.number, s.number_of_ayahs, s.number_of_ayahs));
        }
    }
    result
}

pub fn clear_store() {
    SURAH_CACHE.write().clear();
    *USER_HIGHLIGHTS.write() = Vec::new();
    *CURRENT_SURAH_HIGHLIGHTS.write() = Vec::new();
    *LAST_POSITION.write() = None;
    *SURAH_LIST.write() = Vec::new();
    *ALL_EDITIONS.write() = Vec::new();
    *TRANSLATION_EDITIONS.write() = Vec::new();
    *AUDIO_EDITIONS.write() = Vec::new();
    *GROUPED_EDITIONS.write() = BTreeMap::new();
    *DOWNLOADED_EDITIONS.write() = Vec::new();
    *DOWNLOAD_IN_PROGRESS.write() = None;
    *CURRENT_TRANSLATION.write() = DEFAULT_TRANSLATION_EDITION.to_string();
    *LOADING_EDITIONS.write() = false;
    *LOADING_SURAH.write() = false;
    *LOADING_HIGHLIGHTS.write() = false;
    *LATEST_REQUESTED_SURAH.write() = String::new();
    *QURAN_STORE_INITIALIZED.write() = false;
}
