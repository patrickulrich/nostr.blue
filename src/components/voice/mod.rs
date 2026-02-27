//! Voice message components
//!
//! This module contains components for Nostr voice messages
//! including voice message cards, recorders, and reply composers.
pub mod message_card;
#[cfg(feature = "web")]
pub mod recorder;
pub mod reply_composer;
pub use message_card::VoiceMessageCard;
#[cfg(feature = "web")]
pub use recorder::VoiceRecorder;
pub use reply_composer::VoiceReplyComposer;

/// Stub VoiceRecorder for native platforms (voice recording not yet supported)
#[cfg(not(feature = "web"))]
pub mod recorder_stub {
    use dioxus::prelude::*;

    #[component]
    pub fn VoiceRecorder(
        on_recording_complete: EventHandler<(Vec<u8>, f64, Vec<u8>, String)>,
    ) -> Element {
        rsx! {
            div { class: "p-4 border border-border rounded-lg text-muted-foreground text-sm",
                "Voice recording is not yet available on this platform."
            }
        }
    }
}
#[cfg(not(feature = "web"))]
pub use recorder_stub::VoiceRecorder;
