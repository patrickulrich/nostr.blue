use crate::platform::editor_dom;
use crate::stores::{auth_store, note_draft_store};
use crate::utils::custom_emoji::EmojiSelection;
use crate::utils::mention_ranges::{materialize_mentions, shift_ranges, MentionRange};
use dioxus::prelude::*;
use std::rc::Rc;

const MAX_LENGTH: usize = 5000;

pub struct ComposerConfig {
    pub draft_context: Option<String>,
    pub initial_content: String,
    /// Pretty mention ranges restored alongside the initial content (e.g.
    /// from a persisted draft). Raw-bech32 content with no ranges is left
    /// untouched.
    pub initial_mentions: Vec<MentionRange>,
}

#[derive(Clone, Copy, PartialEq)]
pub struct UseComposerEditor {
    pub content: Signal<String>,
    pub cursor_position: Signal<usize>,
    /// Stable DOM id of the composer textarea. Owned here so imperative DOM
    /// mutations (insert/clear) can address the element directly.
    pub textarea_id: Signal<Rc<String>>,
    /// Pretty `@Name` mention ranges. The DOM text shows labels; wire content
    /// is produced by `materialized_content()`.
    pub mentions: Signal<Vec<MentionRange>>,
    pub show_media_uploader: Signal<bool>,
    pub show_poll_modal: Signal<bool>,
    pub is_publishing: Signal<bool>,
    pub is_sensitive: Signal<bool>,
    pub sensitive_reason: Signal<String>,
    pub is_protected: Signal<bool>,
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
    let textarea_id: Signal<Rc<String>> =
        use_signal(|| Rc::new(format!("composer-textarea-{}", uuid::Uuid::new_v4())));
    let mentions: Signal<Vec<MentionRange>> =
        use_signal(move || config.initial_mentions);
    let show_media_uploader = use_signal(|| false);
    let show_poll_modal = use_signal(|| false);
    let is_publishing = use_signal(|| false);
    let is_sensitive = use_signal(|| false);
    let sensitive_reason = use_signal(String::new);
    let is_protected = use_signal(|| false);

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
    let mut last_draft_save = use_signal(|| 0u64);
    use_effect(move || {
        let ctx = draft_context_signal.read().clone();
        if let Some(ref ctx) = ctx {
            let text = content.read();
            if !text.is_empty() {
                let mentions_snapshot = mentions.read().clone();
                let now = crate::platform::timestamp::now_secs();
                if now.saturating_sub(*last_draft_save.peek()) < 2 {
                    return;
                }
                last_draft_save.set(now);
                if let Some(pk) = auth_store::get_pubkey() {
                    let ctx = ctx.clone();
                    let text = text.clone();
                    spawn(async move {
                        note_draft_store::save_note_draft(
                            &pk,
                            &ctx,
                            &note_draft_store::NoteDraft {
                                content: text,
                                saved_at: crate::platform::timestamp::now_secs(),
                                mentions: mentions_snapshot,
                            },
                        );
                    });
                }
            }
        }
    });

    {
        let content_clone = content;
        let mentions_clone = mentions;
        let draft_ctx_clone = draft_context_signal;
        use_drop(move || {
            let ctx = draft_ctx_clone.peek().clone();
            if let Some(ref ctx) = ctx {
                let text = content_clone.peek().clone();
                if !text.is_empty() {
                    let mentions_snapshot = mentions_clone.peek().clone();
                    if let Some(pk) = auth_store::get_pubkey() {
                        let ctx = ctx.clone();
                        spawn(async move {
                            note_draft_store::save_note_draft(
                                &pk,
                                &ctx,
                                &note_draft_store::NoteDraft {
                                    content: text,
                                    saved_at: crate::platform::timestamp::now_secs(),
                                    mentions: mentions_snapshot,
                                },
                            );
                        });
                    }
                }
            }
        });
    }

