//! Create Cookbook Modal
//! Modal for creating a new cookbook (pinboard tagged with "cookbook")

use dioxus::prelude::*;
use crate::stores::pin_boards_store::{self, PinboardInput};
use crate::components::MediaUploader;
use crate::routes::Route;

/// Modal for creating a new cookbook
#[component]
pub fn CreateCookbookModal(
    on_close: EventHandler<()>,
) -> Element {
    let navigator = use_navigator();

    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut image_url = use_signal(|| None::<String>);
    let mut additional_tags = use_signal(String::new);

    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    // Handle form submission
    let handle_submit = move |evt: Event<FormData>| {
        evt.prevent_default();

        // Validate title
        if title.read().trim().is_empty() {
            error.set(Some("Title is required".to_string()));
            return;
        }

        error.set(None);
        is_submitting.set(true);

        // Parse additional tags and always include "cookbook"
        let mut tags: Vec<String> = vec!["cookbook".to_string()];
        let additional: Vec<String> = additional_tags.read()
            .split(',')
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty() && t != "cookbook")
            .collect();
        tags.extend(additional);

        let input = PinboardInput {
            title: title.read().clone(),
            description: if description.read().is_empty() { None } else { Some(description.read().clone()) },
            image: image_url.read().clone(),
            tags,
            collaborative: false, // Cookbooks are not collaborative by default
        };

        spawn(async move {
            match pin_boards_store::publish_pinboard(input, None).await {
                Ok(naddr) => {
                    // Navigate to the new cookbook
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
        // Backdrop
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal
            div {
                class: "bg-card rounded-xl shadow-xl max-w-lg w-full max-h-[90vh] overflow-y-auto",
                onclick: move |evt| evt.stop_propagation(),

                // Header
                div {
                    class: "sticky top-0 bg-card border-b border-border px-4 py-3 flex items-center justify-between",

                    h2 {
                        class: "text-lg font-bold flex items-center gap-2",
                        span { class: "text-xl", "📚" }
                        "Create Cookbook"
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

                // Form
                form {
                    onsubmit: handle_submit,
                    class: "p-4 space-y-4",

                    // Error message
                    if let Some(ref err) = *error.read() {
                        div {
                            class: "p-3 bg-red-100 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-lg text-sm",
                            "{err}"
                        }
                    }

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
                            value: "{title}",
                            oninput: move |evt| title.set(evt.value()),
                            required: true,
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
                            value: "{description}",
                            oninput: move |evt| description.set(evt.value()),
                        }
                    }

                    // Cover Image
                    div {
                        label {
                            class: "block text-sm font-medium mb-1.5",
                            "Cover Image"
                        }
                        if let Some(ref url) = *image_url.read() {
                            div {
                                class: "relative w-full h-32 rounded-lg overflow-hidden mb-2",
                                img {
                                    src: "{url}",
                                    alt: "Cover image",
                                    class: "w-full h-full object-cover"
                                }
                                button {
                                    r#type: "button",
                                    class: "absolute top-2 right-2 p-1 rounded-full bg-red-500 text-white hover:bg-red-600 text-xs",
                                    onclick: move |_| image_url.set(None),
                                    "✕"
                                }
                            }
                        }
                        MediaUploader {
                            on_upload: move |url: String| image_url.set(Some(url)),
                            button_label: "Upload cover image".to_string(),
                        }
                    }

                    // Additional Tags
                    div {
                        label {
                            class: "block text-sm font-medium mb-1.5",
                            "Additional Tags"
                        }
                        div {
                            class: "flex items-center gap-2 mb-1.5",
                            span {
                                class: "px-2 py-0.5 bg-orange-100 dark:bg-orange-900/30 text-orange-700 dark:text-orange-300 rounded text-xs font-medium",
                                "#cookbook"
                            }
                            span {
                                class: "text-xs text-muted-foreground",
                                "(always included)"
                            }
                        }
                        input {
                            class: "w-full px-3 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary text-sm",
                            r#type: "text",
                            placeholder: "italian, desserts, quick-meals (comma separated)",
                            value: "{additional_tags}",
                            oninput: move |evt| additional_tags.set(evt.value()),
                        }
                    }

                    // Submit button
                    div {
                        class: "pt-2",
                        button {
                            r#type: "submit",
                            class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                            disabled: *is_submitting.read(),
                            if *is_submitting.read() {
                                span {
                                    class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-2"
                                }
                                "Creating..."
                            } else {
                                "Create Cookbook"
                            }
                        }
                    }
                }
            }
        }
    }
}
