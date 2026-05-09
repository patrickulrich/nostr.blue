use crate::components::blobbi::actions::action_types::BlobbiActionType;
use crate::components::blobbi::visual::recipe::EmotionPreset;

pub fn action_emotion(action: BlobbiActionType) -> EmotionPreset {
    match action {
        BlobbiActionType::Feed => EmotionPreset::Happy,
        BlobbiActionType::Play => EmotionPreset::Excited,
        BlobbiActionType::Clean => EmotionPreset::Surprised,
        BlobbiActionType::Medicine => EmotionPreset::Curious,
        BlobbiActionType::PlayMusic => EmotionPreset::Happy,
        BlobbiActionType::Sing => EmotionPreset::Excited,
        BlobbiActionType::Warm => EmotionPreset::Adoring,
        BlobbiActionType::Talk => EmotionPreset::Happy,
        BlobbiActionType::Rest => EmotionPreset::Sleepy,
        BlobbiActionType::Check => EmotionPreset::Neutral,
        BlobbiActionType::Cruzar => EmotionPreset::Adoring,
        BlobbiActionType::UseItem => EmotionPreset::Curious,
    }
}

pub const ACTION_EMOTION_DURATION_MS: u64 = 1500;
