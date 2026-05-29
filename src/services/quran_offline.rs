use crate::services::quran_api::{CompleteQuranData, SurahData};

#[async_trait::async_trait(?Send)]
pub trait QuranOfflineStorage {
    async fn save_complete_quran(
        &self,
        edition_id: &str,
        data: &CompleteQuranData,
    ) -> Result<(), String>;
    async fn load_surah(&self, edition: &str, surah: u32) -> Option<SurahData>;
    async fn delete_edition(&self, edition: &str) -> Result<(), String>;
    async fn list_downloaded(&self) -> Vec<String>;
}

pub fn offline_storage() -> Box<dyn QuranOfflineStorage> {
    #[cfg(feature = "web")]
    {
        Box::new(super::quran_offline_indexeddb::IndexedDbQuranStorage::new())
    }
    #[cfg(feature = "native")]
    {
        Box::new(super::quran_offline_sqlite::SqliteQuranStorage::new())
    }
}
