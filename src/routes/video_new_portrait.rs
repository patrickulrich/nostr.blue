use dioxus::prelude::*;
use crate::stores::auth_store;
use crate::components::MediaUploader;

#[component]
pub fn VideoNewPortrait() -> Element {
    let navigator = navigator();
    let mut title = use_signal(|| String::new());
    let mut description = use_signal(|| String::new());
    let mut video_url = use_signal(|| Option::<String>::None);
    let mut thumbnail_url = use_signal(|| String::new());
    let mut hashtags = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);
    let mut show_video_uploader = use_signal(|| true);
    let mut show_thumbnail_uploader = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);

    // Check if user is authenticated
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

    // Validation
    let can_publish = title.read().chars().count() > 0
        && video_url.read().is_some()
        && !*is_publishing.read();

    // Handle close
    let handle_close = move |_| {
        navigator.go_back();
    };

    // Handle video upload
    let handle_video_uploaded = move |url: String| {
        video_url.set(Some(url.clone()));
        show_video_uploader.set(false);
        log::info!("Video uploaded: {}", url);
    };

    // Handle thumbnail upload
    let handle_thumbnail_uploaded = move |url: String| {
        thumbnail_url.set(url.clone());
        show_thumbnail_uploader.set(false);
        log::info!("Thumbnail uploaded: {}", url);
    };

    // Handle publishing
    let handle_publish = move |_| {
        if !can_publish {
            return;
        }

        let title_val = title.read().clone();
        let description_val = description.read().clone();
        let video_url_val = video_url.read().clone().unwrap_or_default();
        let thumbnail_url_val = thumbnail_url.read().clone();
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

            match crate::stores::nostr_client::publish_video(
                title_val,
                description_val,
                video_url_val,
                thumbnail_url_val,
                tags_vec,
                true, // portrait/vertical video
            ).await {
                Ok(event_id) => {
                    log::info!("Short video published successfully: {}", event_id);
                    is_publishing.set(false);
                    navigator.push(crate::routes::Route::Videos {});
                }
                Err(e) => {
                    log::error!("Failed to publish short video: {}", e);
                    error_message.set(Some(format!("Failed to publish: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };

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
                            "Create Short"
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

                    // Video uploader
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Video * (Portrait/Vertical - Shorts)"
                        }

                        if let Some(url) = video_url.read().as_ref() {
                            div {
                                class: "mb-4 flex justify-center",
                                video {
                                    src: "{url}",
                                    class: "max-w-sm aspect-[9/16] bg-black rounded-lg",
                                    controls: true,
                                }
                            }
                            button {
                                class: "mt-2 text-sm text-red-500 hover:text-red-600",
                                onclick: move |_| {
                                    video_url.set(None);
                                    show_video_uploader.set(true);
                                },
                                "Remove video"
                            }
                        } else if *show_video_uploader.read() {
                            MediaUploader {
                                on_upload: handle_video_uploaded,
                            }
                        }

                        p {
                            class: "mt-2 text-xs text-muted-foreground",
                            "Upload a vertical/portrait video (9:16 aspect ratio recommended)"
                        }
                    }

                    // Thumbnail uploader
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Thumbnail (optional)"
                        }

                        if !thumbnail_url.read().is_empty() {
                            div {
                                class: "mb-4 flex justify-center",
                                img {
                                    src: "{thumbnail_url}",
                                    class: "max-w-sm aspect-[9/16] object-cover rounded-lg border border-border",
                                }
                            }
                            button {
                                class: "mt-2 text-sm text-red-500 hover:text-red-600",
                                onclick: move |_| {
                                    thumbnail_url.set(String::new());
                                    show_thumbnail_uploader.set(true);
                                },
                                "Remove thumbnail"
                            }
                        } else if *show_thumbnail_uploader.read() {
                            MediaUploader {
                                on_upload: handle_thumbnail_uploaded,
                            }
                        } else {
                            button {
                                class: "px-4 py-2 bg-accent hover:bg-accent/80 text-foreground rounded-lg transition",
                                onclick: move |_| show_thumbnail_uploader.set(true),
                                "+ Add thumbnail"
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
                            placeholder: "Give your short a title",
                            value: "{title}",
                            oninput: move |e| title.set(e.value()),
                        }
                    }

                    // Description
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Description (optional)"
                        }
                        textarea {
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none",
                            rows: 4,
                            placeholder: "Describe your short...",
                            value: "{description}",
                            oninput: move |e| description.set(e.value()),
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
                            placeholder: "shorts, viral, funny (comma separated)",
                            value: "{hashtags}",
                            oninput: move |e| hashtags.set(e.value()),
                        }
                    }
                }
            }
        }
    }
}
