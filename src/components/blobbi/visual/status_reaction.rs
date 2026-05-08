use dioxus::prelude::*;

use crate::components::blobbi::actions::action_types::BlobbiActionType;
use crate::components::blobbi::visual::action_emotion::{action_emotion, ACTION_EMOTION_DURATION_MS};
use crate::components::blobbi::visual::recipe::{
    emotion_recipe, merge_recipes, resolve_composable, ComposableRecipe, EmotionPreset,
};
use crate::components::blobbi::core::types::BlobbiCompanion;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmotionOverride {
    pub emotion: EmotionPreset,
    pub expires_at: Option<f64>,
}

static BLOBBI_EMOTION_OVERRIDE: GlobalSignal<Option<EmotionOverride>> = Signal::global(|| None);

fn now_ms() -> f64 {
    crate::platform::timestamp::now_millis() as f64
}

pub fn resolve_recipe_with_override(blobbi: &BlobbiCompanion) -> ComposableRecipe {
    let base_recipe = resolve_composable(
        &blobbi.stats,
        blobbi.is_sleeping(),
        &blobbi.personality.mood,
    );

    let override_data = BLOBBI_EMOTION_OVERRIDE.read().clone();
    match override_data.as_ref() {
        Some(ov) => {
            let expired = ov.expires_at.is_some_and(|t| now_ms() >= t);
            if expired {
                *BLOBBI_EMOTION_OVERRIDE.write() = None;
                base_recipe
            } else {
                merge_recipes(&base_recipe, &emotion_recipe(ov.emotion))
            }
        }
        None => base_recipe,
    }
}

pub fn trigger_action_emotion(action: BlobbiActionType) {
    let emotion = action_emotion(action);
    *BLOBBI_EMOTION_OVERRIDE.write() = Some(EmotionOverride {
        emotion,
        expires_at: Some(now_ms() + ACTION_EMOTION_DURATION_MS as f64),
    });
}

pub fn use_emotion_override_expiry() {
    use_future(move || async move {
        loop {
            crate::platform::timer::sleep_ms(500).await;
            let should_clear = match BLOBBI_EMOTION_OVERRIDE.read().as_ref() {
                Some(ov) => ov.expires_at.is_some_and(|t| now_ms() >= t),
                None => false,
            };
            if should_clear {
                *BLOBBI_EMOTION_OVERRIDE.write() = None;
            }
        }
    });
}
