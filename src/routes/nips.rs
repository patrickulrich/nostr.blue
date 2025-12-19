use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::components::{OfficialNipCard, CustomNipCard, NipCardSkeleton, ClientInitializing, MarkdownEditor};
use crate::services::github_nips::{self, OfficialNip, EventKindInfo};
use crate::hooks::use_infinite_scroll;
use nostr_sdk::Event;

/// Tab selection for the NIPs page
#[derive(Clone, Copy, PartialEq, Debug)]
enum NipsTab {
    Official,
    Custom,
    EventKinds,
}

impl NipsTab {
    fn label(&self) -> &'static str {
        match self {
            NipsTab::Official => "Official",
            NipsTab::Custom => "Custom",
            NipsTab::EventKinds => "Event Kinds",
        }
    }
}

/// NIPs home page - displays official and custom NIPs
#[component]
pub fn NipsHome() -> Element {
    // Tab and search state
    let mut active_tab = use_signal(|| NipsTab::Official);
    let mut search_query = use_signal(String::new);
    let mut search_input = use_signal(String::new); // Debounced input

    // Search results state (overlay)
    let mut search_results = use_signal(Vec::<Event>::new);
    let mut search_loading = use_signal(|| false);
    let mut is_searching = use_signal(|| false); // True when search overlay is active

    // Data state
    let mut official_nips = use_signal(Vec::<OfficialNip>::new);
    let mut custom_nips = use_signal(Vec::<Event>::new);
    let mut event_kinds = use_signal(Vec::<EventKindInfo>::new);

    // Loading and pagination state
    let mut loading = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut error = use_signal(|| None::<String>);

    // Load data when tab changes
    use_effect(move || {
        let tab = *active_tab.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Reset pagination when tab changes
        oldest_timestamp.set(None);
        has_more.set(true);
        error.set(None);

        spawn(async move {
            loading.set(true);

            match tab {
                NipsTab::Official | NipsTab::EventKinds => {
                    // Fetch README from GitHub
                    match github_nips::fetch_nips_readme().await {
                        Ok(readme) => {
                            let nips = github_nips::parse_nips_from_readme(&readme);
                            let kinds = github_nips::parse_event_kinds_from_readme(&readme);
                            official_nips.set(nips);
                            event_kinds.set(kinds);
                        }
                        Err(e) => {
                            log::error!("Failed to fetch NIPs README: {}", e);
                            error.set(Some(e));
                        }
                    }
                }
                NipsTab::Custom => {
                    // Only fetch if client is initialized
                    if client_initialized {
                        match nostr_client::fetch_custom_nips(50, None).await {
                            Ok(events) => {
                                if let Some(last) = events.last() {
                                    oldest_timestamp.set(Some(last.created_at.as_secs()));
                                }
                                has_more.set(events.len() >= 50);
                                custom_nips.set(events);
                            }
                            Err(e) => {
                                log::error!("Failed to fetch custom NIPs: {}", e);
                                error.set(Some(e));
                            }
                        }
                    }
                }
            }

            loading.set(false);
        });
    });

    // Load more function for custom NIPs infinite scroll
    let load_more = move || {
        if *loading.read() || !*has_more.read() || *active_tab.read() != NipsTab::Custom {
            return;
        }

        let until = *oldest_timestamp.read();
        loading.set(true);

        spawn(async move {
            match nostr_client::fetch_custom_nips(50, until).await {
                Ok(mut new_events) => {
                    if let Some(last) = new_events.last() {
                        oldest_timestamp.set(Some(last.created_at.as_secs()));
                    }
                    has_more.set(new_events.len() >= 50);

                    let mut current = custom_nips.read().clone();
                    current.append(&mut new_events);
                    custom_nips.set(current);
                }
                Err(e) => {
                    log::error!("Failed to load more custom NIPs: {}", e);
                }
            }
            loading.set(false);
        });
    };

    // Set up infinite scroll for custom NIPs
    let sentinel_id = use_infinite_scroll(load_more, has_more, loading);

    // Search effect - queries relays for custom NIPs when user types
    use_effect(move || {
        let query = search_input.read().clone();

        // Clear search state if empty
        if query.trim().is_empty() {
            is_searching.set(false);
            search_results.set(Vec::new());
            return;
        }

        // Debounce: only search if query is at least 2 chars
        if query.len() < 2 {
            return;
        }

        is_searching.set(true);
        search_loading.set(true);

        spawn(async move {
            match nostr_client::search_custom_nips(&query, 50).await {
                Ok(events) => {
                    search_results.set(events);
                }
                Err(e) => {
                    log::error!("Failed to search custom NIPs: {}", e);
                    search_results.set(Vec::new());
                }
            }
            search_loading.set(false);
        });
    });

    // Filter official NIPs for search overlay
    let search_official_results = use_memo(move || {
        let query = search_input.read().to_lowercase();
        if query.len() < 2 {
            return Vec::new();
        }
        official_nips.read()
            .iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&query)
                    || n.number.to_lowercase().contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>()
    });

    // Filter official NIPs based on search
    let filtered_official = use_memo(move || {
        let query = search_query.read().to_lowercase();
        if query.is_empty() {
            return official_nips.read().clone();
        }
        official_nips.read()
            .iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&query)
                    || n.number.to_lowercase().contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>()
    });

    // Filter event kinds based on search
    let filtered_kinds = use_memo(move || {
        let query = search_query.read().to_lowercase();
        if query.is_empty() {
            return event_kinds.read().clone();
        }
        event_kinds.read()
            .iter()
            .filter(|k| {
                k.kind.to_lowercase().contains(&query)
                    || k.description.to_lowercase().contains(&query)
                    || k.nip.to_lowercase().contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>()
    });

    // Filter custom NIPs based on search
    let filtered_custom = use_memo(move || {
        let query = search_query.read().to_lowercase();
        if query.is_empty() {
            return custom_nips.read().clone();
        }
        custom_nips.read()
            .iter()
            .filter(|event| {
                // Search in title tag
                let title_match = event.tags.iter()
                    .find(|t| t.kind() == nostr_sdk::TagKind::Title)
                    .and_then(|t| t.content())
                    .map(|s| s.to_lowercase().contains(&query))
                    .unwrap_or(false);

                // Search in d-tag (identifier)
                let id_match = event.tags.identifier()
                    .map(|s| s.to_lowercase().contains(&query))
                    .unwrap_or(false);

                // Search in content
                let content_match = event.content.to_lowercase().contains(&query);

                title_match || id_match || content_match
            })
            .cloned()
            .collect::<Vec<_>>()
    });

    let is_loading = *loading.read();
    let error_msg = error.read();
    let current_tab = *active_tab.read();

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",

                // Title and tabs
                div {
                    class: "px-4 py-3",

                    // Title
                    div {
                        class: "flex items-center justify-between mb-4",
                        h1 {
                            class: "text-2xl font-bold",
                            "NIPs"
                        }

                        // Create button (only for authenticated users)
                        if auth_store::AUTH_STATE.read().is_authenticated {
                            Link {
                                to: crate::routes::Route::NipNew {},
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm font-medium",
                                "+ Create NIP"
                            }
                        }
                    }

                    // Tab navigation
                    div {
                        class: "flex gap-1 mb-4",

                        for tab in [NipsTab::Official, NipsTab::Custom, NipsTab::EventKinds] {
                            button {
                                class: if current_tab == tab {
                                    "px-4 py-2 rounded-lg bg-primary text-primary-foreground font-medium transition"
                                } else {
                                    "px-4 py-2 rounded-lg hover:bg-accent text-muted-foreground hover:text-foreground transition"
                                },
                                onclick: move |_| active_tab.set(tab),
                                "{tab.label()}"
                            }
                        }
                    }

                    // Search bar
                    div {
                        class: "relative",
                        input {
                            r#type: "text",
                            placeholder: match current_tab {
                                NipsTab::Official => "Search NIPs by title or number...",
                                NipsTab::Custom => "Search custom NIPs (searches relays)...",
                                NipsTab::EventKinds => "Search event kinds...",
                            },
                            class: "w-full px-4 py-2 pl-10 pr-10 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                            value: "{search_query}",
                            oninput: move |e| {
                                let val = e.value();
                                search_query.set(val.clone());
                                // Always trigger relay search for custom NIPs
                                search_input.set(val);
                            },
                        }
                        svg {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            circle { cx: "11", cy: "11", r: "8" }
                            path { d: "m21 21-4.3-4.3" }
                        }
                        // Clear button
                        if !search_query.read().is_empty() {
                            button {
                                class: "absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground hover:text-foreground transition",
                                onclick: move |_| {
                                    search_query.set(String::new());
                                    search_input.set(String::new());
                                },
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    class: "w-4 h-4",
                                    path { d: "M6 18L18 6M6 6l12 12" }
                                }
                            }
                        }
                    }
                }
            }

            // Error message
            if let Some(err) = error_msg.as_ref() {
                div {
                    class: "p-4 bg-destructive/10 border border-destructive text-destructive mx-4 mt-4 rounded-lg",
                    p { "Error: {err}" }
                }
            }

            // Search results overlay - shows when actively searching
            if *is_searching.read() {
                div {
                    class: "p-4",

                    // Search results header
                    div {
                        class: "flex items-center justify-between mb-4",
                        h2 {
                            class: "text-lg font-semibold",
                            "Search Results"
                        }
                        if *search_loading.read() {
                            span {
                                class: "text-sm text-muted-foreground animate-pulse",
                                "Searching relays..."
                            }
                        }
                    }

                    // Official NIPs section
                    if !search_official_results().is_empty() {
                        div {
                            class: "mb-6",
                            h3 {
                                class: "text-sm font-medium text-muted-foreground mb-3",
                                "Official NIPs ({search_official_results().len()})"
                            }
                            div {
                                class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4",
                                for nip in search_official_results().iter() {
                                    OfficialNipCard {
                                        key: "{nip.number}",
                                        nip: nip.clone(),
                                    }
                                }
                            }
                        }
                    }

                    // Custom NIPs section
                    div {
                        h3 {
                            class: "text-sm font-medium text-muted-foreground mb-3",
                            if *search_loading.read() {
                                "Custom NIPs (searching...)"
                            } else {
                                "Custom NIPs ({search_results.read().len()})"
                            }
                        }

                        if *search_loading.read() && search_results.read().is_empty() {
                            div {
                                class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                for _ in 0..6 {
                                    NipCardSkeleton {}
                                }
                            }
                        } else if search_results.read().is_empty() && search_official_results().is_empty() {
                            EmptyState {
                                icon: "🔍",
                                title: "No Results Found",
                                description: "No NIPs match your search query.",
                            }
                        } else if !search_results.read().is_empty() {
                            div {
                                class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                for event in search_results.read().iter() {
                                    CustomNipCard {
                                        key: "{event.id}",
                                        event: event.clone(),
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // Regular content (when not searching)
                div {
                    class: "p-4",

                    // Loading state
                    if is_loading && (
                        (current_tab == NipsTab::Official && official_nips.read().is_empty()) ||
                        (current_tab == NipsTab::Custom && custom_nips.read().is_empty()) ||
                        (current_tab == NipsTab::EventKinds && event_kinds.read().is_empty())
                    ) {
                        ClientInitializing {}
                    } else {
                    match current_tab {
                        NipsTab::Official => rsx! {
                            // Official NIPs grid
                            if filtered_official().is_empty() {
                                EmptyState {
                                    icon: "📜",
                                    title: "No NIPs Found",
                                    description: "No official NIPs match your search.",
                                }
                            } else {
                                div {
                                    class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4",
                                    for nip in filtered_official().iter() {
                                        OfficialNipCard {
                                            key: "{nip.number}",
                                            nip: nip.clone(),
                                        }
                                    }
                                }
                            }
                        },

                        NipsTab::Custom => rsx! {
                            // Custom NIPs grid
                            if !*nostr_client::CLIENT_INITIALIZED.read() {
                                ClientInitializing {}
                            } else if custom_nips.read().is_empty() {
                                EmptyState {
                                    icon: "💡",
                                    title: "No Custom NIPs Yet",
                                    description: "Be the first to propose a new NIP!",
                                }
                            } else if filtered_custom().is_empty() {
                                EmptyState {
                                    icon: "🔍",
                                    title: "No Matches Found",
                                    description: "No custom NIPs match your search.",
                                }
                            } else {
                                div {
                                    class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                    for event in filtered_custom().iter() {
                                        CustomNipCard {
                                            key: "{event.id}",
                                            event: event.clone(),
                                        }
                                    }
                                }

                                // Infinite scroll sentinel (only show if not searching)
                                if *has_more.read() && search_query.read().is_empty() {
                                    div {
                                        id: "{sentinel_id}",
                                        class: "h-20 flex items-center justify-center",
                                        if is_loading {
                                            div {
                                                class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 w-full",
                                                for _ in 0..3 {
                                                    NipCardSkeleton {}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },

                        NipsTab::EventKinds => rsx! {
                            // Event kinds table
                            if filtered_kinds().is_empty() {
                                EmptyState {
                                    icon: "🔢",
                                    title: "No Event Kinds Found",
                                    description: "No event kinds match your search.",
                                }
                            } else {
                                div {
                                    class: "overflow-x-auto",
                                    table {
                                        class: "w-full border-collapse",
                                        thead {
                                            tr {
                                                class: "border-b border-border",
                                                th { class: "text-left p-3 font-semibold text-muted-foreground", "Kind" }
                                                th { class: "text-left p-3 font-semibold text-muted-foreground", "Description" }
                                                th { class: "text-left p-3 font-semibold text-muted-foreground", "NIP" }
                                            }
                                        }
                                        tbody {
                                            for kind in filtered_kinds().iter() {
                                                tr {
                                                    class: "border-b border-border hover:bg-muted/50 transition",
                                                    td {
                                                        class: "p-3 font-mono text-primary",
                                                        "{kind.kind}"
                                                    }
                                                    td {
                                                        class: "p-3",
                                                        "{kind.description}"
                                                    }
                                                    td {
                                                        class: "p-3",
                                                        if !kind.nip.is_empty() {
                                                            Link {
                                                                to: crate::routes::Route::NipDetail { nip_id: kind.nip.clone() },
                                                                class: "text-primary hover:underline",
                                                                "NIP-{kind.nip}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }
                    }
                }
            }
        }
    }
}

/// Empty state component
#[component]
fn EmptyState(icon: &'static str, title: &'static str, description: &'static str) -> Element {
    rsx! {
        div {
            class: "text-center py-12",
            div {
                class: "text-6xl mb-4",
                "{icon}"
            }
            h3 {
                class: "text-xl font-semibold mb-2",
                "{title}"
            }
            p {
                class: "text-muted-foreground text-sm",
                "{description}"
            }
        }
    }
}

/// Create a new custom NIP page
#[component]
pub fn NipNew() -> Element {
    let navigator = navigator();
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

    // Redirect if not authenticated
    use_effect(move || {
        if !*is_authenticated.read() {
            navigator.push(crate::routes::Route::NipsHome {});
        }
    });

    // Form state
    let mut title = use_signal(String::new);
    let mut identifier = use_signal(String::new);
    let content = use_signal(String::new);
    let mut related_kinds_input = use_signal(String::new);
    let mut is_publishing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    // Auto-generate identifier from title
    let auto_generate_id = move |_| {
        let t = title.read().clone();
        let slug = t
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        identifier.set(slug);
    };

    // Parse related kinds from comma-separated input
    let parse_related_kinds = move || -> Vec<u32> {
        related_kinds_input.read()
            .split(',')
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .collect()
    };

    // Validation
    let can_publish = use_memo(move || {
        !title.read().is_empty()
            && !identifier.read().is_empty()
            && !content.read().is_empty()
            && !*is_publishing.read()
    });

    let handle_publish = move |_| {
        if !*can_publish.read() {
            return;
        }

        let title_val = title.read().clone();
        let identifier_val = identifier.read().clone();
        let content_val = content.read().clone();
        let related_kinds = parse_related_kinds();

        is_publishing.set(true);
        error.set(None);

        spawn(async move {
            match nostr_client::publish_custom_nip(
                title_val,
                content_val,
                identifier_val.clone(),
                related_kinds,
            ).await {
                Ok(_event_id) => {
                    is_publishing.set(false);
                    // Generate naddr and navigate to detail page
                    if let Some(pubkey_str) = auth_store::get_pubkey() {
                        if let Ok(pubkey) = nostr_sdk::PublicKey::from_hex(&pubkey_str) {
                            if let Ok(naddr) = nostr_client::generate_custom_nip_naddr(
                                &pubkey,
                                &identifier_val,
                                vec![],
                            ) {
                                navigator.push(crate::routes::Route::NipDetail { nip_id: naddr });
                                return;
                            }
                        }
                    }
                    // Fallback: navigate to NIPs home
                    navigator.push(crate::routes::Route::NipsHome {});
                }
                Err(e) => {
                    log::error!("Failed to publish custom NIP: {}", e);
                    error.set(Some(e));
                    is_publishing.set(false);
                }
            }
        });
    };

    if !*is_authenticated.read() {
        return rsx! {
            div {
                class: "p-8 text-center",
                p { class: "text-muted-foreground", "Please sign in to create a NIP." }
            }
        };
    }

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center gap-4",

                    // Back button
                    Link {
                        to: crate::routes::Route::NipsHome {},
                        class: "p-2 rounded-lg hover:bg-accent transition",
                        "← Back"
                    }

                    h1 {
                        class: "text-xl font-bold",
                        "Create Custom NIP"
                    }
                }
            }

            // Form
            div {
                class: "max-w-4xl mx-auto p-4 space-y-6",

                // Error message
                if let Some(err) = error.read().as_ref() {
                    div {
                        class: "p-4 bg-destructive/10 border border-destructive text-destructive rounded-lg",
                        p { "{err}" }
                    }
                }

                // Title input
                div {
                    class: "space-y-2",
                    label {
                        class: "block text-sm font-medium",
                        "Title *"
                    }
                    input {
                        r#type: "text",
                        placeholder: "e.g., Gaming Events Protocol",
                        class: "w-full px-4 py-2 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                        value: "{title}",
                        oninput: move |e| title.set(e.value()),
                        onblur: auto_generate_id,
                    }
                }

                // Identifier input
                div {
                    class: "space-y-2",
                    label {
                        class: "block text-sm font-medium",
                        "Identifier *"
                    }
                    input {
                        r#type: "text",
                        placeholder: "e.g., gaming-events",
                        class: "w-full px-4 py-2 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition font-mono",
                        value: "{identifier}",
                        oninput: move |e| identifier.set(e.value()),
                    }
                    p {
                        class: "text-xs text-muted-foreground",
                        "Unique identifier for this NIP. Auto-generated from title."
                    }
                }

                // Related kinds input
                div {
                    class: "space-y-2",
                    label {
                        class: "block text-sm font-medium",
                        "Related Event Kinds (optional)"
                    }
                    input {
                        r#type: "text",
                        placeholder: "e.g., 30100, 30101, 30102",
                        class: "w-full px-4 py-2 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition font-mono",
                        value: "{related_kinds_input}",
                        oninput: move |e| related_kinds_input.set(e.value()),
                    }
                    p {
                        class: "text-xs text-muted-foreground",
                        "Comma-separated list of event kinds this NIP defines."
                    }
                }

                // Content editor
                div {
                    class: "space-y-2",
                    label {
                        class: "block text-sm font-medium",
                        "Content (Markdown) *"
                    }
                    MarkdownEditor {
                        content: content,
                        placeholder: "Write your NIP specification in Markdown...",
                        min_height: 400,
                    }
                }

                // Publish button
                div {
                    class: "flex justify-end gap-4",
                    Link {
                        to: crate::routes::Route::NipsHome {},
                        class: "px-6 py-2 rounded-lg border border-border hover:bg-accent transition",
                        "Cancel"
                    }
                    button {
                        class: "px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: !*can_publish.read(),
                        onclick: handle_publish,
                        if *is_publishing.read() {
                            "Publishing..."
                        } else {
                            "Publish NIP"
                        }
                    }
                }
            }
        }
    }
}
