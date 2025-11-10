use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::routes::Route;
use nostr_sdk::{EventBuilder, Kind, Tag, Timestamp};
use nostr::{TagKind};
use url::Url;

#[component]
pub fn LiveStreamNew() -> Element {
    let navigator = navigator();
    let mut title = use_signal(|| String::new());
    let mut summary = use_signal(|| String::new());
    let mut image_url = use_signal(|| String::new());
    let mut stream_url = use_signal(|| String::new());
    let mut hashtags = use_signal(|| String::new());
    let mut status = use_signal(|| "planned".to_string());
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);

    // Check if user is authenticated
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

    // Validation with efficient is_empty() and URL parsing
    let can_publish = {
        let title_val = title.read();
        let stream_url_val = stream_url.read();
        let is_pub = *is_publishing.read();

        !title_val.is_empty()
            && !stream_url_val.is_empty()
            && Url::parse(&*stream_url_val)
                .map(|u| {
                    let scheme = u.scheme();
                    scheme == "http" || scheme == "https" || scheme == "rtmp" || scheme == "rtmps"
                })
                .unwrap_or(false)
            && !is_pub
    };

    // Handle close
    let handle_close = move |_| {
        navigator.push(Route::VideosLive {});
    };

    // Handle publishing
    let handle_publish = move |_| {
        if !can_publish {
            return;
        }

        let title_val = title.read().clone();
        let summary_val = summary.read().clone();
        let image_url_val = image_url.read().clone();
        let stream_url_val = stream_url.read().clone();
        let hashtags_val = hashtags.read().clone();
        let status_val = status.read().clone();

        is_publishing.set(true);
        error_message.set(None);

        spawn(async move {
            match publish_live_stream(
                title_val,
                summary_val,
                image_url_val,
                stream_url_val,
                hashtags_val,
                status_val,
            ).await {
                Ok(naddr) => {
                    log::info!("Live stream published successfully: {}", naddr);
                    is_publishing.set(false);
                    navigator.push(Route::LiveStreamDetail { note_id: naddr });
                }
                Err(e) => {
                    log::error!("Failed to publish live stream: {}", e);
                    error_message.set(Some(format!("Failed to publish: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };

    // Redirect if not authenticated
    if !*is_authenticated.read() {
        use_effect(move || {
            navigator.push(Route::Home {});
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
                            crate::components::icons::ArrowLeftIcon { class: "w-6 h-6" }
                        }
                        h1 {
                            class: "text-2xl font-bold",
                            "Create Live Stream"
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
                class: "max-w-4xl mx-auto px-4 py-8",

                // Error message
                if let Some(err) = error_message.read().as_ref() {
                    div {
                        class: "mb-4 p-4 bg-red-100 dark:bg-red-900/20 border border-red-300 dark:border-red-800 rounded-lg text-red-800 dark:text-red-200",
                        "{err}"
                    }
                }

                // Help text
                div {
                    class: "mb-6 p-4 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg",
                    h3 {
                        class: "font-semibold mb-2 text-blue-900 dark:text-blue-100",
                        "Getting Started with Live Streaming"
                    }
                    ul {
                        class: "list-disc list-inside space-y-1 text-sm text-blue-800 dark:text-blue-200",
                        li { "Set up your streaming software (OBS, StreamYard, etc.)" }
                        li { "Get your HLS or RTMP stream URL from your streaming service" }
                        li { "Enter your stream details below" }
                        li { "Click Publish to create your livestream event" }
                        li { "Start streaming when ready!" }
                    }
                }

                // Form
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
                            class: "w-full px-4 py-2 bg-input border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "My Awesome Livestream",
                            value: "{title.read()}",
                            oninput: move |e| title.set(e.value().clone())
                        }
                    }

                    // Summary/Description
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Description"
                        }
                        textarea {
                            class: "w-full px-4 py-2 bg-input border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 min-h-[120px]",
                            placeholder: "Describe what you'll be streaming...",
                            value: "{summary.read()}",
                            oninput: move |e| summary.set(e.value().clone())
                        }
                    }

                    // Stream URL
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Stream URL (HLS/RTMP) *"
                        }
                        input {
                            r#type: "url",
                            class: "w-full px-4 py-2 bg-input border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono text-sm",
                            placeholder: "https://example.com/stream.m3u8",
                            value: "{stream_url.read()}",
                            oninput: move |e| stream_url.set(e.value().clone())
                        }
                        p {
                            class: "mt-1 text-xs text-muted-foreground",
                            "Enter the HLS (.m3u8) or RTMP URL from your streaming software"
                        }
                    }

                    // Thumbnail Image URL
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Thumbnail Image URL"
                        }
                        input {
                            r#type: "url",
                            class: "w-full px-4 py-2 bg-input border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "https://example.com/thumbnail.jpg",
                            value: "{image_url.read()}",
                            oninput: move |e| image_url.set(e.value().clone())
                        }
                        if !image_url.read().is_empty() {
                            div {
                                class: "mt-3",
                                img {
                                    src: "{image_url.read()}",
                                    class: "w-full max-w-md rounded-lg border border-border",
                                    alt: "Thumbnail preview"
                                }
                            }
                        }
                    }

                    // Hashtags
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Tags"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-input border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "gaming, music, tech (comma-separated)",
                            value: "{hashtags.read()}",
                            oninput: move |e| hashtags.set(e.value().clone())
                        }
                        p {
                            class: "mt-1 text-xs text-muted-foreground",
                            "Add tags to help people discover your stream"
                        }
                    }

                    // Status
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Status"
                        }
                        select {
                            class: "w-full px-4 py-2 bg-input border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            value: "{status.read()}",
                            onchange: move |e| status.set(e.value().clone()),
                            option {
                                value: "planned",
                                "Planned (Upcoming)"
                            }
                            option {
                                value: "live",
                                "Live (Start streaming now)"
                            }
                        }
                        p {
                            class: "mt-1 text-xs text-muted-foreground",
                            if *status.read() == "live" {
                                "Make sure your stream is running before setting status to Live"
                            } else {
                                "Set to 'Planned' to announce an upcoming stream"
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn publish_live_stream(
    title: String,
    summary: String,
    image_url: String,
    stream_url: String,
    hashtags: String,
    status: String,
) -> Result<String, String> {
    let client = nostr_client::get_client()
        .ok_or_else(|| "Client not initialized".to_string())?;

    // Generate unique identifier (d tag) with high-resolution time and random component
    let now = chrono::Utc::now();
    let timestamp_ms = now.timestamp_millis();
    let random_component = uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("").to_string();
    let d_tag = format!("stream-{}-{}", timestamp_ms, random_component);

    // Build event with tags
    let mut builder = EventBuilder::new(Kind::from(30311), "");

    // Add required tags
    builder = builder.tag(Tag::custom(TagKind::d(), vec![d_tag.clone()]));
    builder = builder.tag(Tag::custom(TagKind::custom("title"), vec![title.clone()]));

    if !summary.is_empty() {
        builder = builder.tag(Tag::custom(TagKind::custom("summary"), vec![summary.clone()]));
    }

    if !image_url.is_empty() {
        builder = builder.tag(Tag::custom(TagKind::custom("image"), vec![image_url.clone()]));
    }

    builder = builder.tag(Tag::custom(TagKind::custom("streaming"), vec![stream_url.clone()]));
    builder = builder.tag(Tag::custom(TagKind::custom("status"), vec![status.clone()]));

    // Add start time (current time)
    let now = Timestamp::now().as_secs();
    builder = builder.tag(Tag::custom(TagKind::custom("starts"), vec![now.to_string()]));

    // Add hashtags
    if !hashtags.is_empty() {
        for tag in hashtags.split(',') {
            let trimmed = tag.trim();
            if !trimmed.is_empty() {
                builder = builder.tag(Tag::custom(TagKind::t(), vec![trimmed.to_string()]));
            }
        }
    }

    // Publish the event
    let _output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    // Get the author pubkey
    let signer = client.signer().await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let pubkey = signer.get_public_key().await
        .map_err(|e| format!("Failed to get public key: {}", e))?;

    // Return naddr format for navigation
    let naddr = format!("30311:{}:{}", pubkey.to_hex(), d_tag);

    Ok(naddr)
}
