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
    pub on_raise_hand: EventHandler<()>,
    pub on_leave: EventHandler<()>,
    pub on_request_speak: EventHandler<()>,
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

                if !props.is_publishing && !props.is_host {
                    button {
                        class: if props.hand_raised {
                            "w-14 h-14 rounded-full bg-yellow-500/20 text-yellow-500 flex items-center justify-center transition hover:bg-yellow-500/30"
                        } else {
                            "w-14 h-14 rounded-full bg-muted text-foreground flex items-center justify-center transition hover:bg-accent"
                        },
                        onclick: move |_: Event<MouseData>| {
                            props.on_raise_hand.call(());
                        },
                        HandIcon { class: "w-6 h-6".to_string() }
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
