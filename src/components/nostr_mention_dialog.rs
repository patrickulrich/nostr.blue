//! Nostr Mention Dialog Component
//!
//! A dialog for searching and inserting Nostr references (npub, note, naddr).
//! Provides tabbed interface for searching users, notes, and articles.

use dioxus::prelude::*;
use dioxus_core::Task;
use nostr_sdk::prelude::*;

use crate::components::icons::SearchIcon;
use crate::components::modal::{Modal, ModalHeader};
use crate::services::content_search::{search_articles, search_text_notes, ContentSearchResult};
use crate::services::profile_search::{
    get_contact_pubkeys, search_cached_profiles, search_profiles,
};
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::format_time_ago;

/// Selection result from the mention dialog
#[derive(Clone, Debug)]
pub struct MentionSelection {
    /// The nostr: URI to insert (e.g., "nostr:npub1...")
    pub uri: String,
    /// Display text (for preview purposes) - kept for future mention preview feature
    #[allow(dead_code)]
    pub display_text: String,
    /// Type of mention - kept for future mention type differentiation in UI
    #[allow(dead_code)]
    pub mention_type: MentionType,
}

/// Type of Nostr mention
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MentionType {
    User,
    Note,
    Article,
}

/// User search result with PartialEq for use in signals
#[derive(Clone, Debug, PartialEq)]
struct UserSearchResult {
    pubkey: PublicKey,
    name: Option<String>,
    display_name: Option<String>,
    picture: Option<String>,
    is_contact: bool,
}

impl UserSearchResult {
    fn from_profile(p: crate::services::profile_search::ProfileSearchResult) -> Self {
        Self {
            pubkey: p.pubkey,
            name: p.name,
            display_name: p.display_name,
            picture: p.picture,
            is_contact: p.is_contact,
        }
    }

    fn get_display_name(&self) -> String {
        if let Some(ref display_name) = self.display_name {
            if !display_name.is_empty() {
                return display_name.clone();
            }
        }
        if let Some(ref name) = self.name {
            if !name.is_empty() {
                return name.clone();
            }
        }
        let hex = self.pubkey.to_hex();
        format!("{}...{}", &hex[..8], &hex[hex.len() - 8..])
    }

    fn get_username(&self) -> Option<String> {
        self.name.clone()
    }
}

/// Tab selection for the dialog
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum MentionTab {
    #[default]
    Users,
    Notes,
    Articles,
}

#[derive(Props, Clone, PartialEq)]
pub struct NostrMentionDialogProps {
    /// Signal controlling dialog visibility
    pub open: Signal<bool>,
    /// Callback when a mention is selected
    pub on_select: EventHandler<MentionSelection>,
}

