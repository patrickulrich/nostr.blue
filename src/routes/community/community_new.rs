//! Create New Community Page
//! Form for creating a new NIP-72 moderated community
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::community_store::create_community;
use crate::stores::nostr_client::{self, HAS_SIGNER};
use dioxus::prelude::*;
#[component]
pub fn CommunityNew() -> Element {
    let mut identifier = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut image_url = use_signal(String::new);
    let mut rules = use_signal(String::new);
    let mut moderator_input = use_signal(String::new);
    let mut moderators = use_signal(Vec::<String>::new);
    let mut creating = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let has_signer = *HAS_SIGNER.read();
    let nav = navigator();
    let add_moderator = move |_| {
        let pubkey = moderator_input.read().trim().to_string();
        if pubkey.is_empty() {
            return;
        }
        if pubkey.len() != 64 || !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
            error.set(Some(
                "Invalid pubkey format. Please enter a 64-character hex pubkey.".to_string(),
            ));
            return;
        }
        if !moderators.read().contains(&pubkey) {
            moderators.write().push(pubkey);
            moderator_input.set(String::new());
            error.set(None);
        }
    };
    let mut remove_moderator = move |pubkey: String| {
        moderators.write().retain(|p| p != &pubkey);
    };
    let mut handle_submit = move |_| {
        let id = identifier.read().trim().to_string();
        let n = name.read().trim().to_string();
        let desc = description.read().trim().to_string();
        let img = image_url.read().trim().to_string();
        let r = rules.read().trim().to_string();
        let mods = moderators.read().clone();
        if id.is_empty() {
            error.set(Some("Identifier is required".to_string()));
            return;
        }
        if n.is_empty() {
            error.set(Some("Name is required".to_string()));
            return;
        }
        if !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            error.set(Some(
                "Identifier can only contain letters, numbers, dashes, and underscores".to_string(),
            ));
            return;
        }
        creating.set(true);
        error.set(None);
        spawn(async move {
            match create_community(
                &id,
                &n,
                if desc.is_empty() { None } else { Some(&desc) },
                if img.is_empty() { None } else { Some(&img) },
                if r.is_empty() { None } else { Some(&r) },
                mods,
            )
            .await
            {
                Ok(_event_id) => {
                    log::info!("Community created successfully");
                    nav.push(Route::Communities {});
                }
                Err(e) => {
                    log::error!("Failed to create community: {}", e);
                    error.set(Some(e));
                    creating.set(false);
                }
            }
        });
    };
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition",
                        onclick: move |_| {
                            let nav = navigator();
                            nav.go_back();
                        },
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            line {
                                x1: "19",
                                y1: "12",
                                x2: "5",
                                y2: "12",
                            }
                            polyline { points: "12 19 5 12 12 5" }
                        }
                    }
                    h1 { class: "text-xl font-bold", "Create Community" }
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if !has_signer {
                div { class: "p-4",
                    div { class: "p-4 bg-yellow-100 dark:bg-yellow-900 text-yellow-800 dark:text-yellow-200 rounded-lg",
                        "You need to sign in to create a community."
                    }
                }
            } else {
                div { class: "p-4 max-w-2xl mx-auto",
                    if let Some(ref err) = *error.read() {
                        div { class: "mb-4 p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                            "{err}"
                        }
                    }
                    form {
                        class: "space-y-6",
                        onsubmit: move |e| {
                            e.prevent_default();
                            handle_submit(());
                        },
                        div {
                            label { class: "block text-sm font-medium mb-2", "Identifier *" }
                            input {
                                class: "w-full p-3 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                r#type: "text",
                                placeholder: "my-community",
                                value: "{identifier}",
                                disabled: *creating.read(),
                                oninput: move |evt| identifier.set(evt.value()),
                            }
                            p { class: "mt-1 text-xs text-muted-foreground",
                                "Unique identifier for your community. Use lowercase letters, numbers, dashes, or underscores."
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Name *" }
                            input {
                                class: "w-full p-3 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                r#type: "text",
                                placeholder: "My Awesome Community",
                                value: "{name}",
                                disabled: *creating.read(),
                                oninput: move |evt| name.set(evt.value()),
                            }
                            p { class: "mt-1 text-xs text-muted-foreground",
                                "Display name for your community."
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Description" }
                            textarea {
                                class: "w-full p-3 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-blue-500 min-h-[100px]",
                                placeholder: "What is your community about?",
                                value: "{description}",
                                disabled: *creating.read(),
                                oninput: move |evt| description.set(evt.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Image URL" }
                            input {
                                class: "w-full p-3 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                                r#type: "url",
                                placeholder: "https://example.com/community-image.jpg",
                                value: "{image_url}",
                                disabled: *creating.read(),
                                oninput: move |evt| image_url.set(evt.value()),
                            }
                            p { class: "mt-1 text-xs text-muted-foreground",
                                "URL for your community avatar image."
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Rules" }
                            textarea {
                                class: "w-full p-3 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-blue-500 min-h-[100px]",
                                placeholder: "1. Be respectful\n2. No spam\n3. Stay on topic",
                                value: "{rules}",
                                disabled: *creating.read(),
                                oninput: move |evt| rules.set(evt.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Moderators" }
                            div { class: "flex gap-2 mb-2",
                                input {
                                    class: "flex-1 p-3 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-blue-500 font-mono text-sm",
                                    r#type: "text",
                                    placeholder: "64-character hex pubkey",
                                    value: "{moderator_input}",
                                    disabled: *creating.read(),
                                    oninput: move |evt| moderator_input.set(evt.value()),
                                }
                                button {
                                    r#type: "button",
                                    class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50",
                                    disabled: *creating.read() || moderator_input.read().trim().is_empty(),
                                    onclick: add_moderator,
                                    "Add"
                                }
                            }
                            if !moderators.read().is_empty() {
                                div { class: "space-y-2",
                                    for mod_pubkey in moderators.read().iter() {
                                        {
                                            let truncated_pubkey = if mod_pubkey.len() > 24 {
                                                format!("{}...{}", &mod_pubkey[..16], &mod_pubkey[mod_pubkey.len() - 8..])
                                            } else {
                                                mod_pubkey.clone()
                                            };
                                            let pk_for_remove = mod_pubkey.clone();
                                            rsx! {
                                                div {
                                                    key: "{mod_pubkey}",
                                                    class: "flex items-center justify-between p-2 bg-accent/50 rounded-lg",
                                                    div { class: "flex items-center gap-2",
                                                        div { class: "w-8 h-8 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white text-sm",
                                                            svg {
                                                                class: "w-4 h-4",
                                                                xmlns: "http://www.w3.org/2000/svg",
                                                                width: "24",
                                                                height: "24",
                                                                view_box: "0 0 24 24",
                                                                fill: "none",
                                                                stroke: "currentColor",
                                                                stroke_width: "2",
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                path { d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10" }
                                                            }
                                                        }
                                                        span { class: "text-sm font-mono truncate max-w-[200px]", "{truncated_pubkey}" }
                                                    }
                                                    button {
                                                        r#type: "button",
                                                        class: "p-1 hover:bg-red-100 dark:hover:bg-red-900 text-red-500 rounded transition",
                                                        onclick: move |_| remove_moderator(pk_for_remove.clone()),
                                                        svg {
                                                            class: "w-4 h-4",
                                                            xmlns: "http://www.w3.org/2000/svg",
                                                            width: "24",
                                                            height: "24",
                                                            view_box: "0 0 24 24",
                                                            fill: "none",
                                                            stroke: "currentColor",
                                                            stroke_width: "2",
                                                            stroke_linecap: "round",
                                                            stroke_linejoin: "round",
                                                            line {
                                                                x1: "18",
                                                                y1: "6",
                                                                x2: "6",
                                                                y2: "18",
                                                            }
                                                            line {
                                                                x1: "6",
                                                                y1: "6",
                                                                x2: "18",
                                                                y2: "18",
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            p { class: "mt-1 text-xs text-muted-foreground",
                                "Add pubkeys of users who can approve posts. You are automatically the owner and can moderate."
                            }
                        }
                        div { class: "pt-4",
                            button {
                                r#type: "submit",
                                class: "w-full px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *creating.read() || identifier.read().trim().is_empty()
                                    || name.read().trim().is_empty(),
                                if *creating.read() {
                                    span { class: "flex items-center justify-center gap-2",
                                        span { class: "inline-block w-5 h-5 border-2 border-white border-t-transparent rounded-full animate-spin" }
                                        "Creating Community..."
                                    }
                                } else {
                                    span { class: "flex items-center justify-center gap-2",
                                        svg {
                                            class: "w-5 h-5",
                                            xmlns: "http://www.w3.org/2000/svg",
                                            width: "24",
                                            height: "24",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            path { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }
                                            circle { cx: "9", cy: "7", r: "4" }
                                            line {
                                                x1: "19",
                                                y1: "8",
                                                x2: "19",
                                                y2: "14",
                                            }
                                            line {
                                                x1: "22",
                                                y1: "11",
                                                x2: "16",
                                                y2: "11",
                                            }
                                        }
                                        "Create Community"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
