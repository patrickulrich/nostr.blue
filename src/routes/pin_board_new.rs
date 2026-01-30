//! Create/Edit Pinboard Page
//! Simplified form for creating and editing pinboards (Kind 30067)
use crate::components::{ClientInitializing, MediaUploader};
use crate::routes::Route;
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::pin_boards_store::{self, Pinboard, PinboardInput};
use dioxus::prelude::*;
#[component]
pub fn PinBoardNew() -> Element {
    let navigator = use_navigator();
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut image_url = use_signal(|| None::<String>);
    let mut tags_input = use_signal(String::new);
    let mut collaborative = use_signal(|| false);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let handle_submit = move |evt: Event<FormData>| {
        evt.prevent_default();
        if title.read().trim().is_empty() {
            error.set(Some("Title is required".to_string()));
            return;
        }
        error.set(None);
        is_submitting.set(true);
        let tags: Vec<String> = tags_input
            .read()
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        let input = PinboardInput {
            title: title.read().clone(),
            description: if description.read().is_empty() {
                None
            } else {
                Some(description.read().clone())
            },
            image: image_url.read().clone(),
            tags,
            collaborative: *collaborative.read(),
        };
        spawn(async move {
            match pin_boards_store::publish_pinboard(input, None).await {
                Ok(naddr) => {
                    navigator.push(Route::PinBoardDetail { naddr });
                }
                Err(e) => {
                    error.set(Some(e));
                    is_submitting.set(false);
                }
            }
        });
    };
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: Route::PinBoardsHome {},
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition",
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7",
                            }
                        }
                    }
                    h1 { class: "text-xl font-bold flex items-center gap-2",
                        span { class: "text-2xl", "📌" }
                        "Create Board"
                    }
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if !*HAS_SIGNER.read() {
                div { class: "p-4",
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        span { class: "text-5xl mb-4", "🔐" }
                        h2 { class: "text-xl font-semibold mb-2", "Sign In Required" }
                        p { class: "text-muted-foreground mb-4",
                            "You need to sign in to create a pinboard"
                        }
                    }
                }
            } else {
                div { class: "p-4 max-w-2xl mx-auto",
                    form { onsubmit: handle_submit, class: "space-y-6",
                        if let Some(ref err) = *error.read() {
                            div { class: "p-4 bg-red-100 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-lg",
                                "{err}"
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "Title "
                                span { class: "text-red-500", "*" }
                            }
                            input {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "My Awesome Collection",
                                value: "{title}",
                                oninput: move |evt| title.set(evt.value()),
                                required: true,
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Description" }
                            textarea {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                                rows: "3",
                                placeholder: "What is this board about?",
                                value: "{description}",
                                oninput: move |evt| description.set(evt.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Cover Image" }
                            if let Some(ref url) = *image_url.read() {
                                div { class: "relative w-full h-40 rounded-lg overflow-hidden mb-2",
                                    img {
                                        src: "{url}",
                                        alt: "Cover image",
                                        class: "w-full h-full object-cover",
                                    }
                                    button {
                                        r#type: "button",
                                        class: "absolute top-2 right-2 p-1.5 rounded-full bg-red-500 text-white hover:bg-red-600",
                                        onclick: move |_| image_url.set(None),
                                        "X"
                                    }
                                }
                            }
                            MediaUploader {
                                on_upload: move |url: String| image_url.set(Some(url)),
                                button_label: "Upload cover image".to_string(),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Tags" }
                            input {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "cooking, favorites, inspiration (comma separated)",
                                value: "{tags_input}",
                                oninput: move |evt| tags_input.set(evt.value()),
                            }
                            p { class: "text-xs text-muted-foreground mt-1",
                                "Add hashtags to help others discover your board"
                            }
                        }
                        div { class: "flex items-center justify-between p-4 bg-muted/50 rounded-lg",
                            div {
                                label { class: "block text-sm font-medium", "Collaborative Board" }
                                p { class: "text-xs text-muted-foreground mt-0.5",
                                    "Allow anyone to pin content to this board"
                                }
                            }
                            button {
                                r#type: "button",
                                class: if *collaborative.read() { "relative inline-flex h-6 w-11 items-center rounded-full bg-primary transition" } else { "relative inline-flex h-6 w-11 items-center rounded-full bg-gray-300 dark:bg-gray-600 transition" },
                                onclick: move |_| {
                                    let current = *collaborative.read();
                                    collaborative.set(!current);
                                },
                                span { class: if *collaborative.read() { "inline-block h-4 w-4 transform rounded-full bg-white transition translate-x-6" } else { "inline-block h-4 w-4 transform rounded-full bg-white transition translate-x-1" } }
                            }
                        }
                        div { class: "pt-4",
                            button {
                                r#type: "submit",
                                class: "w-full px-6 py-3 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *is_submitting.read(),
                                if *is_submitting.read() {
                                    span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-2" }
                                    "Creating..."
                                } else {
                                    "Create Board"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
#[component]
pub fn PinBoardEdit(naddr: String) -> Element {
    let navigator = use_navigator();
    let naddr_for_effect = naddr.clone();
    let mut board = use_signal(|| None::<Pinboard>);
    let mut loading = use_signal(|| true);
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut image_url = use_signal(|| None::<String>);
    let mut tags_input = use_signal(String::new);
    let mut collaborative = use_signal(|| false);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let naddr_clone = naddr_for_effect.clone();
        loading.set(true);
        spawn(async move {
            match pin_boards_store::fetch_pinboard_by_naddr(&naddr_clone).await {
                Ok(Some(b)) => {
                    title.set(b.title.clone());
                    description.set(b.description.clone().unwrap_or_default());
                    image_url.set(b.image.clone());
                    tags_input.set(b.tags.join(", "));
                    collaborative.set(b.collaborative);
                    board.set(Some(b));
                    loading.set(false);
                }
                Ok(None) => {
                    error.set(Some("Board not found".to_string()));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });
    let handle_submit = move |evt: Event<FormData>| {
        evt.prevent_default();
        if title.read().trim().is_empty() {
            error.set(Some("Title is required".to_string()));
            return;
        }
        let board_ref = board.read();
        let Some(ref existing_board) = *board_ref else {
            error.set(Some("Board not loaded".to_string()));
            return;
        };
        let d_tag = existing_board.d_tag.clone();
        drop(board_ref);
        error.set(None);
        is_submitting.set(true);
        let tags: Vec<String> = tags_input
            .read()
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        let input = PinboardInput {
            title: title.read().clone(),
            description: if description.read().is_empty() {
                None
            } else {
                Some(description.read().clone())
            },
            image: image_url.read().clone(),
            tags,
            collaborative: *collaborative.read(),
        };
        spawn(async move {
            match pin_boards_store::publish_pinboard(input, Some(&d_tag)).await {
                Ok(new_naddr) => {
                    navigator
                        .push(Route::PinBoardDetail {
                            naddr: new_naddr,
                        });
                }
                Err(e) => {
                    error.set(Some(e));
                    is_submitting.set(false);
                }
            }
        });
    };
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: Route::PinBoardDetail {
                            naddr: naddr.clone(),
                        },
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition",
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7",
                            }
                        }
                    }
                    h1 { class: "text-xl font-bold flex items-center gap-2",
                        span { class: "text-2xl", "📌" }
                        "Edit Board"
                    }
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if !*HAS_SIGNER.read() {
                div { class: "p-4",
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        span { class: "text-5xl mb-4", "🔐" }
                        h2 { class: "text-xl font-semibold mb-2", "Sign In Required" }
                        p { class: "text-muted-foreground mb-4",
                            "You need to sign in to edit this board"
                        }
                    }
                }
            } else if *loading.read() {
                div { class: "p-4 max-w-2xl mx-auto animate-pulse space-y-6",
                    div { class: "h-10 w-full bg-muted rounded-lg" }
                    div { class: "h-24 w-full bg-muted rounded-lg" }
                    div { class: "h-40 w-full bg-muted rounded-lg" }
                    div { class: "h-10 w-full bg-muted rounded-lg" }
                }
            } else if board.read().is_none() {
                div { class: "p-4 text-center py-12",
                    p { class: "text-red-500 mb-4", "Board not found" }
                    Link {
                        to: Route::PinBoardsHome {},
                        class: "text-primary hover:underline",
                        "Back to Pinboards"
                    }
                }
            } else {
                div { class: "p-4 max-w-2xl mx-auto",
                    form { onsubmit: handle_submit, class: "space-y-6",
                        if let Some(ref err) = *error.read() {
                            div { class: "p-4 bg-red-100 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-lg",
                                "{err}"
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2",
                                "Title "
                                span { class: "text-red-500", "*" }
                            }
                            input {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "My Awesome Collection",
                                value: "{title}",
                                oninput: move |evt| title.set(evt.value()),
                                required: true,
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Description" }
                            textarea {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                                rows: "3",
                                placeholder: "What is this board about?",
                                value: "{description}",
                                oninput: move |evt| description.set(evt.value()),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Cover Image" }
                            if let Some(ref url) = *image_url.read() {
                                div { class: "relative w-full h-40 rounded-lg overflow-hidden mb-2",
                                    img {
                                        src: "{url}",
                                        alt: "Cover image",
                                        class: "w-full h-full object-cover",
                                    }
                                    button {
                                        r#type: "button",
                                        class: "absolute top-2 right-2 p-1.5 rounded-full bg-red-500 text-white hover:bg-red-600",
                                        onclick: move |_| image_url.set(None),
                                        "X"
                                    }
                                }
                            }
                            MediaUploader {
                                on_upload: move |url: String| image_url.set(Some(url)),
                                button_label: "Upload cover image".to_string(),
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "Tags" }
                            input {
                                class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "cooking, favorites, inspiration (comma separated)",
                                value: "{tags_input}",
                                oninput: move |evt| tags_input.set(evt.value()),
                            }
                        }
                        div { class: "flex items-center justify-between p-4 bg-muted/50 rounded-lg",
                            div {
                                label { class: "block text-sm font-medium", "Collaborative Board" }
                                p { class: "text-xs text-muted-foreground mt-0.5",
                                    "Allow anyone to pin content to this board"
                                }
                            }
                            button {
                                r#type: "button",
                                class: if *collaborative.read() { "relative inline-flex h-6 w-11 items-center rounded-full bg-primary transition" } else { "relative inline-flex h-6 w-11 items-center rounded-full bg-gray-300 dark:bg-gray-600 transition" },
                                onclick: move |_| {
                                    let current = *collaborative.read();
                                    collaborative.set(!current);
                                },
                                span { class: if *collaborative.read() { "inline-block h-4 w-4 transform rounded-full bg-white transition translate-x-6" } else { "inline-block h-4 w-4 transform rounded-full bg-white transition translate-x-1" } }
                            }
                        }
                        div { class: "pt-4",
                            button {
                                r#type: "submit",
                                class: "w-full px-6 py-3 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *is_submitting.read(),
                                if *is_submitting.read() {
                                    span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-2" }
                                    "Saving..."
                                } else {
                                    "Save Changes"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
