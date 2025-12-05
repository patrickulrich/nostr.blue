// Event utility functions
// Helper functions for working with Nostr events

use nostr_sdk::Kind;

/// Check if an event is a voice message (Kind::VoiceMessage or Kind::VoiceMessageReply)
pub fn is_voice_message(event: &nostr_sdk::Event) -> bool {
    event.kind == Kind::VoiceMessage || event.kind == Kind::VoiceMessageReply
}
