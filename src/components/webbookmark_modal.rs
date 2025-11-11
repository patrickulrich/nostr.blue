use dioxus::prelude::*;
use nostr_sdk::Event as NostrEvent;
use crate::stores::webbookmarks::{add_webbookmark, update_webbookmark, get_url, get_title, get_image, get_published_at, get_hashtags};
use crate::utils::url_metadata::fetch_url_metadata;

/// Mode for the bookmark modal (Add or Edit)
#[derive(Clone, Copy, PartialEq)]
pub enum BookmarkModalMode {
    Add,
    Edit,
}

/// Modal for adding or editing a web bookmark
#[component]
pub fn WebBookmarkModal(
    /// Mode: Add or Edit
    mode: BookmarkModalMode,
    /// Event to edit (only for Edit mode)
    event: Option<NostrEvent>,
    /// Handler to close the modal
    on_close: EventHandler<()>,
) -> Element {
    // Form state
    let mut url_input = use_signal(|| String::new());
    let mut title_input = use_signal(|| String::new());
    let mut description_input = use_signal(|| String::new());
    let mut image_input = use_signal(|| String::new());
    let mut tags_input = use_signal(|| String::new());
    let mut published_at_input = use_signal(|| String::new());

    // Track reserved tags (favorite, archived) to preserve them on save
    let mut reserved_tags = use_signal(|| Vec::<String>::new());

    // UI state
    let mut is_fetching_metadata = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut auto_fetched = use_signal(|| false);

    // Initialize form with existing event data (Edit mode)
    use_effect(move || {
        if mode == BookmarkModalMode::Edit {
            if let Some(ref evt) = event {
                // Populate form fields
                if let Some(url) = get_url(evt) {
                    url_input.set(url);
                }
                if let Some(title) = get_title(evt) {
                    title_input.set(title);
                }
                if !evt.content.is_empty() {
                    description_input.set(evt.content.clone());
                }
                if let Some(image) = get_image(evt) {
                    image_input.set(image);
                }

                // Separate reserved tags from user-editable tags
                let all_hashtags = get_hashtags(evt);
                let reserved: Vec<String> = all_hashtags
                    .iter()
                    .filter(|tag| *tag == "favorite" || *tag == "archived")
                    .cloned()
                    .collect();
                let user_tags: Vec<String> = all_hashtags
                    .into_iter()
                    .filter(|tag| tag != "favorite" && tag != "archived")
                    .collect();

                reserved_tags.set(reserved);
                tags_input.set(user_tags.join(", "));

                if let Some(published_ts) = get_published_at(evt) {
                    // Format timestamp as YYYY-MM-DD for date input
                    use chrono::DateTime;
                    if let Some(dt) = DateTime::from_timestamp(published_ts.as_secs() as i64, 0) {
                        published_at_input.set(dt.format("%Y-%m-%d").to_string());
                    }
                }

                auto_fetched.set(true);
            }
        }
    });

    // Handle metadata fetch
    let handle_fetch_metadata = move |_| {
        let url = url_input.read().trim().to_string();

        if url.is_empty() {
            error_msg.set(Some("Please enter a URL first".to_string()));
            return;
        }

        is_fetching_metadata.set(true);
        error_msg.set(None);

        spawn(async move {
            match fetch_url_metadata(url).await {
                Ok(metadata) => {
                    log::info!("Fetched metadata successfully");

                    // Only set fields if they're empty (don't override user edits)
                    if title_input.read().is_empty() {
                        if let Some(title) = metadata.title {
                            title_input.set(title);
                        }
                    }

                    if description_input.read().is_empty() {
                        if let Some(desc) = metadata.description {
                            description_input.set(desc);
                        }
                    }

                    if image_input.read().is_empty() {
                        if let Some(img) = metadata.image {
                            image_input.set(img);
                        }
                    }

                    auto_fetched.set(true);
                    is_fetching_metadata.set(false);
                }
                Err(e) => {
                    log::error!("Failed to fetch metadata: {}", e);
                    error_msg.set(Some(format!("Failed to fetch page metadata: {}", e)));
                    is_fetching_metadata.set(false);
                }
            }
        });
    };

    // Auto-fetch metadata when URL is pasted (Add mode only)
    let handle_url_change = move |evt: Event<FormData>| {
        let new_url = evt.value().clone();
        url_input.set(new_url.clone());

        // Reset auto_fetched flag when URL changes to re-enable auto-fetch
        auto_fetched.set(false);

        // Auto-fetch if this is a new bookmark and we haven't fetched yet
        if mode == BookmarkModalMode::Add && !*auto_fetched.read() && !new_url.trim().is_empty() {
            // Check if it looks like a complete URL
            if new_url.contains('.') && (new_url.starts_with("http") || !new_url.contains(' ')) {
                // Trigger auto-fetch after a short delay (debounce)
                spawn(async move {
                    gloo_timers::future::TimeoutFuture::new(500).await;

                    // Only fetch if URL hasn't changed and we still haven't fetched
                    if url_input.read().trim() == new_url.trim() && !*auto_fetched.read() {
                        is_fetching_metadata.set(true);
                        error_msg.set(None);

                        match fetch_url_metadata(new_url.clone()).await {
                            Ok(metadata) => {
                                if let Some(title) = metadata.title {
                                    title_input.set(title);
                                }
                                if let Some(desc) = metadata.description {
                                    description_input.set(desc);
                                }
                                if let Some(img) = metadata.image {
                                    image_input.set(img);
                                }
                                auto_fetched.set(true);
                                is_fetching_metadata.set(false);
                            }
                            Err(e) => {
                                log::warn!("Auto-fetch failed: {}", e);
                                is_fetching_metadata.set(false);
                            }
                        }
                    }
                });
            }
        }
    };

    // Handle save
    let handle_save = move |_| {
        let url = url_input.read().trim().to_string();
        let title = title_input.read().trim().to_string();
        let description = description_input.read().trim().to_string();
        let image = image_input.read().trim().to_string();
        let tags_str = tags_input.read().trim().to_string();
        let published_date = published_at_input.read().trim().to_string();

        // Validation
        if url.is_empty() {
            error_msg.set(Some("URL is required".to_string()));
            return;
        }

        // Parse tags (comma-separated)
        let mut hashtags: Vec<String> = tags_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Merge reserved tags back in (for Edit mode)
        if mode == BookmarkModalMode::Edit {
            for reserved_tag in reserved_tags.read().iter() {
                if !hashtags.contains(reserved_tag) {
                    hashtags.push(reserved_tag.clone());
                }
            }
        }

        // Deduplicate tags
        hashtags.sort();
        hashtags.dedup();

        // Parse published date to timestamp
        let published_ts = if !published_date.is_empty() {
            use chrono::NaiveDate;
            NaiveDate::parse_from_str(&published_date, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|dt| dt.and_utc().timestamp() as u64)
        } else {
            None
        };

        is_saving.set(true);
        error_msg.set(None);

        spawn(async move {
            let result = match mode {
                BookmarkModalMode::Add => {
                    add_webbookmark(
                        url,
                        if title.is_empty() { None } else { Some(title) },
                        if description.is_empty() { None } else { Some(description) },
                        if image.is_empty() { None } else { Some(image) },
                        published_ts,
                        hashtags,
                    ).await
                }
                BookmarkModalMode::Edit => {
                    update_webbookmark(
                        url,
                        if title.is_empty() { None } else { Some(title) },
                        if description.is_empty() { None } else { Some(description) },
                        if image.is_empty() { None } else { Some(image) },
                        published_ts,
                        hashtags,
                    ).await
                }
            };

            match result {
                Ok(_) => {
                    log::info!("Bookmark saved successfully");
                    is_saving.set(false);
                    on_close.call(());
                }
                Err(e) => {
                    log::error!("Failed to save bookmark: {}", e);
                    error_msg.set(Some(format!("Failed to save: {}", e)));
                    is_saving.set(false);
                }
            }
        });
    };

    let modal_title = match mode {
        BookmarkModalMode::Add => "Add Web Bookmark",
        BookmarkModalMode::Edit => "Edit Web Bookmark",
    };

    rsx! {
        // Modal backdrop
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-card rounded-lg shadow-xl max-w-2xl w-full max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between p-4 border-b border-border",
                    h2 {
                        class: "text-xl font-bold",
                        "{modal_title}"
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                // Form
                div {
                    class: "p-4 space-y-4",

                    // Error message
                    if let Some(err) = error_msg.read().as_ref() {
                        div {
                            class: "p-3 bg-destructive/10 border border-destructive text-destructive rounded-lg text-sm",
                            "{err}"
                        }
                    }

                    // URL input with fetch button
                    div {
                        class: "space-y-2",
                        label {
                            class: "text-sm font-medium",
                            "URL *"
                        }
                        div {
                            class: "flex gap-2",
                            input {
                                class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                r#type: "url",
                                placeholder: "https://example.com/article",
                                value: "{url_input}",
                                oninput: handle_url_change,
                                disabled: mode == BookmarkModalMode::Edit,
                            }
                            if mode == BookmarkModalMode::Add {
                                button {
                                    class: "px-4 py-2 bg-secondary text-secondary-foreground rounded-lg hover:bg-secondary/90 transition disabled:opacity-50",
                                    r#type: "button",
                                    onclick: handle_fetch_metadata,
                                    disabled: *is_fetching_metadata.read() || url_input.read().trim().is_empty(),
                                    if *is_fetching_metadata.read() {
                                        "Fetching..."
                                    } else {
                                        "🔄 Fetch Metadata"
                                    }
                                }
                            }
                        }
                        p {
                            class: "text-xs text-muted-foreground",
                            if mode == BookmarkModalMode::Add {
                                "Paste a URL and we'll automatically fetch the title, description, and image"
                            } else {
                                "URL cannot be changed when editing"
                            }
                        }
                    }

                    // Title input
                    div {
                        class: "space-y-2",
                        label {
                            class: "text-sm font-medium",
                            "Title"
                        }
                        input {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Article title",
                            value: "{title_input}",
                            oninput: move |evt| title_input.set(evt.value().clone()),
                        }
                    }

                    // Description input
                    div {
                        class: "space-y-2",
                        label {
                            class: "text-sm font-medium",
                            "Description"
                        }
                        textarea {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary resize-none",
                            rows: "3",
                            placeholder: "Brief description or notes",
                            value: "{description_input}",
                            oninput: move |evt| description_input.set(evt.value().clone()),
                        }
                    }

                    // Image URL input
                    div {
                        class: "space-y-2",
                        label {
                            class: "text-sm font-medium",
                            "Image URL"
                        }
                        input {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            r#type: "url",
                            placeholder: "https://example.com/image.jpg",
                            value: "{image_input}",
                            oninput: move |evt| image_input.set(evt.value().clone()),
                        }
                        // Image preview
                        if !image_input.read().is_empty() {
                            div {
                                class: "mt-2 border border-border rounded-lg overflow-hidden",
                                img {
                                    src: "{image_input}",
                                    alt: "Preview",
                                    class: "w-full h-40 object-cover",
                                    loading: "lazy",
                                }
                            }
                        }
                    }

                    // Tags input
                    div {
                        class: "space-y-2",
                        label {
                            class: "text-sm font-medium",
                            "Tags"
                        }
                        input {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "javascript, tutorial, web development",
                            value: "{tags_input}",
                            oninput: move |evt| tags_input.set(evt.value().clone()),
                        }
                        p {
                            class: "text-xs text-muted-foreground",
                            "Comma-separated tags (e.g., tech, article, rust)"
                        }
                    }

                    // Published date input
                    div {
                        class: "space-y-2",
                        label {
                            class: "text-sm font-medium",
                            "Published Date"
                        }
                        input {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            r#type: "date",
                            value: "{published_at_input}",
                            oninput: move |evt| published_at_input.set(evt.value().clone()),
                        }
                        p {
                            class: "text-xs text-muted-foreground",
                            "Optional: When was this content published?"
                        }
                    }
                }

                // Footer
                div {
                    class: "flex items-center justify-end gap-3 p-4 border-t border-border",
                    button {
                        class: "px-4 py-2 text-sm rounded-lg hover:bg-accent transition",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                        onclick: handle_save,
                        disabled: *is_saving.read() || url_input.read().trim().is_empty(),
                        if *is_saving.read() {
                            "Saving..."
                        } else {
                            match mode {
                                BookmarkModalMode::Add => "Add Bookmark",
                                BookmarkModalMode::Edit => "Save Changes",
                            }
                        }
                    }
                }
            }
        }
    }
}
