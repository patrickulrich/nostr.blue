use dioxus::prelude::*;
use crate::stores::auth_store;
use crate::components::MediaUploader;

#[component]
pub fn PhotoNew() -> Element {
    let navigator = navigator();
    let mut title = use_signal(|| String::new());
    let mut caption = use_signal(|| String::new());
    let mut image_urls = use_signal(|| Vec::<String>::new());
    let mut hashtags = use_signal(|| String::new());
    let mut location = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);
    let mut show_image_uploader = use_signal(|| true);
    let mut error_message = use_signal(|| Option::<String>::None);

    // Check if user is authenticated
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

    // Validation
    let can_publish = title.read().chars().count() > 0
        && image_urls.read().len() > 0
        && !*is_publishing.read();

    // Handle close
    let handle_close = move |_| {
        navigator.go_back();
    };

    // Handle image upload
    let handle_image_uploaded = move |url: String| {
        let mut urls = image_urls.write();
        urls.push(url.clone());
        log::info!("Image added: {}", url);
        // Keep uploader open for more images
    };

    // Handle remove image
    let mut handle_remove_image = move |index: usize| {
        let mut urls = image_urls.write();
        if index < urls.len() {
            urls.remove(index);
        }
    };

    // Handle publishing
    let handle_publish = move |_| {
        if !can_publish {
            return;
        }

        let title_val = title.read().clone();
        let caption_val = caption.read().clone();
        let image_urls_val = image_urls.read().clone();
        let hashtags_val = hashtags.read().clone();
        let location_val = location.read().clone();

        is_publishing.set(true);
        error_message.set(None);

        spawn(async move {
            // Parse hashtags
            let tags_vec: Vec<String> = hashtags_val
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            match crate::stores::nostr_client::publish_picture(
                title_val,
                caption_val,
                image_urls_val,
                tags_vec,
                location_val,
            ).await {
                Ok(event_id) => {
                    log::info!("Picture post published successfully: {}", event_id);
                    is_publishing.set(false);
                    navigator.push(crate::routes::Route::Photos {});
                }
                Err(e) => {
                    log::error!("Failed to publish picture: {}", e);
                    error_message.set(Some(format!("Failed to publish: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };

    // Redirect if not authenticated
    if !is_authenticated() {
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
                    class: "max-w-4xl mx-auto px-4 py-4 flex items-center justify-between",

                    div {
                        class: "flex items-center gap-4",
                        button {
                            class: "text-muted-foreground hover:text-foreground transition",
                            onclick: handle_close,
                            crate::components::icons::ArrowLeftIcon { class: "w-6 h-6".to_string() }
                        }
                        h1 {
                            class: "text-2xl font-bold",
                            "Share Photo"
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
                            "Post"
                        }
                    }
                }
            }

            // Main content
            div {
                class: "max-w-4xl mx-auto px-4 py-8",

                // Error message
                if let Some(err) = error_message.read().as_ref() {
                    div {
                        class: "mb-4 p-4 bg-red-100 dark:bg-red-900/20 border border-red-300 dark:border-red-800 rounded-lg text-red-800 dark:text-red-200",
                        "{err}"
                    }
                }

                div {
                    class: "space-y-6",

                    // Image uploader
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Images * (upload one or more)"
                        }

                        // Display uploaded images
                        if image_urls.read().len() > 0 {
                            div {
                                class: "grid grid-cols-2 md:grid-cols-3 gap-4 mb-4",
                                for (index , url) in image_urls.read().iter().enumerate() {
                                    div {
                                        key: "{url}",
                                        class: "relative aspect-square group",
                                        img {
                                            src: "{url}",
                                            class: "w-full h-full object-cover rounded-lg border border-border",
                                        }
                                        button {
                                            class: "absolute top-2 right-2 bg-red-500 hover:bg-red-600 text-white rounded-full p-2 opacity-0 group-hover:opacity-100 transition",
                                            onclick: move |_| handle_remove_image(index),
                                            svg {
                                                xmlns: "http://www.w3.org/2000/svg",
                                                class: "w-4 h-4",
                                                fill: "none",
                                                view_box: "0 0 24 24",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M6 18L18 6M6 6l12 12"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Image uploader
                        if *show_image_uploader.read() {
                            MediaUploader {
                                on_upload: handle_image_uploaded,
                            }
                        }

                        if !*show_image_uploader.read() {
                            button {
                                class: "px-4 py-2 bg-accent hover:bg-accent/80 text-foreground rounded-lg transition",
                                onclick: move |_| show_image_uploader.set(true),
                                "+ Add more images"
                            }
                        }
                    }

                    // Title
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Title *"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "Give your photo a title",
                            value: "{title}",
                            oninput: move |e| title.set(e.value()),
                        }
                    }

                    // Caption/Description
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Caption (optional)"
                        }
                        textarea {
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none",
                            rows: 4,
                            placeholder: "Describe your photo...",
                            value: "{caption}",
                            oninput: move |e| caption.set(e.value()),
                        }
                    }

                    // Location
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Location (optional)"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "Where was this taken?",
                            value: "{location}",
                            oninput: move |e| location.set(e.value()),
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
                            placeholder: "photography, nature, sunset (comma separated)",
                            value: "{hashtags}",
                            oninput: move |e| hashtags.set(e.value()),
                        }
                    }
                }
            }
        }
    }
}
