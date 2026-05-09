use dioxus::prelude::*;

use crate::components::blobbi::core::types::{BlobbiCompanion, BlobbiStats, BlobbonautProfile};
use crate::components::blobbi::visual::recipe::ComposableRecipe;

#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub enum InlineActivityState {
    #[default]
    None,
    Music {
        track_id: String,
        is_published: bool,
    },
    Sing {
        is_published: bool,
    },
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct RoomContext {
    pub companion: Signal<BlobbiCompanion>,
    pub profile: Signal<Option<BlobbonautProfile>>,
    pub current_stats: Signal<BlobbiStats>,
    pub recipe: Signal<ComposableRecipe>,
    pub is_using_item: Signal<bool>,
    pub using_item_id: Signal<Option<String>>,
    pub show_photo_modal: Signal<bool>,
    pub show_post_modal: Signal<bool>,
    pub show_shop_modal: Signal<bool>,
    pub show_dev_editor: Signal<bool>,
    pub show_hatch_ceremony: Signal<bool>,
    pub show_adoption_flow: Signal<bool>,
    pub inline_activity: Signal<InlineActivityState>,
    pub action_in_progress: Signal<Option<String>>,
    pub is_publishing: Signal<bool>,
    pub is_current_companion: Signal<bool>,
    pub can_be_companion: Signal<bool>,
    pub is_updating_companion: Signal<bool>,
    pub hero_width: Signal<f64>,
}
