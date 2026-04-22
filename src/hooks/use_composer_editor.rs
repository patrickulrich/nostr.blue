use crate::stores::{auth_store, note_draft_store};
use crate::utils::custom_emoji::EmojiSelection;
use dioxus::prelude::*;

const MAX_LENGTH: usize = 5000;

pub struct ComposerConfig {
    pub draft_context: Option<String>,
    pub initial_content: String,
}

#[derive(Clone, Copy, PartialEq)]
pub struct UseComposerEditor {
    pub content: Signal<String>,
    pub cursor_position: Signal<usize>,
    pub show_media_uploader: Signal<bool>,
    pub show_poll_modal: Signal<bool>,
    pub is_publishing: Signal<bool>,
    pub char_count: Memo<usize>,
    pub remaining: Memo<usize>,
    pub is_over_limit: Memo<bool>,
    pub show_warning: Memo<bool>,
    pub can_publish: Memo<bool>,
    pub counter_color: Memo<&'static str>,
    draft_context: Signal<Option<String>>,
}

pub fn use_composer_editor(config: ComposerConfig) -> UseComposerEditor {
    let content: Signal<String> = use_signal(move || config.initial_content);
    let cursor_position = use_signal(|| 0usize);
    let show_media_uploader = use_signal(|| false);
    let show_poll_modal = use_signal(|| false);
    let is_publishing = use_signal(|| false);

    let char_count = use_memo(move || content.read().chars().count());
    let remaining = use_memo(move || MAX_LENGTH.saturating_sub(*char_count.read()));
    let is_over_limit = use_memo(move || *char_count.read() > MAX_LENGTH);
    let show_warning = use_memo(move || *remaining.read() < 100 && !*is_over_limit.read());
    let can_publish = use_memo(move || {
        *char_count.read() > 0 && !*is_over_limit.read() && !*is_publishing.read()
    });
    let counter_color = use_memo(move || {
        if *is_over_limit.read() {
            "text-red-500"
        } else if *show_warning.read() {
            "text-yellow-500"
        } else {
            "text-gray-500"
        }
    });

    let draft_ctx = config.draft_context.clone();
    let draft_context_signal: Signal<Option<String>> = use_signal(move || draft_ctx);
    use_effect(move || {
        let ctx = draft_context_signal.read().clone();
        if let Some(ref ctx) = ctx {
            let text = content.read();
            if !text.is_empty() {
                if let Some(pk) = auth_store::get_pubkey() {
                    note_draft_store::save_note_draft(
                        &pk,
                        ctx,
                        &note_draft_store::NoteDraft {
                            content: text.clone(),
                            saved_at: crate::platform::timestamp::now_secs(),
                        },
                    );
                }
            }
        }
    });

    UseComposerEditor {
        content,
        cursor_position,
        show_media_uploader,
        show_poll_modal,
        is_publishing,
        char_count,
        remaining,
        is_over_limit,
        show_warning,
        can_publish,
        counter_color,
        draft_context: draft_context_signal,
    }
}

impl UseComposerEditor {
    pub fn insert_at_cursor(&self, text: String) {
        let mut content = self.content;
        let mut cursor_position = self.cursor_position;
        let mut current = content.read().clone();
        let pos = to_char_boundary(&current, *cursor_position.read());
        current.insert_str(pos, &text);
        content.set(current);
        cursor_position.set(pos + text.len());
    }

    pub fn insert_with_spacing(&self, text: String) {
        let mut spaced = text;
        let current = self.content.read().clone();
        let pos = to_char_boundary(&current, *self.cursor_position.read());
        if pos > 0 {
            if let Some(prev) = current[..pos].chars().last() {
                if !prev.is_whitespace() {
                    spaced.insert(0, ' ');
                }
            }
        }
        if pos < current.len() {
            if let Some(next) = current[pos..].chars().next() {
                if !next.is_whitespace() {
                    spaced.push(' ');
                }
            }
        }
        self.insert_at_cursor(spaced);
    }

    pub fn handle_media_uploaded(&self, url: String) {
        self.insert_with_spacing(url);
        let mut show_media_uploader = self.show_media_uploader;
        show_media_uploader.set(false);
    }

    pub fn handle_emoji_selected(&self, selection: EmojiSelection) {
        self.insert_at_cursor(selection.insertion_text());
    }

    pub fn handle_gif_selected(&self, url: String) {
        self.insert_with_spacing(url);
    }

    pub fn handle_poll_created(&self, nevent_ref: String) {
        self.insert_with_spacing(nevent_ref);
        let mut show_poll_modal = self.show_poll_modal;
        show_poll_modal.set(false);
    }

    pub fn clear(&self) {
        let mut content = self.content;
        let mut show_media_uploader = self.show_media_uploader;
        content.set(String::new());
        show_media_uploader.set(false);
    }

    pub fn clear_draft(&self) {
        let ctx = self.draft_context.read().clone();
        if let Some(ref ctx) = ctx {
            if let Some(pk) = auth_store::get_pubkey() {
                note_draft_store::clear_note_draft(&pk, ctx);
            }
        }
    }

    pub fn content_value(&self) -> String {
        self.content.read().clone()
    }
}

pub fn to_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    if s.is_char_boundary(pos) {
        return pos;
    }
    for offset in 1..=3 {
        if pos >= offset && s.is_char_boundary(pos - offset) {
            return pos - offset;
        }
    }
    0
}

pub fn restore_draft_or_empty(draft_context: &str) -> String {
    auth_store::get_pubkey()
        .and_then(|pk| note_draft_store::read_note_draft(&pk, draft_context))
        .map(|d| d.content)
        .unwrap_or_default()
}