    UseComposerEditor {
        content,
        cursor_position,
        textarea_id,
        mentions,
        show_media_uploader,
        show_poll_modal,
        is_publishing,
        is_sensitive,
        sensitive_reason,
        is_protected,
        char_count,
        remaining,
        is_over_limit,
        show_warning,
        can_publish,
        counter_color,
        draft_context: draft_context_signal,
    }
}

/// Create a stable textarea id for composers that use `MentionAutocomplete`
/// directly (without the full editor hook) and need to perform imperative DOM
/// writes (e.g. clearing after publish, since the textarea is uncontrolled).
pub fn new_textarea_id(prefix: &str) -> Rc<String> {
    Rc::new(format!("{}-textarea-{}", prefix, uuid::Uuid::new_v4()))
}

/// Compute the result of inserting `text` at `caret` (UTF-8 byte index) into `content`.
/// Returns the new content and the new caret position (just after the insertion).
pub fn apply_insert(content: &str, caret: usize, text: &str) -> (String, usize) {
    let pos = to_char_boundary(content, caret);
    let mut out = String::with_capacity(content.len() + text.len());
    out.push_str(&content[..pos]);
    out.push_str(text);
    out.push_str(&content[pos..]);
    (out, pos + text.len())
}

/// Wrap `text` in single spaces when the characters adjacent to the insertion
/// point (in `content` at `caret`) are not already whitespace.
pub fn spacing_wrap(content: &str, caret: usize, text: &str) -> String {
    let pos = to_char_boundary(content, caret);
    let mut spaced = text.to_string();
    if pos > 0 {
        if let Some(prev) = content[..pos].chars().last() {
            if !prev.is_whitespace() {
                spaced.insert(0, ' ');
            }
        }
    }
    if pos < content.len() {
        if let Some(next) = content[pos..].chars().next() {
            if !next.is_whitespace() {
                spaced.push(' ');
            }
        }
    }
    spaced
}

impl UseComposerEditor {
    /// Insert `text` at the current caret, applying smart spacing when requested.
    ///
    /// The textarea is uncontrolled, so the DOM value + caret are written
    /// imperatively (in one atomic step) and the Rust mirror signal is updated
    /// alongside. The caret is read fresh from the DOM when possible so it never
    /// goes stale (async tracking on WebView platforms lags under fast typing).
    fn insert_impl(&self, text: String, with_spacing: bool) {
        let mut content = self.content;
        let mut cursor_position = self.cursor_position;
        let id = (*self.textarea_id.read()).clone();
        let tracked_caret = *cursor_position.read();

        #[cfg(feature = "web")]
        {
            let current = content.read().clone();
            let caret = editor_dom::read_caret_sync(&id, &current)
                .unwrap_or_else(|| to_char_boundary(&current, tracked_caret));
            let ins = if with_spacing {
                spacing_wrap(&current, caret, &text)
            } else {
                text
            };
            let (new_content, new_caret) = apply_insert(&current, caret, &ins);
            content.set(new_content.clone());
            cursor_position.set(new_caret);
            editor_dom::write_value_and_caret_sync(&id, &new_content, new_caret);
        }

        #[cfg(not(feature = "web"))]
        {
            spawn(async move {
                let current = content.read().clone();
                let caret = editor_dom::read_caret(&id, &current, tracked_caret).await;
                let ins = if with_spacing {
                    spacing_wrap(&current, caret, &text)
                } else {
                    text
                };
                let (new_content, new_caret) = apply_insert(&current, caret, &ins);
                content.set(new_content.clone());
                cursor_position.set(new_caret);
                editor_dom::write_value_and_caret(&id, &new_content, new_caret).await;
            });
        }
    }

    pub fn insert_at_cursor(&self, text: String) {
        self.insert_impl(text, false);
    }

    pub fn insert_with_spacing(&self, text: String) {
        self.insert_impl(text, true);
    }

