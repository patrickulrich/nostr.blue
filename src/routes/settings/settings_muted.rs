use crate::hooks::{use_nostr_resource, NostrResourceState};
use crate::routes::Route;
use crate::stores::nostr_client;
use dioxus::prelude::*;

#[component]
pub fn SettingsMuted() -> Element {
    let muted_posts = use_nostr_resource(move || {
        async move {
            nostr_client::get_muted_posts()
                .await
                .map_err(|e| format!("Failed to load muted posts: {}", e))
        }
    });
    let mut muted_list = use_signal(Vec::<String>::new);
    {
        let state = muted_posts.state();
        use_effect(move || {
            if let NostrResourceState::Loaded(data) = &*state.read() {
                muted_list.set(data.clone());
            }
        });
    }
    let handle_unmute = move |event_id: String| {
        let event_id_clone = event_id.clone();
        spawn(async move {
            match nostr_client::unmute_post(event_id).await {
                Ok(_) => {
                    log::info!("Post unmuted successfully");
                    muted_list.with_mut(|posts| {
                        posts.retain(|p| p != &event_id_clone);
                    });
                }
                Err(e) => {
                    log::error!("Failed to unmute post: {}", e);
                }
            }
        });
    };
    let muted_words = use_nostr_resource(move || {
        async move {
            nostr_client::get_muted_words()
                .await
                .map_err(|e| format!("Failed to load muted words: {}", e))
        }
    });
    let mut words_list = use_signal(Vec::<String>::new);
    {
        let state = muted_words.state();
        use_effect(move || {
            if let NostrResourceState::Loaded(data) = &*state.read() {
                words_list.set(data.clone());
            }
        });
    }
    let mut new_word = use_signal(String::new);
    let handle_add_word = move || {
        let word = new_word.read().trim().to_string();
        if word.is_empty() {
            return;
        }
        let word_clone = word.clone();
        let word_for_list = word.clone();
        spawn(async move {
            match nostr_client::mute_word(word_clone).await {
                Ok(_) => {
                    log::info!("Word muted successfully");
                    new_word.set(String::new());
                    words_list.with_mut(|words| {
                        words.push(word_for_list);
                    });
                }
                Err(e) => {
                    log::error!("Failed to mute word: {}", e);
                }
            }
        });
    };
    let handle_remove_word = move |word: String| {
        let word_clone = word.clone();
        spawn(async move {
            match nostr_client::unmute_word(word).await {
                Ok(_) => {
                    log::info!("Word unmuted successfully");
                    words_list.with_mut(|words| {
                        words.retain(|w| w.to_lowercase() != word_clone.to_lowercase());
                    });
                }
                Err(e) => {
                    log::error!("Failed to unmute word: {}", e);
                }
            }
        });
    };
    rsx! {
        div { class: "max-w-2xl mx-auto px-4 py-6",
            div { class: "mb-6",
                Link {
                    to: Route::Settings {},
                    class: "text-sm text-primary hover:underline mb-4 inline-block",
                    "\u{2190} Back to Settings"
                }
                h1 { class: "text-2xl font-bold", "Muted Content" }
                p { class: "text-muted-foreground mt-2", "Manage muted posts, blocked users, and filtered words" }
            }
            // Muted Posts Section
            div { class: "bg-background border border-border rounded-lg shadow-xs mb-6",
                div { class: "px-4 py-3 border-b border-border",
                    h2 { class: "text-lg font-semibold", "Muted Posts" }
                }
                match &*muted_posts.state().read() {
                    NostrResourceState::Initializing | NostrResourceState::Loading => rsx! {
                        div { class: "p-8 text-center",
                            div { class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto mb-4" }
                            p { class: "text-muted-foreground", "Loading muted posts..." }
                        }
                    },
                    NostrResourceState::AuthRequired => rsx! {
                        div { class: "p-8 text-center",
                            p { class: "text-muted-foreground", "Sign in required" }
                        }
                    },
                    NostrResourceState::Error(e) => rsx! {
                        div { class: "p-8",
                            div { class: "bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-red-600",
                                "{e}"
                            }
                        }
                    },
                    NostrResourceState::Loaded(_) if muted_list.read().is_empty() => rsx! {
                        div { class: "p-8 text-center",
                            h3 { class: "text-lg font-semibold mb-2", "No muted posts" }
                            p { class: "text-muted-foreground",
                                "Posts you mute or report will appear here"
                            }
                        }
                    },
                    NostrResourceState::Loaded(_) => rsx! {
                        div { class: "divide-y divide-border",
                            for event_id in muted_list.read().iter() {
                                div {
                                    key: "{event_id}",
                                    class: "p-4 flex items-center justify-between hover:bg-accent/50 transition",
                                    div { class: "flex-1 min-w-0",
                                        Link {
                                            to: Route::AddressViewer {
                                                address: crate::utils::nip19_urls::note_route_id(event_id, None),
                                            },
                                            class: "font-mono text-sm text-muted-foreground hover:text-foreground hover:underline truncate block",
                                            if event_id.len() > 40 {
                                                "{&event_id[..16]}...{&event_id[event_id.len()-16..]}"
                                            } else {
                                                "{event_id}"
                                            }
                                        }
                                    }
                                    button {
                                        class: "px-4 py-2 text-sm bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg transition",
                                        onclick: {
                                            let eid = event_id.clone();
                                            move |_| handle_unmute(eid.clone())
                                        },
                                        "Unmute"
                                    }
                                }
                            }
                        }
                        div { class: "p-4 bg-accent/30 text-sm text-muted-foreground text-center border-t border-border",
                            {
                                let count = muted_list.read().len();
                                let word = if count == 1 { "post" } else { "posts" };
                                format!("{} muted {}", count, word)
                            }
                        }
                    },
                }
            }
            // Muted Words Section
            div { class: "bg-background border border-border rounded-lg shadow-xs",
                div { class: "px-4 py-3 border-b border-border",
                    h2 { class: "text-lg font-semibold", "Muted Words" }
                    p { class: "text-sm text-muted-foreground mt-1",
                        "Posts containing these words will be hidden from your feed"
                    }
                }
                match &*muted_words.state().read() {
                    NostrResourceState::Initializing | NostrResourceState::Loading => rsx! {
                        div { class: "p-8 text-center",
                            div { class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto mb-4" }
                            p { class: "text-muted-foreground", "Loading muted words..." }
                        }
                    },
                    NostrResourceState::AuthRequired => rsx! {
                        div { class: "p-8 text-center",
                            p { class: "text-muted-foreground", "Sign in required" }
                        }
                    },
                    NostrResourceState::Error(e) => rsx! {
                        div { class: "p-8",
                            div { class: "bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-red-600",
                                "{e}"
                            }
                        }
                    },
                    NostrResourceState::Loaded(_) => rsx! {
                        // Add word input
                        div { class: "p-4 border-b border-border",
                            div { class: "flex gap-2",
                                input {
                                    r#type: "text",
                                    class: "flex-1 px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary/50",
                                    placeholder: "Add a word or phrase to mute...",
                                    value: "{new_word}",
                                    oninput: move |e: Event<FormData>| {
                                        new_word.set(e.value());
                                    },
                                    onkeydown: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter {
                                            e.prevent_default();
                                            handle_add_word();
                                        }
                                    },
                                }
                                button {
                                    class: "px-4 py-2 text-sm bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg transition whitespace-nowrap",
                                    onclick: move |_| handle_add_word(),
                                    "Add"
                                }
                            }
                        }
                        if words_list.read().is_empty() {
                            div { class: "p-8 text-center",
                                h3 { class: "text-lg font-semibold mb-2", "No muted words" }
                                p { class: "text-muted-foreground",
                                    "Add words to filter from your feed"
                                }
                            }
                        } else {
                            div { class: "divide-y divide-border",
                                for word in words_list.read().iter() {
                                    div {
                                        key: "{word}",
                                        class: "p-4 flex items-center justify-between hover:bg-accent/50 transition",
                                        span { class: "text-sm font-medium", "{word}" }
                                        button {
                                            class: "px-4 py-2 text-sm bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg transition",
                                            onclick: {
                                                let w = word.clone();
                                                move |_| handle_remove_word(w.clone())
                                            },
                                            "Remove"
                                        }
                                    }
                                }
                            }
                            div { class: "p-4 bg-accent/30 text-sm text-muted-foreground text-center border-t border-border",
                                {
                                    let count = words_list.read().len();
                                    let word_label = if count == 1 { "word" } else { "words" };
                                    format!("{} muted {}", count, word_label)
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
