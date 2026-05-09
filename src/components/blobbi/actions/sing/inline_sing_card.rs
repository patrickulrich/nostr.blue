use dioxus::prelude::*;

use crate::components::blobbi::actions::action_types::BlobbiActionType;
use crate::components::blobbi::actions::care_actions::execute_blobbi_action;
use crate::components::blobbi::actions::sing::lyrics::{random_lyrics, LyricSet};
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::stores::blobbi_store;

#[derive(Clone, PartialEq)]
enum SingState {
    ShowingLyrics,
    Recording,
    Processing,
    Done,
}

#[component]
pub fn InlineSingCard(
    blobbi: BlobbiCompanion,
    on_close: EventHandler,
) -> Element {
    let mut state = use_signal(|| SingState::ShowingLyrics);
    let elapsed = use_signal(|| 0.0_f64);
    let mut lyric_set = use_signal(|| None::<&'static LyricSet>);
    let mut is_mounted = use_signal(|| true);
    let mut acting = use_signal(|| false);

    if lyric_set.read().is_none() {
        let set = random_lyrics();
        lyric_set.set(Some(set));
    }

    let current_lyrics = (*lyric_set.read()).unwrap_or(random_lyrics());

    use_drop(move || {
        is_mounted.set(false);
    });

    let do_sing = move |_| {
        if acting() {
            return;
        }
        acting.set(true);
        state.set(SingState::Processing);
        crate::components::blobbi::rooms::reaction_state::set_reaction(
            crate::components::blobbi::rooms::reaction_state::BlobbiReactionState::Singing,
        );
        let blobbi = blobbi.clone();
        spawn(async move {
            match execute_blobbi_action(&blobbi, BlobbiActionType::Sing).await {
                Ok(updated) => blobbi_store::update_blobbi_in_collection(&updated),
                Err(e) => log::error!("Sing action failed: {}", e),
            }
            if *is_mounted.read() {
                state.set(SingState::Done);
            }
            crate::components::blobbi::rooms::reaction_state::reset_reaction();
            acting.set(false);
        });
    };

    let close_handler = move |_| {
        on_close.call(());
    };

    rsx! {
        div { class: "fixed inset-0 z-50 flex items-end justify-center bg-black/50 backdrop-blur-sm",
            div { class: "w-full max-w-lg bg-card border border-border rounded-t-2xl p-5 space-y-4 animate-slide-up",
                div { class: "flex items-center justify-between",
                    h3 { class: "text-lg font-semibold",
                        "🎤 {current_lyrics.title}"
                    }
                    button {
                        class: "p-1 hover:bg-accent rounded-lg transition",
                        onclick: close_handler,
                        "✕"
                    }
                }

                div { class: "bg-muted/50 rounded-xl p-4",
                    div { class: "text-sm leading-relaxed text-center space-y-1",
                        for line in current_lyrics.lines {
                            p { "{line}" }
                        }
                    }
                }

                match state.read().clone() {
                    SingState::ShowingLyrics => rsx! {
                        div { class: "text-xs text-muted-foreground text-center",
                            "Sing along and tap when you're ready!"
                        }
                        div { class: "flex gap-2",
                            button {
                                class: "flex-1 py-2.5 rounded-xl bg-blue-500 hover:bg-blue-600 text-white font-medium transition",
                                onclick: do_sing,
                                "🎤 Sing!"
                            }
                            button {
                                class: "py-2.5 px-4 rounded-xl bg-muted hover:bg-accent text-muted-foreground transition",
                                onclick: close_handler,
                                "Cancel"
                            }
                        }
                    },
                    SingState::Recording => rsx! {
                        div { class: "flex items-center justify-center gap-2",
                            div { class: "w-3 h-3 bg-red-500 rounded-full animate-pulse" }
                            span { class: "font-mono text-lg",
                                "{format_elapsed(*elapsed.read())}"
                            }
                        }
                        div { class: "flex items-center gap-1 justify-center h-10",
                            for i in 0..20 {
                                div {
                                    key: "{i}",
                                    class: "w-1 bg-primary rounded-full animate-pulse",
                                    style: "height: {((i % 5) + 1) * 6}px; animation-delay: {i * 80}ms;",
                                }
                            }
                        }
                    },
                    SingState::Processing => rsx! {
                        div { class: "flex items-center justify-center gap-2 py-4",
                            div { class: "animate-spin text-2xl", "🎤" }
                            span { class: "text-muted-foreground", "Singing to your Blobbi..." }
                        }
                    },
                    SingState::Done => rsx! {
                        div { class: "text-center space-y-3",
                            div { class: "text-2xl", "🎵" }
                            p { class: "text-green-500 font-medium", "Your Blobbi loved it!" }
                            button {
                                class: "px-6 py-2 rounded-xl bg-muted hover:bg-accent transition",
                                onclick: close_handler,
                                "Close"
                            }
                        }
                    },
                }
            }
        }
    }
}

fn format_elapsed(secs: f64) -> String {
    let mins = (secs / 60.0).floor() as u32;
    let s = (secs % 60.0).floor() as u32;
    format!("{:02}:{:02}", mins, s)
}
