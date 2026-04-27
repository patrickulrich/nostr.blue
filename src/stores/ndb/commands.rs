use nostrdb::NoteKey;

pub enum NdbRequest {
    QueryNoteKeys {
        filter_jsons: Vec<String>,
        limit: i32,
        reply: tokio::sync::oneshot::Sender<Result<Vec<(NoteKey, u64)>, String>>,
    },
    GetNoteData {
        key: NoteKey,
        reply: tokio::sync::oneshot::Sender<Result<NoteData, String>>,
    },
    GetNoteDataById {
        id: [u8; 32],
        reply: tokio::sync::oneshot::Sender<Result<Option<NoteData>, String>>,
    },
    GetProfile {
        pubkey: [u8; 32],
        reply: tokio::sync::oneshot::Sender<Result<Option<ProfileData>, String>>,
    },
    Subscribe {
        key: String,
        filter_jsons: Vec<String>,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Unsubscribe {
        key: String,
    },
    Shutdown,
}

#[derive(Clone, Debug)]
pub struct NoteData {
    pub id: [u8; 32],
    pub pubkey: [u8; 32],
    pub kind: u16,
    pub content: String,
    pub created_at: u64,
    pub tags: Vec<Vec<String>>,
    pub sig: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ProfileData {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub about: Option<String>,
    pub picture: Option<String>,
    pub banner: Option<String>,
    pub nip05: Option<String>,
    pub website: Option<String>,
    pub lud16: Option<String>,
}

#[derive(Clone, Debug)]
pub enum NdbEvent {
    SubscriptionUpdated {
        key: String,
        new_notes: Vec<NoteData>,
    },
}