#[component]
pub fn NostrMentionDialog(props: NostrMentionDialogProps) -> Element {
    let mut open = props.open;
    let mut active_tab = use_signal(|| MentionTab::Users);
    let mut search_query = use_signal(String::new);

    // User search state (store as tuples to avoid PartialEq issues)
    let mut user_results = use_signal(Vec::<UserSearchResult>::new);
    let mut is_searching_users = use_signal(|| false);
    let mut user_search_task: Signal<Option<Task>> = use_signal(|| None);

    // Note search state
    let mut note_results = use_signal(Vec::<ContentSearchResult>::new);
    let mut is_searching_notes = use_signal(|| false);
    let mut note_search_task: Signal<Option<Task>> = use_signal(|| None);

    // Article search state
    let mut article_results = use_signal(Vec::<ContentSearchResult>::new);
    let mut is_searching_articles = use_signal(|| false);
    let mut article_search_task: Signal<Option<Task>> = use_signal(|| None);

    // Contact pubkeys for relevance sorting
    let mut contact_pubkeys = use_signal(Vec::<PublicKey>::new);

    // Load contacts on mount
    use_effect(move || {
        spawn(async move {
            let contacts = get_contact_pubkeys().await;
            contact_pubkeys.set(contacts);
        });
    });

    // Reset state when dialog opens
    use_effect(move || {
        if *open.read() {
            search_query.set(String::new());
            user_results.set(Vec::new());
            note_results.set(Vec::new());
            article_results.set(Vec::new());
            active_tab.set(MentionTab::Users);
        }
    });

    // Handle search input - searches based on active tab
    let mut handle_search = move |query: String| {
        search_query.set(query.clone());

        // Cancel any pending search tasks
        if let Some(task) = user_search_task.take() {
            task.cancel();
        }
        if let Some(task) = note_search_task.take() {
            task.cancel();
        }
        if let Some(task) = article_search_task.take() {
            task.cancel();
        }

        if query.is_empty() {
            user_results.set(Vec::new());
            note_results.set(Vec::new());
            article_results.set(Vec::new());
            is_searching_users.set(false);
            is_searching_notes.set(false);
            is_searching_articles.set(false);
            return;
        }

        let tab = *active_tab.read();
        let contacts = contact_pubkeys.read().clone();
        let query_snapshot = query.clone();

        match tab {
            MentionTab::Users => {
                // First, search cached profiles synchronously
                let cached_results = search_cached_profiles(&query, 20, &contacts, &[]);
                let mapped: Vec<UserSearchResult> = cached_results
                    .into_iter()
                    .map(UserSearchResult::from_profile)
                    .collect();
                user_results.set(mapped);

                // Then start async relay search
                is_searching_users.set(true);
                let new_task = spawn(async move {
                    // Debounce
                    #[cfg(target_family = "wasm")]
                    gloo_timers::future::TimeoutFuture::new(300).await;
                    #[cfg(not(target_family = "wasm"))]
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

                    match search_profiles(&query_snapshot, 20, true).await {
                        Ok(results) => {
                            if search_query.read().as_str() == query_snapshot.as_str() {
                                let mapped: Vec<UserSearchResult> = results
                                    .into_iter()
                                    .map(UserSearchResult::from_profile)
                                    .collect();
                                user_results.set(mapped);
                                is_searching_users.set(false);
                            }
                        }
                        Err(e) => {
                            if search_query.read().as_str() == query_snapshot.as_str() {
                                log::warn!("Profile search failed: {}", e);
                                is_searching_users.set(false);
                            }
                        }
                    }
                });
                user_search_task.set(Some(new_task));
            }
            MentionTab::Notes => {
                is_searching_notes.set(true);
                let new_task = spawn(async move {
                    // Debounce
                    #[cfg(target_family = "wasm")]
                    gloo_timers::future::TimeoutFuture::new(300).await;
                    #[cfg(not(target_family = "wasm"))]
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

                    match search_text_notes(&query_snapshot, 20, &contacts).await {
                        Ok(results) => {
                            if search_query.read().as_str() == query_snapshot.as_str() {
                                note_results.set(results);
                                is_searching_notes.set(false);
                            }
                        }
                        Err(e) => {
                            if search_query.read().as_str() == query_snapshot.as_str() {
                                log::warn!("Note search failed: {}", e);
                                is_searching_notes.set(false);
                            }
                        }
                    }
                });
                note_search_task.set(Some(new_task));
            }
            MentionTab::Articles => {
                is_searching_articles.set(true);
                let new_task = spawn(async move {
                    // Debounce
                    #[cfg(target_family = "wasm")]
                    gloo_timers::future::TimeoutFuture::new(300).await;
                    #[cfg(not(target_family = "wasm"))]
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

                    match search_articles(&query_snapshot, 20, &contacts).await {
                        Ok(results) => {
                            if search_query.read().as_str() == query_snapshot.as_str() {
                                article_results.set(results);
                                is_searching_articles.set(false);
                            }
                        }
                        Err(e) => {
                            if search_query.read().as_str() == query_snapshot.as_str() {
                                log::warn!("Article search failed: {}", e);
                                is_searching_articles.set(false);
                            }
                        }
                    }
                });
                article_search_task.set(Some(new_task));
            }
        }
    };

    // Handle tab change - trigger search if there's a query
    let mut handle_tab_change = move |tab: MentionTab| {
        active_tab.set(tab);
        let query = search_query.read().clone();
        if !query.is_empty() {
            handle_search(query);
        }
    };

    // Handle user selection
    let mut handle_user_select = move |profile: UserSearchResult| {
        let npub = profile
            .pubkey
            .to_bech32()
            .unwrap_or_else(|_| profile.pubkey.to_hex());
        let uri = format!("nostr:{}", npub);
        let display_text = profile.get_display_name();

        props.on_select.call(MentionSelection {
            uri,
            display_text,
            mention_type: MentionType::User,
        });
        open.set(false);
    };

    // Handle note selection
    let mut handle_note_select = move |result: ContentSearchResult| {
        let nevent = Nip19Event::new(result.event.id)
            .author(result.event.pubkey)
            .to_bech32()
            .unwrap_or_else(|_| result.event.id.to_hex());
        let uri = format!("nostr:{}", nevent);
        let display_text = result.event.content.chars().take(50).collect::<String>()
            + if result.event.content.len() > 50 {
                "..."
            } else {
                ""
            };

        props.on_select.call(MentionSelection {
            uri,
            display_text,
            mention_type: MentionType::Note,
        });
        open.set(false);
    };

    // Handle article selection
    let mut handle_article_select = move |result: ContentSearchResult| {
        // Extract d-tag for addressable event
        let d_tag = result
            .event
            .tags
            .iter()
            .find(|t| t.kind() == TagKind::d())
            .and_then(|t| t.content())
            .map(String::from)
            .unwrap_or_default();

        let coordinate =
            Coordinate::new(Kind::from(30023), result.event.pubkey).identifier(d_tag.clone());

        let naddr = Nip19Coordinate::new(coordinate, vec![])
            .to_bech32()
            .unwrap_or_else(|_| result.event.id.to_hex());
        let uri = format!("nostr:{}", naddr);

        // Get title from tags
        let title = result
            .event
            .tags
            .iter()
            .find(|t| t.kind() == TagKind::Title)
            .and_then(|t| t.content())
            .map(String::from)
            .unwrap_or_else(|| "Untitled".to_string());

        props.on_select.call(MentionSelection {
            uri,
            display_text: title,
            mention_type: MentionType::Article,
        });
        open.set(false);
    };

    let close_modal = move |_| {
        open.set(false);
    };

    let is_searching = match *active_tab.read() {
        MentionTab::Users => *is_searching_users.read(),
        MentionTab::Notes => *is_searching_notes.read(),
        MentionTab::Articles => *is_searching_articles.read(),
    };

    rsx! {
        Modal {
            open: open,

            div {
                class: "w-full max-w-lg max-h-[70vh] flex flex-col",

                ModalHeader {
                    title: "Insert Nostr Mention".to_string(),
                    on_close: close_modal,
                }

                div {
                    class: "flex-1 flex flex-col overflow-hidden",

                    // Tab bar
                    div {
                        class: "flex border-b border-border",

                        TabButton {
                            active: *active_tab.read() == MentionTab::Users,
                            onclick: move |_| handle_tab_change(MentionTab::Users),
                            "Users"
                        }
                        TabButton {
                            active: *active_tab.read() == MentionTab::Notes,
                            onclick: move |_| handle_tab_change(MentionTab::Notes),
                            "Notes"
                        }
                        TabButton {
                            active: *active_tab.read() == MentionTab::Articles,
                            onclick: move |_| handle_tab_change(MentionTab::Articles),
                            "Articles"
                        }
                    }

                    // Search input
                    div {
                        class: "p-3 border-b border-border",

                        div {
                            class: "relative",

                            div {
                                class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
                                SearchIcon { class: "w-4 h-4" }
                            }

                            input {
                                class: "w-full pl-10 pr-4 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring text-sm",
                                r#type: "text",
                                placeholder: match *active_tab.read() {
                                    MentionTab::Users => "Search users by name or npub...",
                                    MentionTab::Notes => "Search notes...",
                                    MentionTab::Articles => "Search articles...",
                                },
                                value: "{search_query}",
                                oninput: move |e| handle_search(e.value()),
                            }

                            if is_searching {
                                div {
                                    class: "absolute right-3 top-1/2 -translate-y-1/2",
                                    svg {
                                        class: "w-4 h-4 animate-spin text-muted-foreground",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        circle {
                                            class: "opacity-25",
                                            cx: "12",
                                            cy: "12",
                                            r: "10",
                                            stroke: "currentColor",
                                            stroke_width: "4",
                                        }
                                        path {
                                            class: "opacity-75",
                                            fill: "currentColor",
                                            d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Results list
                    div {
                        class: "flex-1 overflow-y-auto",

                        match *active_tab.read() {
                            MentionTab::Users => {
                                let results = user_results.read();
                                rsx! {
                                    if results.is_empty() && !search_query.read().is_empty() && !*is_searching_users.read() {
                                        EmptyState { message: "No users found" }
                                    } else if results.is_empty() && search_query.read().is_empty() {
                                        EmptyState { message: "Type to search for users" }
                                    } else {
                                        div {
                                            class: "divide-y divide-border",
                                            for profile in results.iter() {
                                                {
                                                    let profile = profile.clone();
                                                    let profile_for_click = profile.clone();
                                                    rsx! {
                                                        UserResultRow {
                                                            profile: profile,
                                                            onclick: move |_| handle_user_select(profile_for_click.clone()),
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            MentionTab::Notes => {
                                let results = note_results.read();
                                rsx! {
                                    if results.is_empty() && !search_query.read().is_empty() && !*is_searching_notes.read() {
                                        EmptyState { message: "No notes found" }
                                    } else if results.is_empty() && search_query.read().is_empty() {
                                        EmptyState { message: "Type to search for notes" }
                                    } else {
                                        div {
                                            class: "divide-y divide-border",
                                            for result in results.iter() {
                                                {
                                                    let result = result.clone();
                                                    let result_for_click = result.clone();
                                                    rsx! {
                                                        NoteResultRow {
                                                            result: result,
                                                            onclick: move |_| handle_note_select(result_for_click.clone()),
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            MentionTab::Articles => {
                                let results = article_results.read();
                                rsx! {
                                    if results.is_empty() && !search_query.read().is_empty() && !*is_searching_articles.read() {
                                        EmptyState { message: "No articles found" }
                                    } else if results.is_empty() && search_query.read().is_empty() {
                                        EmptyState { message: "Type to search for articles" }
                                    } else {
                                        div {
                                            class: "divide-y divide-border",
                                            for result in results.iter() {
                                                {
                                                    let result = result.clone();
                                                    let result_for_click = result.clone();
                                                    rsx! {
                                                        ArticleResultRow {
                                                            result: result,
                                                            onclick: move |_| handle_article_select(result_for_click.clone()),
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
            }
        }
    }
}

// ============================================================================
// Helper Components
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct TabButtonProps {
    active: bool,
    onclick: EventHandler<MouseEvent>,
    children: Element,
}

#[component]
fn TabButton(props: TabButtonProps) -> Element {
    let class = if props.active {
        "flex-1 py-3 text-sm font-medium border-b-2 border-primary text-primary"
    } else {
        "flex-1 py-3 text-sm font-medium text-muted-foreground hover:text-foreground transition"
    };

    rsx! {
        button {
            class: class,
            onclick: props.onclick,
            {props.children}
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct EmptyStateProps {
    message: &'static str,
}

#[component]
fn EmptyState(props: EmptyStateProps) -> Element {
    rsx! {
        div {
            class: "flex items-center justify-center h-32 text-muted-foreground text-sm",
            {props.message}
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct UserResultRowProps {
    profile: UserSearchResult,
    onclick: EventHandler<MouseEvent>,
}

#[component]
fn UserResultRow(props: UserResultRowProps) -> Element {
    let display_name = props.profile.get_display_name();
    let username = props.profile.get_username();
    let picture = props.profile.picture.clone();
    let npub_short = {
        let hex = props.profile.pubkey.to_hex();
        format!("{}...{}", &hex[..8], &hex[hex.len() - 8..])
    };

    rsx! {
        button {
            class: "w-full flex items-center gap-3 p-3 hover:bg-accent transition text-left",
            onclick: props.onclick,

            // Avatar
            div {
                class: "w-10 h-10 rounded-full bg-muted shrink-0 overflow-hidden",

                if let Some(url) = picture {
                    img {
                        src: "{url}",
                        class: "w-full h-full object-cover",
                        alt: "Profile picture",
                    }
                } else {
                    div {
                        class: "w-full h-full flex items-center justify-center text-muted-foreground",
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path {
                                d: "M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"
                            }
                            circle { cx: "12", cy: "7", r: "4" }
                        }
                    }
                }
            }

            // Name and username
            div {
                class: "flex-1 min-w-0",

                p {
                    class: "font-medium truncate",
                    "{display_name}"
                }

                div {
                    class: "flex items-center gap-2 text-xs text-muted-foreground",

                    if let Some(name) = username {
                        span { "@{name}" }
                    }

                    span {
                        class: "truncate",
                        "{npub_short}"
                    }
                }
            }

            // Contact indicator
            if props.profile.is_contact {
                span {
                    class: "px-2 py-0.5 text-xs rounded-full bg-primary/10 text-primary",
                    "Contact"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct NoteResultRowProps {
    result: ContentSearchResult,
    onclick: EventHandler<MouseEvent>,
}

#[component]
fn NoteResultRow(props: NoteResultRowProps) -> Element {
    // Get author display name from cache
    let author_name = {
        let pubkey_hex = props.result.event.pubkey.to_hex();
        let cache = PROFILE_CACHE.read();
        cache
            .peek(&pubkey_hex)
            .and_then(|p| p.display_name.clone().or(p.name.clone()))
            .unwrap_or_else(|| format!("{}...", &pubkey_hex[..8]))
    };

    // Truncate content
    let content_preview: String = props.result.event.content.chars().take(100).collect();
    let content_preview = if props.result.event.content.len() > 100 {
        format!("{}...", content_preview)
    } else {
        content_preview
    };

    // Format timestamp
    let timestamp = props.result.event.created_at.as_secs();
    let time_ago = format_time_ago(timestamp);

    rsx! {
        button {
            class: "w-full flex flex-col gap-1 p-3 hover:bg-accent transition text-left",
            onclick: props.onclick,

            // Author and time
            div {
                class: "flex items-center gap-2 text-xs text-muted-foreground",

                span { class: "font-medium", "{author_name}" }
                span { "·" }
                span { "{time_ago}" }

                if props.result.is_from_contact {
                    span {
                        class: "ml-auto px-2 py-0.5 rounded-full bg-primary/10 text-primary",
                        "Contact"
                    }
                }
            }

            // Content preview
            p {
                class: "text-sm line-clamp-2",
                "{content_preview}"
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ArticleResultRowProps {
    result: ContentSearchResult,
    onclick: EventHandler<MouseEvent>,
}

#[component]
fn ArticleResultRow(props: ArticleResultRowProps) -> Element {
    // Extract title from tags
    let title = props
        .result
        .event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::Title)
        .and_then(|t| t.content())
        .map(String::from)
        .unwrap_or_else(|| "Untitled".to_string());

    // Extract summary from tags
    let summary = props
        .result
        .event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::Summary)
        .and_then(|t| t.content())
        .map(String::from);

    // Get author display name from cache
    let author_name = {
        let pubkey_hex = props.result.event.pubkey.to_hex();
        let cache = PROFILE_CACHE.read();
        cache
            .peek(&pubkey_hex)
            .and_then(|p| p.display_name.clone().or(p.name.clone()))
            .unwrap_or_else(|| format!("{}...", &pubkey_hex[..8]))
    };

    // Format timestamp
    let timestamp = props.result.event.created_at.as_secs();
    let time_ago = format_time_ago(timestamp);

    // Get cover image from tags
    let cover_image = props
        .result
        .event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::Image)
        .and_then(|t| t.content())
        .map(String::from);

    rsx! {
        button {
            class: "w-full flex gap-3 p-3 hover:bg-accent transition text-left",
            onclick: props.onclick,

            // Cover image thumbnail
            if let Some(url) = cover_image {
                img {
                    src: "{url}",
                    class: "w-16 h-16 rounded object-cover shrink-0",
                    alt: "Cover image",
                }
            }

            // Content
            div {
                class: "flex-1 min-w-0",

                // Title
                p {
                    class: "font-medium truncate",
                    "{title}"
                }

                // Summary or content preview
                if let Some(summary_text) = summary {
                    p {
                        class: "text-sm text-muted-foreground line-clamp-2",
                        "{summary_text}"
                    }
                }

                // Author and time
                div {
                    class: "flex items-center gap-2 text-xs text-muted-foreground mt-1",

                    span { "{author_name}" }
                    span { "·" }
                    span { "{time_ago}" }

                    if props.result.is_from_contact {
                        span {
                            class: "ml-auto px-2 py-0.5 rounded-full bg-primary/10 text-primary",
                            "Contact"
                        }
                    }
                }
            }
        }
    }
}
