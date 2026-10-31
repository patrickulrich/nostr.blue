use nostr::event::{Event, Kind, Tag};
use nostr::nips::nip10::Nip10Tag;

/// Check if an event is a voice message (Kind::VoiceMessage or Kind::VoiceMessageReply)
pub fn is_voice_message(event: &nostr::event::Event) -> bool {
    event.kind == Kind::VoiceMessage || event.kind == Kind::VoiceMessageReply
}

/// Check if a tag is an `e` tag with a NIP-10 `root` marker
/// (replacement for the removed 0.44 `Tag::is_root()` helper).
pub fn tag_is_root(tag: &Tag) -> bool {
    Nip10Tag::try_from(tag).map(|t| t.is_root()).unwrap_or(false)
}

/// Check if a tag is an `e` tag with a NIP-10 `reply` marker
/// (replacement for the removed 0.44 `Tag::is_reply()` helper).
pub fn tag_is_reply(tag: &Tag) -> bool {
    Nip10Tag::try_from(tag).map(|t| t.is_reply()).unwrap_or(false)
}

/// Check if an event carries NIP-10 root/reply markers, i.e. it is a thread reply.
/// Mentions (`e` tags without markers) are not counted.
pub fn is_thread_reply(event: &Event) -> bool {
    event
        .tags
        .iter()
        .any(|tag| tag_is_root(tag) || tag_is_reply(tag))
}
