use crate::components::{
    ClientInitializing, CustomNipCard, MarkdownEditor, NipCardSkeleton, SupportedSpecCard,
};
use crate::hooks::use_infinite_scroll;
use crate::routes::nips::registry::{self, SpecType};
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
use nostr_sdk::Event;
use std::collections::HashSet;

/// Tab selection for the Protocol Docs page.
#[derive(Clone, Copy, PartialEq, Debug)]
enum DocsTab {
    OurNips,
    Custom,
}
impl DocsTab {
    fn label(&self) -> &'static str {
        match self {
            DocsTab::OurNips => "Our NIPs",
            DocsTab::Custom => "Custom",
        }
    }
}
const ALL_TABS: [DocsTab; 2] = [DocsTab::OurNips, DocsTab::Custom];

/// Filter chip for the "Our NIPs" grid.
#[derive(Clone, Copy, PartialEq, Debug)]
enum SpecFilter {
    All,
    ByType(SpecType),
}
impl SpecFilter {
    fn label(&self) -> &'static str {
        match self {
            SpecFilter::All => "All",
            SpecFilter::ByType(t) => t.label_plural(),
        }
    }
}
const FILTER_CHIPS: &[SpecFilter] = &[
    SpecFilter::All,
    SpecFilter::ByType(SpecType::Nip),
    SpecFilter::ByType(SpecType::Nut),
    SpecFilter::ByType(SpecType::Bud),
    SpecFilter::ByType(SpecType::Nkbip),
    SpecFilter::ByType(SpecType::Market),
];

