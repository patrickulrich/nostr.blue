use dioxus::prelude::*;

#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub enum BlobbiReactionState {
    #[default]
    Idle,
    Listening,
    Swaying,
    Singing,
    Happy,
}

pub static BLOBBI_REACTION: GlobalSignal<BlobbiReactionState> = Signal::global(BlobbiReactionState::default);

pub fn set_reaction(reaction: BlobbiReactionState) {
    *BLOBBI_REACTION.write() = reaction;
}

pub fn reset_reaction() {
    *BLOBBI_REACTION.write() = BlobbiReactionState::Idle;
}

pub fn reaction_string() -> Option<String> {
    match &*BLOBBI_REACTION.read() {
        BlobbiReactionState::Idle => None,
        BlobbiReactionState::Listening => Some("listening".to_string()),
        BlobbiReactionState::Swaying => Some("swaying".to_string()),
        BlobbiReactionState::Singing => Some("singing".to_string()),
        BlobbiReactionState::Happy => Some("happy".to_string()),
    }
}
