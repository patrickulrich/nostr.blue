//! Add to Cookbook Modal
//! Modal for adding a recipe to an existing cookbook or creating a new one

use dioxus::prelude::*;
use nostr_sdk::{nips::nip01::Coordinate, FromBech32};
use crate::stores::pin_boards_store::{self, Pinboard, PinboardInput, PinInput, PinReference};
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::components::MediaUploader;
use crate::routes::Route;

/// Modal for adding a recipe to a cookbook
#[component]
pub fn AddToCookbookModal(
    /// The recipe's naddr to pin
    recipe_naddr: String,
    /// Optional recipe title for display
    #[props(default)]
    recipe_title: Option<String>,
    on_close: EventHandler<()>,
) -> Element {
    let navigator = use_navigator();

    // Mode: existing cookbook or create new
    let mut create_new = use_signal(|| false);

    // Existing cookbooks state
    let mut cookbooks = use_signal(Vec::<Pinboard>::new);
    let mut cookbooks_loading = use_signal(|| true);
    let mut selected_cookbook = use_signal(|| None::<Pinboard>);

    // New cookbook form state
    let mut new_title = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut new_image_url = use_signal(|| None::<String>);

    // Operation state
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);

    // Fetch user's cookbooks on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized || !*HAS_SIGNER.read() {
            cookbooks_loading.set(false);
            return;
        }

        spawn(async move {
            // Fetch user's own cookbooks
            match pin_boards_store::fetch_user_cookbooks().await {
                Ok(books) => {
                    cookbooks.set(books);
                }
                Err(e) => {
                    log::error!("Failed to fetch user cookbooks: {}", e);
                }
            }
            cookbooks_loading.set(false);
        });
    });

    let recipe_naddr_for_add = recipe_naddr.clone();
    let recipe_naddr_for_create = recipe_naddr.clone();

    // Handle adding to existing cookbook
    let handle_add_to_existing = move |_| {
        let cookbook = match selected_cookbook.read().clone() {
            Some(c) => c,
            None => {
                error.set(Some("Please select a cookbook".to_string()));
                return;
            }
        };

        let naddr = recipe_naddr_for_add.clone();
        error.set(None);
        is_submitting.set(true);

        spawn(async move {
            // Create a pin for this recipe
            // Use a_tag (coordinate format) for board reference to ensure proper discovery
            let pin_input = PinInput {
                board_addresses: vec![cookbook.a_tag.clone()],
                reference: PinReference::Coordinate {
                    address: naddr,
                    relay_hint: None,
                },
                title: None, // Will use recipe's own title
                content: String::new(),
                tags: vec![],
            };

            match pin_boards_store::publish_pin(pin_input).await {
                Ok(_) => {
                    success.set(true);
                    is_submitting.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    is_submitting.set(false);
                }
            }
        });
    };

    // Handle creating new cookbook and adding recipe
    let handle_create_and_add = move |_| {
        let title_val = new_title.read().clone();
        if title_val.trim().is_empty() {
            error.set(Some("Cookbook name is required".to_string()));
            return;
        }

        let naddr = recipe_naddr_for_create.clone();
        error.set(None);
        is_submitting.set(true);

        let description = if new_description.read().is_empty() {
            None
        } else {
            Some(new_description.read().clone())
        };
        let image = new_image_url.read().clone();

        spawn(async move {
            // Create the cookbook first
            let cookbook_input = PinboardInput {
                title: title_val,
                description,
                image,
                tags: vec!["cookbook".to_string()],
                collaborative: false,
            };

            match pin_boards_store::publish_pinboard(cookbook_input, None).await {
                Ok(cookbook_naddr) => {
                    // Convert naddr to a_tag (coordinate format) for board reference
                    let board_a_tag = Coordinate::from_bech32(&cookbook_naddr)
                        .map(|coord| format!(
                            "{}:{}:{}",
                            coord.kind.as_u16(),
                            coord.public_key.to_hex(),
                            coord.identifier
                        ))
                        .unwrap_or_else(|_| cookbook_naddr.clone());

                    // Now add the recipe as a pin
                    let pin_input = PinInput {
                        board_addresses: vec![board_a_tag],
                        reference: PinReference::Coordinate {
                            address: naddr,
                            relay_hint: None,
                        },
                        title: None,
                        content: String::new(),
                        tags: vec![],
                    };

                    match pin_boards_store::publish_pin(pin_input).await {
                        Ok(_) => {
                            success.set(true);
                            is_submitting.set(false);
                            // Navigate to the new cookbook after a short delay
                            spawn(async move {
                                gloo_timers::future::TimeoutFuture::new(1500).await;
                                navigator.push(Route::PinBoardDetail { naddr: cookbook_naddr });
                            });
                        }
                        Err(e) => {
                            // Cookbook created but pin failed - still navigate
                            log::error!("Failed to add recipe to new cookbook: {}", e);
                            error.set(Some(format!("Cookbook created but failed to add recipe: {}", e)));
                            is_submitting.set(false);
                        }
                    }
                }
                Err(e) => {
                    error.set(Some(e));
                    is_submitting.set(false);
                }
            }
        });
    };

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal
            div {
                class: "bg-card rounded-xl shadow-xl max-w-md w-full max-h-[85vh] overflow-y-auto",
                onclick: move |evt| evt.stop_propagation(),

                // Header
                div {
                    class: "sticky top-0 bg-card border-b border-border px-4 py-3 flex items-center justify-between",

                    h2 {
                        class: "text-lg font-bold flex items-center gap-2",
                        span { class: "text-xl", "📚" }
                        "Add to Cookbook"
                    }

                    button {
                        r#type: "button",
                        class: "p-1.5 rounded-full hover:bg-muted transition",
                        onclick: move |_| on_close.call(()),
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M6 18L18 6M6 6l12 12"
                            }
                        }
                    }
                }

                // Content
                div {
                    class: "p-4",

                    // Success message
                    if *success.read() {
                        div {
                            class: "flex flex-col items-center justify-center py-8 text-center",
                            span { class: "text-5xl mb-3", "✅" }
                            h3 { class: "text-lg font-semibold mb-1", "Recipe Added!" }
                            p { class: "text-sm text-muted-foreground", "The recipe has been added to your cookbook." }
                        }
                    } else {
                        // Recipe info
                        if let Some(ref title) = recipe_title {
                            div {
                                class: "mb-4 p-3 bg-muted/50 rounded-lg",
                                p {
                                    class: "text-sm text-muted-foreground",
                                    "Adding recipe:"
                                }
                                p {
                                    class: "font-medium truncate",
                                    "{title}"
                                }
                            }
                        }

                        // Error message
                        if let Some(ref err) = *error.read() {
                            div {
                                class: "mb-4 p-3 bg-red-100 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-lg text-sm",
                                "{err}"
                            }
                        }

                        // Toggle between existing and new
                        div {
                            class: "flex gap-2 border-b border-border pb-2 mb-4",
                            button {
                                r#type: "button",
                                class: if !*create_new.read() {
                                    "px-3 py-1 text-sm font-medium border-b-2 border-primary"
                                } else {
                                    "px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground"
                                },
                                onclick: move |_| create_new.set(false),
                                "Existing Cookbook"
                            }
                            button {
                                r#type: "button",
                                class: if *create_new.read() {
                                    "px-3 py-1 text-sm font-medium border-b-2 border-primary"
                                } else {
                                    "px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground"
                                },
                                onclick: move |_| create_new.set(true),
                                "Create New"
                            }
                        }

                        // Existing cookbook selector
                        if !*create_new.read() {
                            div {
                                class: "space-y-4",

                                if *cookbooks_loading.read() {
                                    div {
                                        class: "text-sm text-muted-foreground italic py-4 text-center",
                                        "Loading your cookbooks..."
                                    }
                                } else if cookbooks.read().is_empty() {
                                    div {
                                        class: "text-center py-6",
                                        span { class: "text-4xl mb-2 block", "📚" }
                                        p { class: "text-sm text-muted-foreground mb-3", "You don't have any cookbooks yet." }
                                        button {
                                            r#type: "button",
                                            class: "text-sm text-primary hover:underline",
                                            onclick: move |_| create_new.set(true),
                                            "Create your first cookbook →"
                                        }
                                    }
                                } else {
                                    div {
                                        class: "space-y-2 max-h-60 overflow-y-auto",
                                        for cookbook in cookbooks.read().iter() {
                                            {
                                                let cb = cookbook.clone();
                                                let cb_for_check = cookbook.clone();
                                                let is_selected = selected_cookbook.read().as_ref()
                                                    .map(|s| s.naddr == cb_for_check.naddr)
                                                    .unwrap_or(false);

                                                rsx! {
                                                    button {
                                                        key: "{cb.naddr}",
                                                        r#type: "button",
                                                        class: if is_selected {
                                                            "w-full p-3 border-2 border-primary rounded-lg flex items-center gap-3 text-left bg-primary/5"
                                                        } else {
                                                            "w-full p-3 border border-border rounded-lg flex items-center gap-3 text-left hover:bg-muted/50 transition"
                                                        },
                                                        onclick: move |_| selected_cookbook.set(Some(cb.clone())),

                                                        // Thumbnail
                                                        div {
                                                            class: "w-12 h-12 rounded-lg overflow-hidden bg-gradient-to-br from-orange-500/60 to-amber-600/60 flex-shrink-0",
                                                            if let Some(ref img) = cb_for_check.image {
                                                                img {
                                                                    src: "{img}",
                                                                    alt: "{cb_for_check.title}",
                                                                    class: "w-full h-full object-cover"
                                                                }
                                                            } else {
                                                                div {
                                                                    class: "w-full h-full flex items-center justify-center text-xl",
                                                                    "📚"
                                                                }
                                                            }
                                                        }

                                                        // Info
                                                        div {
                                                            class: "flex-1 min-w-0",
                                                            p {
                                                                class: "font-medium truncate",
                                                                "{cb_for_check.title}"
                                                            }
                                                            if let Some(ref desc) = cb_for_check.description {
                                                                p {
                                                                    class: "text-xs text-muted-foreground truncate",
                                                                    "{desc}"
                                                                }
                                                            }
                                                        }

                                                        // Checkmark
                                                        if is_selected {
                                                            svg {
                                                                class: "w-5 h-5 text-primary flex-shrink-0",
                                                                fill: "none",
                                                                stroke: "currentColor",
                                                                view_box: "0 0 24 24",
                                                                path {
                                                                    stroke_linecap: "round",
                                                                    stroke_linejoin: "round",
                                                                    stroke_width: "2",
                                                                    d: "M5 13l4 4L19 7"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Add button
                                    div {
                                        class: "pt-4",
                                        button {
                                            r#type: "button",
                                            class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                                            disabled: *is_submitting.read() || selected_cookbook.read().is_none(),
                                            onclick: handle_add_to_existing,
                                            if *is_submitting.read() {
                                                span {
                                                    class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-2"
                                                }
                                                "Adding..."
                                            } else {
                                                "Add to Cookbook"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Create new cookbook form
                        if *create_new.read() {
                            div {
                                class: "space-y-4",

                                // Title
                                div {
                                    label {
                                        class: "block text-sm font-medium mb-1.5",
                                        "Cookbook Name "
                                        span { class: "text-red-500", "*" }
                                    }
                                    input {
                                        class: "w-full px-3 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary text-sm",
                                        r#type: "text",
                                        placeholder: "My Recipe Collection",
                                        value: "{new_title}",
                                        oninput: move |evt| new_title.set(evt.value()),
                                    }
                                }

                                // Description
                                div {
                                    label {
                                        class: "block text-sm font-medium mb-1.5",
                                        "Description"
                                    }
                                    textarea {
                                        class: "w-full px-3 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary resize-none text-sm",
                                        rows: "2",
                                        placeholder: "What recipes will this cookbook contain?",
                                        value: "{new_description}",
                                        oninput: move |evt| new_description.set(evt.value()),
                                    }
                                }

                                // Cover Image
                                div {
                                    label {
                                        class: "block text-sm font-medium mb-1.5",
                                        "Cover Image"
                                    }
                                    if let Some(ref url) = *new_image_url.read() {
                                        div {
                                            class: "relative w-full h-24 rounded-lg overflow-hidden mb-2",
                                            img {
                                                src: "{url}",
                                                alt: "Cover image",
                                                class: "w-full h-full object-cover"
                                            }
                                            button {
                                                r#type: "button",
                                                class: "absolute top-2 right-2 p-1 rounded-full bg-red-500 text-white hover:bg-red-600 text-xs",
                                                onclick: move |_| new_image_url.set(None),
                                                "✕"
                                            }
                                        }
                                    }
                                    MediaUploader {
                                        on_upload: move |url: String| new_image_url.set(Some(url)),
                                        button_label: "Upload cover".to_string(),
                                    }
                                }

                                // Create button
                                div {
                                    class: "pt-2",
                                    button {
                                        r#type: "button",
                                        class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                                        disabled: *is_submitting.read(),
                                        onclick: handle_create_and_add,
                                        if *is_submitting.read() {
                                            span {
                                                class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-2"
                                            }
                                            "Creating..."
                                        } else {
                                            "Create & Add Recipe"
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
}