/// Protocol Docs home page - displays nostr.blue's supported specs and custom NIPs.
#[component]
pub fn NipsHome() -> Element {
    let mut active_tab = use_signal(|| DocsTab::OurNips);
    let mut active_filter = use_signal(|| SpecFilter::All);
    let mut search_query = use_signal(String::new);
    let mut search_input = use_signal(String::new);
    let mut search_results = use_signal(Vec::<Event>::new);
    let mut search_loading = use_signal(|| false);
    let mut is_searching = use_signal(|| false);
    let mut custom_nips = use_signal(Vec::<Event>::new);
    let mut loading = use_signal(|| false);
    let mut loading_more = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut error = use_signal(|| None::<String>);
    let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

    // Custom-tab data loading (relay fetch).
    use_effect(move || {
        let tab = *active_tab.read();
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);
        loading_more.set(false);
        if tab != DocsTab::Custom {
            return;
        }
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            match nostr_client::fetch_custom_nips(50, None).await {
                Ok(events) => {
                    let oldest = events
                        .iter()
                        .map(|e| e.created_at.as_secs())
                        .min()
                        .map(|t| t.saturating_sub(1));
                    if let Some(ts) = oldest {
                        oldest_timestamp.set(Some(ts));
                    }
                    has_more.set(events.len() >= 50);
                    custom_nips.set(events);
                }
                Err(e) => {
                    log::error!("Failed to fetch custom NIPs: {}", e);
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    let load_more = move || {
        if *loading_more.read() || !*has_more.read() || *active_tab.read() != DocsTab::Custom {
            return;
        }
        let until = *oldest_timestamp.read();
        loading_more.set(true);
        spawn(async move {
            match nostr_client::fetch_custom_nips(50, until).await {
                Ok(new_events) => {
                    let raw_count = new_events.len();
                    let oldest = new_events
                        .iter()
                        .map(|e| e.created_at.as_secs())
                        .min()
                        .map(|t| t.saturating_sub(1));
                    if let Some(ts) = oldest {
                        oldest_timestamp.set(Some(ts));
                    }
                    let existing_ids: HashSet<nostr_sdk::EventId> =
                        custom_nips.read().iter().map(|e| e.id).collect();
                    let unique_new: Vec<Event> = new_events
                        .into_iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .collect();
                    has_more.set(raw_count >= 50 && !unique_new.is_empty());
                    if !unique_new.is_empty() {
                        let mut current = custom_nips.read().clone();
                        current.extend(unique_new);
                        custom_nips.set(current);
                    }
                }
                Err(e) => {
                    log::error!("Failed to load more custom NIPs: {}", e);
                }
            }
            loading_more.set(false);
        });
    };
    let sentinel_id = use_infinite_scroll(load_more, has_more, loading_more);

    // Custom-tab relay search.
    use_effect(move || {
        let query = search_input.read().clone();
        if *active_tab.read() != DocsTab::Custom {
            is_searching.set(false);
            return;
        }
        if query.trim().is_empty() {
            is_searching.set(false);
            search_results.set(Vec::new());
            return;
        }
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

    // Our NIPs: filter the registry by active chip + search query.
    let filtered_specs = use_memo(move || {
        let filter = *active_filter.read();
        let query = search_query.read().to_lowercase();
        registry::all()
            .iter()
            .copied()
            .filter(|s| match filter {
                SpecFilter::All => true,
                SpecFilter::ByType(t) => s.spec_type == t,
            })
            .filter(|s| {
                if query.is_empty() {
                    return true;
                }
                s.title.to_lowercase().contains(&query)
                    || s.number.to_lowercase().contains(&query)
                    || s.spec_type.label().to_lowercase().contains(&query)
                    || s.badge().to_lowercase().contains(&query)
            })
            .collect::<Vec<_>>()
    });

    let filtered_custom = use_memo(move || {
        let query = search_query.read().to_lowercase();
        if query.is_empty() {
            return custom_nips.read().clone();
        }
        custom_nips
            .read()
            .iter()
            .filter(|event| {
                let title_match = event
                    .tags
                    .iter()
                    .find(|t| t.kind() == nostr_sdk::TagKind::Title)
                    .and_then(|t| t.content())
                    .map(|s| s.to_lowercase().contains(&query))
                    .unwrap_or(false);
                let id_match = event
                    .tags
                    .identifier()
                    .map(|s| s.to_lowercase().contains(&query))
                    .unwrap_or(false);
                let content_match = event.content.to_lowercase().contains(&query);
                title_match || id_match || content_match
            })
            .cloned()
            .collect::<Vec<_>>()
    });

    let is_loading = *loading.read();
    let error_msg = error.read();
    let current_tab = *active_tab.read();
    let current_filter = *active_filter.read();

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3",
                    div { class: "flex items-center justify-between mb-4",
                        h1 { class: "text-2xl font-bold", "Protocol Docs" }
                        if auth_store::AUTH_STATE.read().is_authenticated {
                            Link {
                                to: crate::routes::Route::NipNew {},
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm font-medium",
                                "+ Create NIP"
                            }
                        }
                    }
                    div { class: "flex gap-1 mb-4 overflow-x-auto scrollbar-none",
                        for tab in ALL_TABS {
                            button {
                                class: if current_tab == tab {
                                    "px-4 py-2 rounded-lg bg-primary text-primary-foreground font-medium transition whitespace-nowrap"
                                } else {
                                    "px-4 py-2 rounded-lg hover:bg-accent text-muted-foreground hover:text-foreground transition whitespace-nowrap"
                                },
                                onclick: move |_| {
                                    active_tab.set(tab);
                                    search_query.set(String::new());
                                    search_input.set(String::new());
                                },
                                "{tab.label()}"
                            }
                        }
                    }
                    div { class: "relative",
                        input {
                            r#type: "text",
                            placeholder: match current_tab {
                                DocsTab::OurNips => "Search supported specs by title or number...",
                                DocsTab::Custom => "Search custom NIPs (searches relays)...",
                            },
                            class: "w-full px-4 py-2 pl-10 pr-10 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                            value: "{search_query}",
                            oninput: move |e| {
                                let val = e.value();
                                search_query.set(val.clone());
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

            if let Some(err) = error_msg.as_ref() {
                div { class: "p-4 bg-destructive/10 border border-destructive text-destructive mx-4 mt-4 rounded-lg",
                    p { "Error: {err}" }
                }
            }

            // Custom-tab relay search results.
            if *is_searching.read() && current_tab == DocsTab::Custom {
                div { class: "p-4",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-lg font-semibold", "Search Results" }
                        if *search_loading.read() {
                            span { class: "text-sm text-muted-foreground animate-pulse",
                                "Searching relays..."
                            }
                        }
                    }
                    if *search_loading.read() && search_results.read().is_empty() {
                        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                            for _ in 0..6 {
                                NipCardSkeleton {}
                            }
                        }
                    } else if search_results.read().is_empty() {
                        EmptyState {
                            icon: "🔍",
                            title: "No Results Found",
                            description: "No custom NIPs match your search query.",
                        }
                    } else {
                        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                            for event in search_results.read().iter() {
                                CustomNipCard {
                                    key: "{event.id}",
                                    event: event.clone(),
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "p-4",
                    match current_tab {
                        DocsTab::OurNips => rsx! {
                            if filtered_specs().is_empty() {
                                EmptyState {
                                    icon: "📜",
                                    title: "No Specs Found",
                                    description: "No supported specs match your filter.",
                                }
                            } else {
                                div { class: "mb-4",
                                    div { class: "flex flex-wrap gap-2",
                                        for (idx, chip) in FILTER_CHIPS.iter().enumerate() {
                                            button {
                                                key: "{idx}",
                                                class: if current_filter == *chip {
                                                    "px-3 py-1.5 rounded-full bg-primary text-primary-foreground text-sm font-medium transition"
                                                } else {
                                                    "px-3 py-1.5 rounded-full bg-muted text-muted-foreground hover:text-foreground hover:bg-accent text-sm transition"
                                                },
                                                onclick: move |_| active_filter.set(*chip),
                                                "{chip.label()}"
                                            }
                                        }
                                    }
                                }
                                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4",
                                    for (idx, spec) in filtered_specs().iter().enumerate() {
                                        SupportedSpecCard {
                                            key: "{idx}",
                                            spec: *spec,
                                        }
                                    }
                                }
                            }
                        },
                        DocsTab::Custom => rsx! {
                            if !client_initialized {
                                ClientInitializing {}
                            } else if is_loading && custom_nips.read().is_empty() {
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
                                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                    for event in filtered_custom().iter() {
                                        CustomNipCard { key: "{event.id}", event: event.clone() }
                                    }
                                }
                                if search_query.read().is_empty() {
                                    div {
                                        id: "{sentinel_id}",
                                        class: "h-20 flex items-center justify-center",
                                        if *loading_more.read() {
                                            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 w-full",
                                                for _ in 0..3 {
                                                    NipCardSkeleton {}
                                                }
                                            }
                                        } else if !*has_more.read() {
                                            p { class: "text-muted-foreground text-sm",
                                                "No more custom NIPs to load"
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

/// Empty state component.
#[component]
fn EmptyState(icon: &'static str, title: &'static str, description: &'static str) -> Element {
    rsx! {
        div { class: "text-center py-12",
            div { class: "text-6xl mb-4", "{icon}" }
            h3 { class: "text-xl font-semibold mb-2", "{title}" }
            p { class: "text-muted-foreground text-sm", "{description}" }
        }
    }
}

/// Create a new custom NIP page.
#[component]
pub fn NipNew() -> Element {
    let navigator = navigator();
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);
    use_effect(move || {
        if !*is_authenticated.read() {
            navigator.push(crate::routes::Route::NipsHome {});
        }
    });
    let mut title = use_signal(String::new);
    let mut identifier = use_signal(String::new);
    let content = use_signal(String::new);
    let mut related_kinds_input = use_signal(String::new);
    let mut is_publishing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
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
    let parse_related_kinds = move || -> Vec<u32> {
        related_kinds_input
            .read()
            .split(',')
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .collect()
    };
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
            )
            .await
            {
                Ok(_event_id) => {
                    is_publishing.set(false);
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
            div { class: "p-8 text-center",
                p { class: "text-muted-foreground", "Please sign in to create a NIP." }
            }
        };
    }
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: crate::routes::Route::NipsHome {},
                        class: "p-2 rounded-lg hover:bg-accent transition",
                        "← Back"
                    }
                    h1 { class: "text-xl font-bold", "Create Custom NIP" }
                }
            }
            div { class: "max-w-4xl mx-auto p-4 space-y-6",
                if let Some(err) = error.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive text-destructive rounded-lg",
                        p { "{err}" }
                    }
                }
                div { class: "space-y-2",
                    label { class: "block text-sm font-medium", "Title *" }
                    input {
                        r#type: "text",
                        placeholder: "e.g., Gaming Events Protocol",
                        class: "w-full px-4 py-2 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                        value: "{title}",
                        oninput: move |e| title.set(e.value()),
                        onblur: auto_generate_id,
                    }
                }
                div { class: "space-y-2",
                    label { class: "block text-sm font-medium", "Identifier *" }
                    input {
                        r#type: "text",
                        placeholder: "e.g., gaming-events",
                        class: "w-full px-4 py-2 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition font-mono",
                        value: "{identifier}",
                        oninput: move |e| identifier.set(e.value()),
                    }
                    p { class: "text-xs text-muted-foreground",
                        "Unique identifier for this NIP. Auto-generated from title."
                    }
                }
                div { class: "space-y-2",
                    label { class: "block text-sm font-medium", "Related Event Kinds (optional)" }
                    input {
                        r#type: "text",
                        placeholder: "e.g., 30100, 30101, 30102",
                        class: "w-full px-4 py-2 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition font-mono",
                        value: "{related_kinds_input}",
                        oninput: move |e| related_kinds_input.set(e.value()),
                    }
                    p { class: "text-xs text-muted-foreground",
                        "Comma-separated list of event kinds this NIP defines."
                    }
                }
                div { class: "space-y-2",
                    label { class: "block text-sm font-medium", "Content (Markdown) *" }
                    MarkdownEditor {
                        content,
                        placeholder: "Write your NIP specification in Markdown...",
                        min_height: 400,
                    }
                }
                div { class: "flex justify-end gap-4",
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
