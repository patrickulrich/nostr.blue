use dioxus::prelude::*;
use crate::stores::auth_store;
use crate::components::MarkdownEditor;

#[component]
pub fn ArticleNew() -> Element {
    let navigator = navigator();
    let mut title = use_signal(|| String::new());
    let mut summary = use_signal(|| String::new());
    let content = use_signal(|| String::new());
    let mut identifier = use_signal(|| String::new());
    let mut cover_image = use_signal(|| String::new());
    let mut hashtags = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);

    // Check if user is authenticated
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

    // Validation
    let title_chars = title.read().chars().count();
    let content_chars = content.read().chars().count();
    let can_publish = title_chars > 0
        && content_chars > 0
        && identifier.read().len() > 0
        && !*is_publishing.read();

    // Handle close
    let handle_close = move |_| {
        navigator.go_back();
    };

    // Handle publishing
    let handle_publish = move |_| {
        if !can_publish {
            return;
        }

        let title_val = title.read().clone();
        let summary_val = summary.read().clone();
        let content_val = content.read().clone();
        let identifier_val = identifier.read().clone();
        let cover_image_val = cover_image.read().clone();
        let hashtags_val = hashtags.read().clone();

        is_publishing.set(true);
        error_message.set(None);

        spawn(async move {
            // Parse hashtags
            let tags_vec: Vec<String> = hashtags_val
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            match crate::stores::nostr_client::publish_article(
                title_val,
                summary_val,
                content_val,
                identifier_val,
                cover_image_val,
                tags_vec,
            ).await {
                Ok(event_id) => {
                    log::info!("Article published successfully: {}", event_id);
                    is_publishing.set(false);
                    navigator.push(crate::routes::Route::Articles {});
                }
                Err(e) => {
                    log::error!("Failed to publish article: {}", e);
                    error_message.set(Some(format!("Failed to publish: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };

    // Auto-generate identifier from title if empty
    use_effect(move || {
        if identifier.read().is_empty() && !title.read().is_empty() {
            let slug = title.read()
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .split('-')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("-");

            if !slug.is_empty() {
                identifier.set(slug);
            }
        }
    });

    // Redirect if not authenticated
    if !*is_authenticated.read() {
        use_effect(move || {
            navigator.push(crate::routes::Route::Home {});
        });
        return rsx! {
            div { class: "flex items-center justify-center h-screen",
                "Redirecting..."
            }
        };
    }

    rsx! {
        div {
            class: "min-h-screen bg-background",

            // Header
            div {
                class: "border-b border-border bg-background sticky top-0 z-10",
                div {
                    class: "max-w-6xl mx-auto px-4 py-4 flex items-center justify-between",

                    div {
                        class: "flex items-center gap-4",
                        button {
                            class: "text-muted-foreground hover:text-foreground transition",
                            onclick: handle_close,
                            crate::components::icons::ArrowLeftIcon { class: "w-6 h-6".to_string() }
                        }
                        h1 {
                            class: "text-2xl font-bold",
                            "Write Article"
                        }
                    }

                    button {
                        class: if can_publish {
                            "px-6 py-2 bg-blue-500 hover:bg-blue-600 text-white font-bold rounded-full transition"
                        } else {
                            "px-6 py-2 bg-gray-300 text-gray-500 font-bold rounded-full cursor-not-allowed"
                        },
                        disabled: !can_publish,
                        onclick: handle_publish,

                        if *is_publishing.read() {
                            "Publishing..."
                        } else {
                            "Publish"
                        }
                    }
                }
            }

            // Main content
            div {
                class: "max-w-6xl mx-auto px-4 py-8",

                // Error message
                if let Some(err) = error_message.read().as_ref() {
                    div {
                        class: "mb-4 p-4 bg-red-100 dark:bg-red-900/20 border border-red-300 dark:border-red-800 rounded-lg text-red-800 dark:text-red-200",
                        "{err}"
                    }
                }

                // Form fields
                div {
                    class: "space-y-6",

                    // Title
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Title *"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-2xl font-bold",
                            placeholder: "Enter article title",
                            value: "{title}",
                            oninput: move |e| title.set(e.value()),
                        }
                    }

                    // Identifier (d-tag)
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Identifier (URL slug) *"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "unique-article-identifier",
                            value: "{identifier}",
                            oninput: move |e| identifier.set(e.value()),
                        }
                        p {
                            class: "mt-1 text-xs text-muted-foreground",
                            "Unique identifier for this article. Auto-generated from title."
                        }
                    }

                    // Summary
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Summary (optional)"
                        }
                        textarea {
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none",
                            rows: 3,
                            placeholder: "Brief description of your article",
                            value: "{summary}",
                            oninput: move |e| summary.set(e.value()),
                        }
                    }

                    // Cover image URL
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Cover Image URL (optional)"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "https://example.com/image.jpg",
                            value: "{cover_image}",
                            oninput: move |e| cover_image.set(e.value()),
                        }
                    }

                    // Hashtags
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Hashtags (optional)"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "nostr, bitcoin, technology (comma separated)",
                            value: "{hashtags}",
                            oninput: move |e| hashtags.set(e.value()),
                        }
                    }

                    // Content editor
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Content *"
                        }
                        div {
                            class: "border border-border rounded-lg overflow-hidden bg-background",
                            style: "height: 600px;",
                            MarkdownEditor {
                                content: content,
                                min_height: 600,
                                placeholder: "Write your article content here... Markdown is supported.".to_string(),
                            }
                        }
                    }

                    // Character counts
                    div {
                        class: "flex justify-between text-sm text-muted-foreground",
                        span {
                            "Title: {title_chars} characters"
                        }
                        span {
                            "Content: {content_chars} characters"
                        }
                    }
                }
            }
        }
    }
}
