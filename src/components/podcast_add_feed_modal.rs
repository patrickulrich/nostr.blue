//! Add Podcast Feed Modal
//!
//! Modal for subscribing to new podcast RSS feeds.
//! Validates the feed URL, previews podcast metadata, and saves to NIP-51 list.

use dioxus::prelude::*;
use crate::services::podcast_index;
use crate::stores::{nostr_client, podcast_subscription};

/// Modal for adding a podcast RSS feed subscription
#[component]
pub fn PodcastAddFeedModal(
    /// Handler to close the modal
    on_close: EventHandler<()>,
    /// Handler called when feed is successfully added (passes feed URL)
    on_added: EventHandler<String>,
) -> Element {
    // Form state
    let mut feed_url = use_signal(String::new);

    // Preview state
    let mut preview = use_signal(|| None::<PodcastPreview>);
    let mut is_fetching = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    // Trigger for preview fetch
    let mut trigger_preview = use_signal(|| 0u32);

    // Handle preview fetch when triggered
    use_effect(move || {
        let _trigger = *trigger_preview.read();
        let url = feed_url.read().trim().to_string();

        // Don't run on initial mount (trigger is 0)
        if _trigger == 0 || url.is_empty() {
            return;
        }

        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            error_msg.set(Some("URL must start with http:// or https://".to_string()));
            return;
        }

        // Check for client initialization and signer (NIP-98 auth required)
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();

        if !client_initialized || !has_signer {
            error_msg.set(Some("Please sign in to search for podcasts".to_string()));
            return;
        }

        is_fetching.set(true);
        error_msg.set(None);
        preview.set(None);

        spawn(async move {
            // Use Podcast Index API to look up by feed URL - this gives us the podcast ID
            match podcast_index::get_podcast_by_url(&url).await {
                Ok(feed) => {
                    log::info!("Found podcast: {} (id: {}, guid: {:?})", feed.title, feed.id, feed.podcast_guid);
                    // Require GUID for NIP-73 compliance
                    let Some(guid) = feed.podcast_guid.clone() else {
                        error_msg.set(Some("Podcast does not have a GUID - cannot subscribe".to_string()));
                        is_fetching.set(false);
                        return;
                    };
                    let has_v4v = feed.has_v4v();
                    let image = feed.get_image().map(String::from);
                    preview.set(Some(PodcastPreview {
                        podcast_guid: guid,
                        podcast_id: feed.id,
                        title: feed.title,
                        description: feed.description,
                        image,
                        author: feed.author,
                        has_v4v,
                        feed_url: url,
                    }));
                    is_fetching.set(false);
                }
                Err(e) => {
                    log::error!("Failed to fetch podcast: {}", e);
                    error_msg.set(Some(format!("Podcast not found in index: {}", e)));
                    is_fetching.set(false);
                }
            }
        });
    });

    // Function to trigger preview
    let mut do_preview = move || {
        let url = feed_url.read().trim().to_string();
        if url.is_empty() {
            error_msg.set(Some("Please enter a feed URL".to_string()));
            return;
        }
        trigger_preview.set(trigger_preview() + 1);
    };

    // Handle subscribe
    let handle_subscribe = move |_| {
        let Some(preview_data) = preview.read().clone() else {
            return;
        };
        let podcast_guid = preview_data.podcast_guid.clone();
        let podcast_id = preview_data.podcast_id;
        let url = preview_data.feed_url.clone();

        is_saving.set(true);
        error_msg.set(None);

        spawn(async move {
            match podcast_subscription::add_rss_subscription(&podcast_guid, Some(podcast_id), Some(&url)).await {
                Ok(()) => {
                    log::info!("Subscribed to podcast: {} (guid: {}, id: {})", url, podcast_guid, podcast_id);
                    is_saving.set(false);
                    on_added.call(url);
                }
                Err(e) => {
                    log::error!("Failed to subscribe: {}", e);
                    error_msg.set(Some(format!("Failed to subscribe: {}", e)));
                    is_saving.set(false);
                }
            }
        });
    };

    // Handle enter key in input
    let handle_keydown = move |e: Event<KeyboardData>| {
        if e.key() == Key::Enter && !*is_fetching.read() {
            do_preview();
        }
    };

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-card border border-border rounded-xl max-w-md w-full shadow-xl overflow-hidden",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "modal-title",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "p-4 border-b border-border",
                    div {
                        class: "flex items-center justify-between",
                        h2 {
                            class: "text-lg font-bold",
                            id: "modal-title",
                            "Add Podcast Feed"
                        }
                        button {
                            class: "p-2 hover:bg-accent rounded-lg transition",
                            onclick: move |_| on_close.call(()),
                            svg {
                                class: "w-5 h-5",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M6 18L18 6M6 6l12 12"
                                }
                            }
                        }
                    }
                }

                // Body
                div {
                    class: "p-4 space-y-4",

                    // URL input
                    div {
                        label {
                            class: "block text-sm font-medium mb-1.5",
                            "RSS Feed URL"
                        }
                        div {
                            class: "flex gap-2",
                            input {
                                class: "flex-1 px-3 py-2 bg-muted rounded-lg border border-border focus:outline-hidden focus:ring-2 focus:ring-primary text-sm",
                                r#type: "url",
                                placeholder: "https://example.com/feed.xml",
                                value: "{feed_url}",
                                oninput: move |e| feed_url.set(e.value()),
                                onkeydown: handle_keydown,
                                disabled: *is_fetching.read() || *is_saving.read(),
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50 text-sm font-medium",
                                onclick: move |_| do_preview(),
                                disabled: *is_fetching.read() || *is_saving.read() || feed_url.read().is_empty(),
                                if *is_fetching.read() {
                                    "Loading..."
                                } else {
                                    "Preview"
                                }
                            }
                        }
                    }

                    // Error message
                    if let Some(err) = error_msg.read().as_ref() {
                        div {
                            class: "p-3 bg-destructive/10 text-destructive rounded-lg text-sm",
                            "{err}"
                        }
                    }

                    // Preview
                    if let Some(ref podcast) = *preview.read() {
                        div {
                            class: "border border-border rounded-lg overflow-hidden",

                            // Podcast header
                            div {
                                class: "flex gap-3 p-3",

                                // Cover image
                                div {
                                    class: "w-16 h-16 rounded-lg overflow-hidden bg-muted shrink-0",
                                    if let Some(ref img) = podcast.image {
                                        img {
                                            src: "{img}",
                                            alt: "{podcast.title}",
                                            class: "w-full h-full object-cover",
                                        }
                                    } else {
                                        div {
                                            class: "w-full h-full flex items-center justify-center text-muted-foreground",
                                            svg {
                                                class: "w-8 h-8",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "1.5",
                                                view_box: "0 0 24 24",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Podcast info
                                div {
                                    class: "flex-1 min-w-0",
                                    h3 {
                                        class: "font-semibold truncate",
                                        "{podcast.title}"
                                    }
                                    if let Some(ref author) = podcast.author {
                                        p {
                                            class: "text-sm text-muted-foreground truncate",
                                            "{author}"
                                        }
                                    }
                                    if podcast.has_v4v {
                                        div {
                                            class: "mt-1",
                                            span {
                                                class: "px-1.5 py-0.5 text-xs font-medium bg-amber-500/20 text-amber-500 rounded",
                                                "V4V"
                                            }
                                        }
                                    }
                                }
                            }

                            // Description
                            if let Some(ref desc) = podcast.description {
                                div {
                                    class: "px-3 pb-3",
                                    p {
                                        class: "text-sm text-muted-foreground line-clamp-2",
                                        "{desc}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Footer
                if preview.read().is_some() {
                    div {
                        class: "p-4 border-t border-border bg-muted/30",
                        div {
                            class: "flex gap-3 justify-end",
                            button {
                                class: "px-4 py-2 rounded-lg hover:bg-accent transition text-sm",
                                onclick: move |_| on_close.call(()),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50 text-sm font-medium",
                                onclick: handle_subscribe,
                                disabled: *is_saving.read(),
                                if *is_saving.read() {
                                    "Subscribing..."
                                } else {
                                    "Subscribe"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Preview data for a podcast feed
#[derive(Clone, Debug)]
struct PodcastPreview {
    podcast_guid: String,
    podcast_id: u64,
    title: String,
    description: Option<String>,
    image: Option<String>,
    author: Option<String>,
    has_v4v: bool,
    feed_url: String,
}
