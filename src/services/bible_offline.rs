use crate::services::bible_api::{ChapterResponse, TranslationComplete};

#[async_trait::async_trait(?Send)]
pub trait BibleOfflineStorage {
    async fn save_complete_translation(
        &self,
        translation_id: &str,
        data: &TranslationComplete,
    ) -> Result<(), String>;
    async fn load_chapter(
        &self,
        translation: &str,
        book: &str,
        chapter: u32,
    ) -> Option<ChapterResponse>;
    async fn delete_translation(&self, translation: &str) -> Result<(), String>;
    async fn list_downloaded(&self) -> Vec<String>;
}

pub fn offline_storage() -> Box<dyn BibleOfflineStorage> {
    #[cfg(feature = "web")]
    {
        Box::new(super::bible_offline_indexeddb::IndexedDbBibleStorage::new())
    }
    #[cfg(feature = "native")]
    {
        Box::new(super::bible_offline_sqlite::SqliteBibleStorage::new())
    }
}
