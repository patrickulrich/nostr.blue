use crate::components::icons::{
    HandIcon, MicrophoneIcon, MicrophoneOffIcon, PhoneOffIcon,
};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ActionBarProps {
    pub is_connected: bool,
    pub is_muted: bool,
    pub is_publishing: bool,
    pub is_host: bool,
    pub hand_raised: bool,
    #[props(default = 0)]
    pub speaker_request_count: u32,
    pub on_toggle_mute: EventHandler<()>,
    /// Audience-only button that toggles the kind 10312 `hand=1` presence
    /// flag, which surfaces the user in the host's SpeakerQueue. The host
    /// promotes them via Phase 1.3's 30312 role flip.
    ///
    /// This IS the "Request to Speak" affordance — there is no separate
    /// request-to-speak flow.
    pub on_raise_hand: EventHandler<()>,
    pub on_leave: EventHandler<()>,
}

#[component]
pub fn ActionBar(props: ActionBarProps) -> Element {
    rsx! {
        div { class: "fixed bottom-0 left-0 right-0 z-50 bg-background/95 backdrop-blur-md border-t border-border",
            div { class: "flex items-center justify-center gap-6 px-6 py-4 max-w-lg mx-auto",
                button {
                    class: if props.is_muted {
                        "w-14 h-14 rounded-full bg-red-500/20 text-red-500 flex items-center justify-center transition hover:bg-red-500/30"
                    } else {
                        "w-14 h-14 rounded-full bg-muted text-foreground flex items-center justify-center transition hover:bg-accent"
                    },
                    onclick: move |_: Event<MouseData>| {
                        props.on_toggle_mute.call(());
                    },
                    if props.is_muted {
                        MicrophoneOffIcon { class: "w-6 h-6".to_string() }
                    } else {
                        MicrophoneIcon { class: "w-6 h-6".to_string() }
                    }
                }

                // Audience-only: "Request to Speak" / "Cancel Request" via the
                // hand-raise presence flag. Hosts and active speakers don't
                // see this control.
                if !props.is_publishing && !props.is_host {
                    button {
                        class: if props.hand_raised {
                            "flex flex-col items-center justify-center px-3 h-14 rounded-xl bg-yellow-500/20 text-yellow-600 dark:text-yellow-400 transition hover:bg-yellow-500/30"
                        } else {
                            "flex flex-col items-center justify-center px-3 h-14 rounded-xl bg-muted text-foreground transition hover:bg-accent"
                        },
                        onclick: move |_: Event<MouseData>| {
                            props.on_raise_hand.call(());
                        },
                        HandIcon { class: "w-5 h-5".to_string() }
                        span { class: "text-[10px] font-medium mt-0.5",
                            if props.hand_raised { "Cancel" } else { "Request" }
                        }
                    }
                }

                if props.is_host && props.speaker_request_count > 0 {
                    div { class: "relative",
                        HandIcon { class: "w-6 h-6 text-yellow-500".to_string() }
                        span { class: "absolute -top-1 -right-2 w-4 h-4 bg-yellow-500 text-white text-[10px] font-bold rounded-full flex items-center justify-center",
                            "{props.speaker_request_count}"
                        }
                    }
                }

                button {
                    class: "w-14 h-14 rounded-full bg-red-500 text-white flex items-center justify-center transition hover:bg-red-600",
                    onclick: move |_: Event<MouseData>| {
                        props.on_leave.call(());
                    },
                    PhoneOffIcon { class: "w-6 h-6".to_string() }
                }
            }
        }
    }
}
