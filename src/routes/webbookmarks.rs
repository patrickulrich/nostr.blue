use dioxus::prelude::*;
use crate::stores::{nostr_client, webbookmarks};
use crate::components::{WebBookmarkCard, WebBookmarkCardSkeleton, WebBookmarkModal, BookmarkModalMode, ClientInitializing};
use crate::hooks::use_infinite_scroll;
use nostr_sdk::{Event, Timestamp};

#[derive(Clone, Copy, PartialEq, Debug)]
enum FeedType {
    Following,
    Global,
}

impl FeedType {
    fn label(&self) -> &'static str {
        match self {
            FeedType::Following => "Following",
            FeedType::Global => "Global",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum FilterTab {
    All,
    Favorites,
    Archived,
}

impl FilterTab {
    fn label(&self) -> &'static str {
        match self {
            FilterTab::All => "All",
            FilterTab::Favorites => "Favorites",
            FilterTab::Archived => "Archived",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum SortOrder {
    DateAdded,
    DatePublished,
    Title,
}

impl SortOrder {
    #[allow(dead_code)]
    fn label(&self) -> &'static str {
        match self {
            SortOrder::DateAdded => "Date Added",
            SortOrder::DatePublished => "Date Published",
            SortOrder::Title => "Title",
        }
    }
}

#[component]
pub fn WebBookmarks() -> Element {
    // State for feed events
    let mut bookmarks = use_signal(|| Vec::<Event>::new());
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);

    // Filter and sort state
    let mut filter_tab = use_signal(|| FilterTab::All);
    let mut sort_order = use_signal(|| SortOrder::DateAdded);
    let mut search_query = use_signal(|| String::new());
    let mut selected_tag = use_signal(|| Option::<String>::None);

    // Modal state
    let mut show_add_modal = use_signal(|| false);
    let mut show_edit_modal = use_signal(|| false);
    let mut editing_event = use_signal(|| Option::<Event>::None);

    // Quick-add state
    let mut quick_url = use_signal(|| String::new());
    let mut quick_adding = use_signal(|| false);

    // Pagination state for infinite scroll
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Load bookmarks on mount and when refresh is triggered or feed type changes
    use_effect(move || {
        let _ = refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);

        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => webbookmarks::load_following_webbookmarks(None, 50).await,
                FeedType::Global => webbookmarks::load_global_webbookmarks(None, 50).await,
            };

            match result {
                Ok(feed_events) => {
                    // Track oldest timestamp for pagination
                    if let Some(last_event) = feed_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }

                    // Determine if there are more events to load
                    has_more.set(feed_events.len() >= 50);

                    bookmarks.set(feed_events);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Load more function for infinite scroll
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }

        let until = *oldest_timestamp.read();
        let current_feed_type = *feed_type.read();

        loading.set(true);

        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => webbookmarks::load_following_webbookmarks(until, 50).await,
                FeedType::Global => webbookmarks::load_global_webbookmarks(until, 50).await,
            };

            match result {
                Ok(new_bookmarks) => {
                    // Filter out duplicates before appending
                    let current = bookmarks.read().clone();
                    let existing_ids: std::collections::HashSet<_> = current.iter().map(|e| e.id).collect();

                    let filtered_bookmarks: Vec<_> = new_bookmarks
                        .into_iter()
                        .filter(|bookmark| !existing_ids.contains(&bookmark.id))
                        .collect();

                    // Update oldest timestamp from the actual oldest non-duplicate item
                    if let Some(last_event) = filtered_bookmarks.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }

                    // Determine if there are more events to load
                    has_more.set(filtered_bookmarks.len() >= 50);

                    // Append only non-duplicate events
                    let mut updated = current;
                    updated.extend(filtered_bookmarks);
                    bookmarks.set(updated);
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more bookmarks: {}", e);
                    loading.set(false);
                }
            }
        });
    };

    // Set up infinite scroll
    let sentinel_id = use_infinite_scroll(load_more, has_more, loading);

    // Quick add handler
    let handle_quick_add = move |_| {
        let url = quick_url.read().trim().to_string();

        if url.is_empty() {
            return;
        }

        quick_adding.set(true);

        spawn(async move {
            match webbookmarks::add_webbookmark(url, None, None, None, None, vec![]).await {
                Ok(_) => {
                    log::info!("Quick bookmark added successfully");
                    quick_url.set(String::new());
                    quick_adding.set(false);

                    // Refresh bookmarks
                    let current = *refresh_trigger.peek();
                    refresh_trigger.set(current + 1);
                }
                Err(e) => {
                    log::error!("Failed to quick add bookmark: {}", e);
                    quick_adding.set(false);
                }
            }
        });
    };


    // Filter and sort bookmarks
    let filtered_bookmarks = use_memo(move || {
        let bookmarks_list = bookmarks.read();
        let current_tab = *filter_tab.read();
        let search = search_query.read().to_lowercase();
        let tag_filter = selected_tag.read().clone();
        let sort = *sort_order.read();

        // Apply filters
        let mut filtered: Vec<Event> = bookmarks_list
            .iter()
            .filter(|event| {
                // Filter by tab
                match current_tab {
                    FilterTab::All => !webbookmarks::is_archived(event),
                    FilterTab::Favorites => webbookmarks::is_favorite(event) && !webbookmarks::is_archived(event),
                    FilterTab::Archived => webbookmarks::is_archived(event),
                }
            })
            .filter(|event| {
                // Filter by search query
                if search.is_empty() {
                    return true;
                }

                let title = webbookmarks::get_title(event).unwrap_or_default().to_lowercase();
                let url = webbookmarks::get_url(event).unwrap_or_default().to_lowercase();
                let desc = event.content.to_lowercase();

                title.contains(&search) || url.contains(&search) || desc.contains(&search)
            })
            .filter(|event| {
                // Filter by selected tag
                if let Some(ref tag) = tag_filter {
                    webbookmarks::get_hashtags(event).contains(tag)
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        // Apply sorting
        match sort {
            SortOrder::DateAdded => {
                filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
            SortOrder::DatePublished => {
                filtered.sort_by(|a, b| {
                    let a_ts = webbookmarks::get_published_at(a).unwrap_or(Timestamp::from(0));
                    let b_ts = webbookmarks::get_published_at(b).unwrap_or(Timestamp::from(0));
                    b_ts.cmp(&a_ts)
                });
            }
            SortOrder::Title => {
                filtered.sort_by(|a, b| {
                    let a_title = webbookmarks::get_title(a).unwrap_or_default().to_lowercase();
                    let b_title = webbookmarks::get_title(b).unwrap_or_default().to_lowercase();
                    a_title.cmp(&b_title)
                });
            }
        }

        filtered
    });

    // Extract all unique tags for filter dropdown
    let all_tags = use_memo(move || {
        let bookmarks_list = bookmarks.read();
        let mut tags = std::collections::HashSet::new();

        for event in bookmarks_list.iter() {
            for tag in webbookmarks::get_display_hashtags(event) {
                tags.insert(tag);
            }
        }

        let mut tag_vec: Vec<String> = tags.into_iter().collect();
        tag_vec.sort();
        tag_vec
    });

    let bookmark_list = filtered_bookmarks.read();
    let is_loading = *loading.read();
    let error_msg = error.read();

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 space-y-3",

                    // Top row: Feed type dropdown and refresh
                    div {
                        class: "flex items-center justify-between",

                        // Feed type selector (dropdown)
                        div {
                            class: "relative",
                            button {
                                class: "text-xl font-bold flex items-center gap-2 hover:bg-accent px-3 py-1 rounded-lg transition",
                                onclick: move |_| {
                                    let current = *show_dropdown.read();
                                    show_dropdown.set(!current);
                                },
                                "🔖 {feed_type.read().label()}"
                                span {
                                    class: "text-sm",
                                    if *show_dropdown.read() { "▲" } else { "▼" }
                                }
                            }

                            // Dropdown menu
                            if *show_dropdown.read() {
                                div {
                                    class: "absolute top-full left-0 mt-2 bg-card border border-border rounded-lg shadow-lg min-w-[200px] overflow-hidden z-30",

                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::Following);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div {
                                                class: "font-medium",
                                                "Following"
                                            }
                                            div {
                                                class: "text-xs text-muted-foreground",
                                                "Bookmarks from people you follow"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::Following {
                                            span { "✓" }
                                        }
                                    }

                                    div {
                                        class: "border-t border-border"
                                    }

                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::Global);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div {
                                                class: "font-medium",
                                                "Global"
                                            }
                                            div {
                                                class: "text-xs text-muted-foreground",
                                                "Bookmarks from across the network"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::Global {
                                            span { "✓" }
                                        }
                                    }
                                }
                            }
                        }

                        // Actions
                        div {
                            class: "flex items-center gap-2",

                            button {
                                class: "text-sm px-3 py-1 rounded-lg hover:bg-accent transition",
                                onclick: move |_| {
                                    let current = *refresh_trigger.peek();
                                    refresh_trigger.set(current + 1);
                                },
                                "↻ Refresh"
                            }

                            button {
                                class: "text-sm px-3 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                onclick: move |_| show_add_modal.set(true),
                                "+ Add"
                            }
                        }
                    }

                    // Quick add bar
                    div {
                        class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 text-sm bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            r#type: "url",
                            placeholder: "Paste URL to quickly save...",
                            value: "{quick_url}",
                            oninput: move |evt| quick_url.set(evt.value().clone()),
                            onkeypress: {
                                let mut handle_quick = handle_quick_add.clone();
                                move |evt: dioxus::prelude::Event<KeyboardData>| {
                                    if evt.key() == Key::Enter {
                                        handle_quick(());
                                    }
                                }
                            },
                        }
                        button {
                            class: "px-4 py-2 text-sm bg-secondary text-secondary-foreground rounded-lg hover:bg-secondary/90 transition disabled:opacity-50",
                            onclick: {
                                let mut handle_quick = handle_quick_add.clone();
                                move |_| handle_quick(())
                            },
                            disabled: *quick_adding.read() || quick_url.read().trim().is_empty(),
                            if *quick_adding.read() { "Saving..." } else { "Quick Save" }
                        }
                    }

                    // Filter row: Tabs, Search, Sort, Tag filter
                    div {
                        class: "flex flex-wrap items-center gap-3",

                        // Filter tabs
                        div {
                            class: "flex gap-1 bg-muted p-1 rounded-lg",
                            for tab in [FilterTab::All, FilterTab::Favorites, FilterTab::Archived] {
                                button {
                                    key: "{tab:?}",
                                    class: if *filter_tab.read() == tab {
                                        "px-3 py-1 text-sm rounded bg-background shadow-sm font-medium"
                                    } else {
                                        "px-3 py-1 text-sm rounded hover:bg-background/50 transition"
                                    },
                                    onclick: move |_| filter_tab.set(tab),
                                    "{tab.label()}"
                                }
                            }
                        }

                        // Search
                        input {
                            class: "flex-1 min-w-[200px] px-3 py-1 text-sm bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search bookmarks...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value().clone()),
                        }

                        // Tag filter
                        if !all_tags.read().is_empty() {
                            select {
                                class: "px-3 py-1 text-sm bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                onchange: move |evt| {
                                    let value = evt.value().clone();
                                    if value.is_empty() {
                                        selected_tag.set(None);
                                    } else {
                                        selected_tag.set(Some(value));
                                    }
                                },
                                option {
                                    value: "",
                                    selected: selected_tag.read().is_none(),
                                    "All Tags"
                                }
                                for tag in all_tags.read().iter() {
                                    option {
                                        key: "{tag}",
                                        value: "{tag}",
                                        selected: selected_tag.read().as_ref() == Some(tag),
                                        "#{tag}"
                                    }
                                }
                            }
                        }

                        // Sort
                        select {
                            class: "px-3 py-1 text-sm bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                            onchange: move |evt| {
                                let value = evt.value();
                                sort_order.set(match value.as_str() {
                                    "published" => SortOrder::DatePublished,
                                    "title" => SortOrder::Title,
                                    _ => SortOrder::DateAdded,
                                });
                            },
                            option {
                                value: "added",
                                selected: *sort_order.read() == SortOrder::DateAdded,
                                "Sort: Date Added"
                            }
                            option {
                                value: "published",
                                selected: *sort_order.read() == SortOrder::DatePublished,
                                "Sort: Date Published"
                            }
                            option {
                                value: "title",
                                selected: *sort_order.read() == SortOrder::Title,
                                "Sort: Title"
                            }
                        }
                    }
                }
            }

            // Error message
            if let Some(err) = error_msg.as_ref() {
                div {
                    class: "p-4 bg-destructive/10 border border-destructive text-destructive",
                    p { "Failed to load bookmarks: {err}" }
                    button {
                        class: "mt-2 px-3 py-1 bg-destructive text-destructive-foreground rounded-lg",
                        onclick: move |_| {
                            let current = *refresh_trigger.peek();
                            refresh_trigger.set(current + 1);
                        },
                        "Try Again"
                    }
                }
            }

            // Bookmarks grid
            div {
                class: "p-4",

                // Initial loading state
                if !*nostr_client::CLIENT_INITIALIZED.read() || (is_loading && bookmark_list.is_empty()) {
                    ClientInitializing {}
                } else if bookmark_list.is_empty() {
                    // Empty state
                    div {
                        class: "text-center py-12",
                        div {
                            class: "text-6xl mb-4",
                            "🔖"
                        }
                        h3 {
                            class: "text-xl font-semibold mb-2",
                            "No Bookmarks Found"
                        }
                        p {
                            class: "text-muted-foreground text-sm mb-4",
                            match *filter_tab.read() {
                                FilterTab::All => "Start saving web pages you want to read later.",
                                FilterTab::Favorites => "You haven't favorited any bookmarks yet.",
                                FilterTab::Archived => "No archived bookmarks.",
                            }
                        }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90",
                            onclick: move |_| show_add_modal.set(true),
                            "+ Add Your First Bookmark"
                        }
                    }
                } else {
                    // Bookmark grid
                    div {
                        class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                        for bookmark in bookmark_list.iter() {
                            WebBookmarkCard {
                                key: "{bookmark.id}",
                                event: bookmark.clone(),
                                on_edit: None,
                            }
                        }
                    }

                    // Infinite scroll sentinel
                    if *has_more.read() {
                        div {
                            id: "{sentinel_id}",
                            class: "h-20 flex items-center justify-center",
                            if is_loading {
                                div {
                                    class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 w-full",
                                    for _ in 0..3 {
                                        WebBookmarkCardSkeleton {}
                                    }
                                }
                            }
                        }
                    } else {
                        // End of feed indicator
                        div {
                            class: "text-center py-8 text-muted-foreground text-sm",
                            "You've reached the end"
                        }
                    }
                }
            }
        }

        // Modals
        if *show_add_modal.read() {
            WebBookmarkModal {
                mode: BookmarkModalMode::Add,
                event: None,
                on_close: move |_| {
                    show_add_modal.set(false);
                    // Refresh bookmarks after adding
                    let current = *refresh_trigger.peek();
                    refresh_trigger.set(current + 1);
                }
            }
        }

        if *show_edit_modal.read() {
            WebBookmarkModal {
                mode: BookmarkModalMode::Edit,
                event: editing_event.read().clone(),
                on_close: move |_| {
                    show_edit_modal.set(false);
                    editing_event.set(None);
                    // Refresh bookmarks after editing
                    let current = *refresh_trigger.peek();
                    refresh_trigger.set(current + 1);
                }
            }
        }
    }
}
