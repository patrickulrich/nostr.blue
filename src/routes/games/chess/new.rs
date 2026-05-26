use dioxus::prelude::*;
use nostr_sdk::{FromBech32, PublicKey};

use crate::routes::Route;
use crate::stores::chess::types::ChessColor;
use crate::stores::chess::publish;

#[component]
pub fn ChessGameNew() -> Element {
    let mut selected_color = use_signal(|| ChessColor::White);
    let mut opponent_input = use_signal(String::new);
    let mut is_creating = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let nav = navigator();

    let is_authenticated = crate::stores::auth_store::AUTH_STATE.read().is_authenticated;

    rsx! {
        div { class: "max-w-lg mx-auto px-4 py-6 space-y-6",
            div { class: "flex items-center gap-3",
                button {
                    class: "p-2 hover:bg-accent rounded-lg transition",
                    onclick: move |_| { let _ = nav.push(Route::ChessHome {}); },
                    "←"
                }
                h1 { class: "text-xl font-bold text-foreground", "New Chess Game" }
            }

            if !is_authenticated {
                div { class: "rounded-xl border border-border bg-card p-4 text-center",
                    p { class: "text-muted-foreground", "Log in to create a chess game." }
                }
            } else {
                div { class: "space-y-6",
                    // Color selection
                    div { class: "space-y-2",
                        label { class: "text-sm font-medium text-foreground", "Play as" }
                        div { class: "flex gap-3",
                            button {
                                class: if *selected_color.read() == ChessColor::White {
                                    "flex-1 py-3 rounded-xl border-2 border-primary bg-primary/10 text-foreground font-medium transition"
                                } else {
                                    "flex-1 py-3 rounded-xl border border-border bg-card text-foreground hover:bg-accent/5 transition"
                                },
                                onclick: move |_| selected_color.set(ChessColor::White),
                                "♔ White"
                            }
                            button {
                                class: if *selected_color.read() == ChessColor::Black {
                                    "flex-1 py-3 rounded-xl border-2 border-primary bg-primary/10 text-foreground font-medium transition"
                                } else {
                                    "flex-1 py-3 rounded-xl border border-border bg-card text-foreground hover:bg-accent/5 transition"
                                },
                                onclick: move |_| selected_color.set(ChessColor::Black),
                                "♚ Black"
                            }
                        }
                    }

                    // Opponent (optional)
                    div { class: "space-y-2",
                        label { class: "text-sm font-medium text-foreground", "Opponent (optional)" }
                        input {
                            r#type: "text",
                            class: "w-full px-3 py-2 rounded-xl border border-border bg-card text-foreground text-sm placeholder:text-muted-foreground/50 focus:outline-none focus:ring-2 focus:ring-primary/50",
                            placeholder: "npub or hex pubkey...",
                            value: opponent_input.read().clone(),
                            oninput: move |e| opponent_input.set(e.value()),
                        }
                        p { class: "text-xs text-muted-foreground",
                            "Leave empty for an open challenge anyone can accept."
                        }
                    }

                    if let Some(err) = error.read().as_ref() {
                        div { class: "rounded-xl border border-red-500/30 bg-red-500/10 p-3",
                            p { class: "text-sm text-red-500", {err.clone()} }
                        }
                    }

                    button {
                        class: if *is_creating.read() {
                            "w-full py-3 rounded-xl bg-primary/50 text-primary-foreground/50 font-medium text-sm cursor-not-allowed"
                        } else {
                            "w-full py-3 rounded-xl bg-primary text-primary-foreground font-medium text-sm hover:bg-primary/90 transition"
                        },
                        disabled: *is_creating.read(),
                        onclick: move |_| {
                            let color = *selected_color.read();
                            let opponent_str = opponent_input.read().clone();
                            let opponent_pk = if opponent_str.is_empty() {
                                None
                            } else {
                                parse_pubkey(&opponent_str)
                            };

                            if !opponent_str.is_empty() && opponent_pk.is_none() {
                                error.set(Some("Invalid opponent pubkey".to_string()));
                                return;
                            }

                            is_creating.set(true);
                            error.set(None);

                            let nav = nav;
                            spawn(async move {
                                match publish::publish_challenge(color, opponent_pk).await {
                                    Ok(event_id) => {
                                        let _ = nav.push(Route::ChessGameDetail {
                                            game_id: event_id.to_hex(),
                                        });
                                    }
                                    Err(e) => {
                                        error.set(Some(e));
                                        is_creating.set(false);
                                    }
                                }
                            });
                        },
                        if *is_creating.read() { "Creating..." } else { "Create Game" }
                    }
                }
            }
        }
    }
}

fn parse_pubkey(input: &str) -> Option<PublicKey> {
    if input.starts_with("npub1") {
        PublicKey::from_bech32(input).ok()
    } else {
        PublicKey::from_hex(input).ok()
    }
}