    /// Handle a plain user text input event (`oninput` from the textarea).
    /// Updates the mirror and minimal-diff shifts every mention range; edits
    /// that overlap a range demote it to plain text.
    pub fn handle_text_input(&self, new_value: String) {
        let mut content = self.content;
        let mut mentions = self.mentions;
        let old_value = content.peek().clone();
        let old_ranges = mentions.peek().clone();
        let shifted = shift_ranges(&old_ranges, &old_value, &new_value);
        mentions.set(shifted);
        content.set(new_value);
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
        let mut cursor_position = self.cursor_position;
        let mut mentions = self.mentions;
        content.set(String::new());
        cursor_position.set(0);
        mentions.set(Vec::new());
        let id = (*self.textarea_id.read()).clone();
        #[cfg(feature = "web")]
        {
            editor_dom::write_value_and_caret_sync(&id, "", 0);
        }
        #[cfg(not(feature = "web"))]
        {
            spawn(async move {
                editor_dom::write_value_and_caret(&id, "", 0).await;
            });
        }
        let mut show_media_uploader = self.show_media_uploader;
        let mut is_protected = self.is_protected;
        show_media_uploader.set(false);
        is_protected.set(false);
    }

    pub fn clear_draft(&self) {
        let ctx = self.draft_context.read().clone();
        if let Some(ref ctx) = ctx {
            if let Some(pk) = auth_store::get_pubkey() {
                note_draft_store::clear_note_draft(&pk, ctx);
            }
        }
    }

    /// Wire-format content: pretty `@Name` labels replaced by canonical
    /// `nostr:nprofile1…` URIs. Ranges are validated against the current
    /// text, so stale ranges are simply skipped.
    pub fn materialized_content(&self) -> String {
        let content = self.content.read().clone();
        let mentions = self.mentions.read().clone();
        materialize_mentions(&content, &mentions)
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

pub fn restore_draft_or_empty(draft_context: &str) -> (String, Vec<MentionRange>) {
    auth_store::get_pubkey()
        .and_then(|pk| note_draft_store::read_note_draft(&pk, draft_context))
        .map(|d| (d.content, d.mentions))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_insert_middle() {
        let (out, caret) = apply_insert("hello world", 5, " there");
        assert_eq!(out, "hello there world");
        assert_eq!(caret, 11);
    }

    #[test]
    fn apply_insert_empty() {
        let (out, caret) = apply_insert("", 0, "abc");
        assert_eq!(out, "abc");
        assert_eq!(caret, 3);
    }

    #[test]
    fn apply_insert_end() {
        let (out, caret) = apply_insert("abc", 3, "d");
        assert_eq!(out, "abcd");
        assert_eq!(caret, 4);
    }

    #[test]
    fn apply_insert_snaps_to_char_boundary() {
        // "😀" is 4 bytes; caret inside the emoji snaps back to 0
        let (out, caret) = apply_insert("\u{1F600}x", 2, "a");
        assert_eq!(out, "a\u{1F600}x");
        assert_eq!(caret, 1);
    }

    #[test]
    fn spacing_wrap_adds_spaces_both_sides() {
        assert_eq!(spacing_wrap("ab", 1, "X"), " X ");
    }

    #[test]
    fn spacing_wrap_respects_existing_whitespace() {
        assert_eq!(spacing_wrap("a b", 2, "X"), "X "); // prev is space, next is 'b'
        assert_eq!(spacing_wrap("a ", 2, "X"), "X"); // next is end
        assert_eq!(spacing_wrap("a b", 1, "X"), " X"); // next char is space
        assert_eq!(spacing_wrap(" ", 0, "X"), "X"); // prev is start
    }

    #[test]
    fn to_char_boundary_snap() {
        assert_eq!(to_char_boundary("h\u{e9}llo", 100), 6); // h(1) + é(2) + l + l + o
        assert_eq!(to_char_boundary("h\u{e9}llo", 1), 1);
        assert_eq!(to_char_boundary("h\u{e9}llo", 2), 1); // inside é (2 bytes)
        assert_eq!(to_char_boundary("\u{1F600}", 2), 0); // inside emoji
    }
}
