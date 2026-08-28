use crate::components::{NoteCard, NoteCardSkeleton, PhotoCard, VideoCard};
use crate::hooks::{use_mute_block_cache, use_stale_guard};
use crate::services::content_search::{
    get_contact_pubkeys, search_content_streaming, ContentSearchResult,
    ERR_SEARCH_RELAYS_UNREACHABLE,
};
use crate::services::profile_search::{search_profiles, ProfileSearchResult};
use crate::services::search::query_parser;
use crate::stores::nostr_client;
use crate::stores::ui::{search_history, search_results_cache};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum SearchTab {
    TextNotes,
    Articles,
    Photos,
    Videos,
    People,
}
impl SearchTab {
    fn label(&self) -> &'static str {
        match self {
            SearchTab::TextNotes => "Posts",
            SearchTab::Articles => "Articles",
            SearchTab::Photos => "Photos",
            SearchTab::Videos => "Videos",
            SearchTab::People => "People",
        }
    }
    fn cache_key(&self) -> &'static str {
        match self {
            SearchTab::TextNotes => "posts",
            SearchTab::Articles => "articles",
            SearchTab::Photos => "photos",
            SearchTab::Videos => "videos",
            SearchTab::People => "people",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum SortOrder {
    Newest,
    Oldest,
    FollowingFirst,
    Relevance,
}
impl SortOrder {
    fn label(&self) -> &'static str {
        match self {
            SortOrder::Newest => "Newest",
            SortOrder::Oldest => "Oldest",
            SortOrder::FollowingFirst => "Following first",
            SortOrder::Relevance => "Relevance",
        }
    }
}

/// Result page size; "Load more" raises the limit in these steps.
const BASE_LIMIT: usize = 50;
const MAX_LIMIT: usize = 150;

#[component]
pub fn Search(q: String) -> Element {
    let mut active_tab = use_signal(|| SearchTab::TextNotes);
    let mut results = use_signal(Vec::<ContentSearchResult>::new);
    let mut profile_results = use_signal(Vec::<ProfileSearchResult>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut search_relays_unreachable = use_signal(|| false);
    let mut contact_pubkeys = use_signal(Vec::<PublicKey>::new);
    let mut query = use_signal(|| q.clone());
    let mut current_limit = use_signal(|| BASE_LIMIT);
    let mut sort_order = use_signal(|| SortOrder::FollowingFirst);
    let mut show_sort_dropdown = use_signal(|| false);
    let mut search_task: Signal<Option<dioxus_core::Task>> = use_signal(|| None);
    let mut stale_guard = use_stale_guard();
    let (cached_muted_posts, cached_blocked_users, cached_muted_words) = use_mute_block_cache();

    let detected_type = use_memo(move || query_parser::detect_search_type(&query.read()));
    let unknown_authors = use_memo(move || {
        if let query_parser::SearchType::FullText(parsed) = &*detected_type.read() {
            let (_, unresolved) =
                crate::services::content_search::resolve_author_names(&parsed.author_names);
            unresolved
        } else {
            Vec::new()
        }
    });

    use_effect(use_reactive!(|q| {
        query.set(q);
        current_limit.set(BASE_LIMIT);
    }));

    // NIP-19 / hex lookups typed into the search box go straight to their
    // viewers instead of full-text-searching the bech32 string.
    {
        let query_for_redirect = query;
        let mut last_redirected = use_signal(String::new);
        use_effect(move || {
            let q = query_for_redirect.read().clone();
            if q.is_empty() || *last_redirected.read() == q {
                return;
            }
            let navigator = navigator();
            match query_parser::detect_search_type(&q) {
                query_parser::SearchType::ProfileLookup { pubkey, .. } => {
                    last_redirected.set(q.clone());
                    navigator.push(crate::routes::Route::AddressViewer {
                        address: crate::utils::nip19_urls::profile_route_id(&pubkey.to_hex()),
                    });
                }
                query_parser::SearchType::NoteLookup { event_id, author, .. } => {
                    last_redirected.set(q.clone());
                    navigator.push(crate::routes::Route::Note {
                        note_id: crate::utils::nip19_urls::note_route_id(
                            &event_id.to_hex(),
                            author.map(|pk| pk.to_hex()).as_deref(),
                        ),
                        from_voice: None,
                    });
                }
                query_parser::SearchType::AddressLookup { .. } => {
                    // The query itself is the naddr — the handler decodes and
                    // routes it (keeping any embedded relay hints).
                    last_redirected.set(q.clone());
                    navigator.push(crate::routes::Route::Nip19Handler { identifier: q });
                }
                _ => {}
            }
        });
    }

    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            let contacts = get_contact_pubkeys().await;
            contact_pubkeys.set(contacts);
        });
    });

    // Search effect: cache-first render, then streaming refresh.
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let q = query.read().clone();
        let tab = *active_tab.read();
        let limit = *current_limit.read();
        let contacts = contact_pubkeys.read().clone();
        if !client_initialized || q.is_empty() {
            return;
        }

        // Instant cache render (tab switches don't refetch-block).
        if let Some(cached) = search_results_cache::get_cached(&q, tab.cache_key()) {
            results.set(cached.content);
            profile_results.set(cached.profiles);
            loading.set(false);
        }

        // Cancel any in-flight search and invalidate its result tokens.
        if let Some(task) = search_task.write().take() {
            task.cancel();
        }
        let token = stale_guard.bump();

        loading.set(true);
        error.set(None);
        search_relays_unreachable.set(false);

        if tab == SearchTab::People {
            let mut results_sig = results;
            let mut profile_results_sig = profile_results;
            let mut loading_sig = loading;
            let mut error_sig = error;
            let guard = stale_guard;
            let task = spawn(async move {
                let search_result = search_profiles(&q, limit, true).await;
                if guard.is_stale(token) {
                    return;
                }
                match search_result {
                    Ok(profiles) => {
                        profile_results_sig.set(profiles.clone());
                        results_sig.set(Vec::new());
                        search_results_cache::store(&q, "people", Vec::new(), profiles);
                        loading_sig.set(false);
                    }
                    Err(e) => {
                        error_sig.set(Some(format!("Search failed: {e}")));
                        loading_sig.set(false);
                    }
                }
            });
            search_task.set(Some(task));
            return;
        }

        let mut results_sig = results;
        let mut loading_sig = loading;
        let mut error_sig = error;
        let mut unreachable_sig = search_relays_unreachable;
        let guard = stale_guard;
        let tab_cache_key = tab.cache_key().to_string();
        let mut batch_results = results_sig;
        let batch_guard = stale_guard;
        let batch_guard_token = token;
        let task = spawn(async move {
            let outcome = search_content_streaming(
                &q,
                limit,
                &tab_cache_key,
                &contacts,
                move |batch| {
                    if batch_guard.is_stale(batch_guard_token) {
                        return;
                    }
                    // Progressive fill: append deduped new items.
                    batch_results.write().extend(batch);
                },
            )
            .await;
            if guard.is_stale(token) {
                return;
            }
            match outcome {
                Ok((final_results, _)) => {
                    // Authoritative replace: includes engagement enrichment
                    // and client-side `-exclude` filtering.
                    results_sig.set(final_results.clone());
                    search_results_cache::store(&q, &tab_cache_key, final_results, Vec::new());
                    loading_sig.set(false);
                }
                Err(e) if e == ERR_SEARCH_RELAYS_UNREACHABLE => {
                    unreachable_sig.set(true);
                    loading_sig.set(false);
                }
                Err(e) => {
                    error_sig.set(Some(format!("Search failed: {e}")));
                    loading_sig.set(false);
                }
            }
        });
        search_task.set(Some(task));
    });

    let tabs = [
        SearchTab::TextNotes,
        SearchTab::Articles,
        SearchTab::Photos,
        SearchTab::Videos,
        SearchTab::People,
    ];

    let sorted_results = use_memo(move || {
        let mut sorted = results.read().clone();
        let order = *sort_order.read();
        match order {
            SortOrder::Newest => {
                sorted.sort_by_key(|b| std::cmp::Reverse(b.event.created_at));
            }
            SortOrder::Oldest => {
                sorted.sort_by_key(|a| a.event.created_at);
            }
            SortOrder::FollowingFirst => {
                sorted.sort_by(|a, b| match (a.is_from_contact, b.is_from_contact) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => b.event.created_at.cmp(&a.event.created_at),
                });
            }
            SortOrder::Relevance => {
                sorted.sort_by_key(|b| std::cmp::Reverse(b.relevance));
            }
        }
        sorted
    });

    let has_more = *current_limit.read() < MAX_LIMIT
        && results.read().len() >= *current_limit.read();

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3",
                    h2 { class: "text-xl font-bold flex items-center gap-2",
                        span { "🔍" }
                        "Search Results"
                    }
                    p { class: "text-sm text-muted-foreground mt-1",
                        "Searching for: \"{query.read()}\""
                    }
                    {render_query_chips(&detected_type.read(), &unknown_authors.read())}
                }
                div { class: "flex border-b border-border overflow-x-auto scrollbar-hide",
                    for tab in tabs.iter() {
                        {
                            let tab_value = *tab;
                            let is_active = *active_tab.read() == tab_value;
                            rsx! {
                                button {
                                    key: "{tab.label()}",
                                    class: if is_active { "px-6 py-3 text-sm font-medium border-b-2 border-primary text-primary transition" } else { "px-6 py-3 text-sm font-medium border-b-2 border-transparent text-muted-foreground hover:text-foreground hover:border-border transition" },
                                    onclick: move |_| {
                                        active_tab.set(tab_value);
                                    },
                                    "{tab.label()}"
                                }
                            }
                        }
                    }
                }
            }
            if *search_relays_unreachable.read() {
                div { class: "p-4",
                    div { class: "p-4 bg-yellow-500/10 border border-yellow-500/30 text-yellow-700 dark:text-yellow-300 rounded-lg text-sm flex items-center justify-between gap-3",
                        span {
                            "⚠️ No NIP-50 search relays reachable — full-text search is unavailable. "
                            "Hashtag and local results still work."
                        }
                        a {
                            class: "shrink-0 underline underline-offset-2 hover:opacity-80",
                            href: "#",
                            onclick: move |e: dioxus::prelude::Event<MouseData>| {
                                e.prevent_default();
                                let navigator = navigator();
                                navigator.push(crate::routes::Route::SettingsRelays {});
                            },
                            "Settings → Relays"
                        }
                    }
                }
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "p-4",
                    div { class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "❌ {err}"
                    }
                }
            }
            if *loading.read() && results.read().is_empty() && profile_results.read().is_empty() {
                div { class: "divide-y divide-border",
                    for i in 0..5 {
                        NoteCardSkeleton { key: "{i}" }
                    }
                }
            }
            if !*loading.read() && results.read().is_empty() && profile_results.read().is_empty() && !query.read().is_empty() {
                div { class: "flex flex-col items-center justify-center py-16 px-4",
                    div { class: "text-6xl mb-4", "🔍" }
                    p { class: "text-lg font-medium text-muted-foreground mb-2", "No results found" }
                    p { class: "text-sm text-muted-foreground text-center max-w-md",
                        "Try searching with different keywords or switch to another tab"
                    }
                }
            }
            if *active_tab.read() == SearchTab::People && !profile_results.read().is_empty() {
                {render_people_results(&profile_results.read())}
            }
            if *active_tab.read() != SearchTab::People && !results.read().is_empty() {
                div { class: "divide-y divide-border",
                    div { class: "px-4 py-3 bg-muted/30 flex items-center justify-between gap-4",
                        p { class: "text-sm text-muted-foreground",
                            "Found {results.read().len()} {active_tab.read().label().to_lowercase()}"
                            if *loading.read() {
                                span { class: "ml-2", "·" }
                                span { class: "animate-pulse", "streaming…" }
                            }
                        }
                        div { class: "relative",
                            button {
                                class: "flex items-center gap-2 px-3 py-1.5 text-sm bg-background border border-border rounded-lg hover:bg-accent/50 transition",
                                onclick: move |_| {
                                    let current = *show_sort_dropdown.read();
                                    show_sort_dropdown.set(!current);
                                },
                                svg {
                                    class: "w-4 h-4 text-muted-foreground",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "m3 16 4 4 4-4" }
                                    path { d: "M7 20V4" }
                                    path { d: "m21 8-4-4-4 4" }
                                    path { d: "M17 4v16" }
                                }
                                span { "{sort_order.read().label()}" }
                                svg {
                                    class: "w-4 h-4 text-muted-foreground",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "m6 9 6 6 6-6" }
                                }
                            }
                            if *show_sort_dropdown.read() {
                                div {
                                    class: "fixed inset-0 z-40",
                                    onclick: move |_| show_sort_dropdown.set(false),
                                }
                                div { class: "absolute right-0 top-full mt-1 w-40 bg-background border border-border rounded-lg shadow-lg z-50 overflow-hidden",
                                    for option in [SortOrder::Newest, SortOrder::Oldest, SortOrder::FollowingFirst, SortOrder::Relevance] {
                                        {
                                            let is_selected = *sort_order.read() == option;
                                            rsx! {
                                                button {
                                                    key: "{option.label()}",
                                                    class: if is_selected { "w-full px-4 py-2 text-sm text-left bg-accent/50 text-foreground" } else { "w-full px-4 py-2 text-sm text-left hover:bg-accent/30 text-foreground" },
                                                    onclick: move |_| {
                                                        sort_order.set(option);
                                                        show_sort_dropdown.set(false);
                                                    },
                                                    div { class: "flex items-center gap-2",
                                                        if is_selected {
                                                            svg {
                                                                class: "w-4 h-4 text-primary",
                                                                xmlns: "http://www.w3.org/2000/svg",
                                                                view_box: "0 0 24 24",
                                                                fill: "none",
                                                                stroke: "currentColor",
                                                                stroke_width: "2",
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                path { d: "M20 6 9 17l-5-5" }
                                                            }
                                                        } else {
                                                            div { class: "w-4 h-4" }
                                                        }
                                                        "{option.label()}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    for result in sorted_results.read().iter() {
                        {
                            let event_clone = result.event.clone();
                            let is_from_contact = result.is_from_contact;
                            let tab = *active_tab.read();
                            rsx! {
                                div {
                                    key: "{result.event.id.to_hex()}",
                                    class: if is_from_contact { "relative border-l-4 border-l-blue-500" } else { "" },
                                    if is_from_contact {
                                        div { class: "absolute top-2 right-2 z-10",
                                            span { class: "text-xs px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded-full",
                                                "Following"
                                            }
                                        }
                                    }
                                    match tab {
                                        SearchTab::TextNotes | SearchTab::Articles => rsx! {
                                            NoteCard {
                                                event: event_clone,
                                                collapsible: true,
                                                cached_muted_posts: cached_muted_posts.read().clone(),
                                                cached_blocked_users: cached_blocked_users.read().clone(),
                                                cached_muted_words: cached_muted_words.read().clone(),
                                            }
                                        },
                                        SearchTab::Photos => rsx! {
                                            PhotoCard { event: event_clone }
                                        },
                                        SearchTab::Videos => rsx! {
                                            VideoCard { event: event_clone }
                                        },
                                        SearchTab::People => rsx! {},
                                    }
                                }
                            }
                        }
                    }
                    if has_more {
                        div { class: "p-4 text-center",
                            button {
                                class: "px-6 py-2 text-sm font-medium bg-background border border-border rounded-full hover:bg-accent/50 transition disabled:opacity-50",
                                disabled: *loading.read(),
                                onclick: move |_| {
                                    let next = (*current_limit.read() + BASE_LIMIT).min(MAX_LIMIT);
                                    current_limit.set(next);
                                },
                                if *loading.read() {
                                    "Loading…"
                                } else {
                                    "Load more"
                                }
                            }
                        }
                    }
                }
            }
            if query.read().is_empty() {
                div { class: "flex flex-col items-center justify-center py-16 px-4",
                    div { class: "text-6xl mb-4", "🔍" }
                    p { class: "text-lg font-medium text-muted-foreground mb-2", "Start searching" }
                    p { class: "text-sm text-muted-foreground text-center max-w-md",
                        "Use the search bar above to find posts, articles, photos, and videos on Nostr"
                    }
                }
            }
        }
    }
}

fn render_query_chips(
    search_type: &query_parser::SearchType,
    unknown_authors: &[String],
) -> Element {
    match search_type {
        query_parser::SearchType::FullText(parsed) => {
            let mut chips = Vec::new();
            if !parsed.kinds.is_empty() {
                for kind in &parsed.kinds {
                    chips.push(format!("kind:{}", kind.as_u16()));
                }
            }
            if let Some(since) = parsed.since {
                chips.push(format!("since:{}", since.as_secs()));
            }
            if let Some(until) = parsed.until {
                chips.push(format!("until:{}", until.as_secs()));
            }
            if !parsed.hashtags.is_empty() {
                for tag in &parsed.hashtags {
                    chips.push(format!("#{}", tag));
                }
            }
            if !parsed.authors.is_empty() {
                chips.push(format!("from:{} authors", parsed.authors.len()));
            }
            for name in unknown_authors {
                chips.push(format!("unknown author: {name}"));
            }
            if let Some(lang) = &parsed.language {
                chips.push(format!("lang:{}", lang));
            }
            if let Some(domain) = &parsed.domain {
                chips.push(format!("domain:{}", domain));
            }
            if !parsed.exclude_terms.is_empty() {
                for term in &parsed.exclude_terms {
                    chips.push(format!("-{}", term));
                }
            }
            if chips.is_empty() {
                return rsx! {};
            }
            rsx! {
                div { class: "flex flex-wrap gap-1.5 mt-2",
                    for chip in chips {
                        span {
                            key: "{chip}",
                            class: if chip.starts_with("unknown author") {
                                "text-xs px-2 py-0.5 bg-orange-500/10 text-orange-600 dark:text-orange-400 rounded-full"
                            } else {
                                "text-xs px-2 py-0.5 bg-primary/10 text-primary rounded-full"
                            },
                            "{chip}"
                        }
                    }
                }
            }
        }
        query_parser::SearchType::Hashtag(tag) => rsx! {
            div { class: "mt-1",
                span { class: "text-xs px-2 py-0.5 bg-primary/10 text-primary rounded-full",
                    "Hashtag: #{tag}"
                }
            }
        },
        _ => rsx! {},
    }
}

fn render_people_results(profiles: &[ProfileSearchResult]) -> Element {
    let navigator = navigator();
    rsx! {
        div { class: "divide-y divide-border",
            for profile in profiles {
                {
                    let profile_clone = profile.clone();
                    rsx! {
                        button {
                            key: "{profile.pubkey.to_hex()}",
                            class: "w-full px-4 py-3 flex items-center gap-3 hover:bg-muted cursor-pointer transition text-left",
                            onclick: move |_| {
                                let pubkey_hex = profile_clone.pubkey.to_hex();
                                search_history::add_profile(
                                    pubkey_hex.clone(),
                                    profile_clone.get_display_name(),
                                );
                                navigator.push(crate::routes::Route::AddressViewer {
                                    address: crate::utils::nip19_urls::profile_route_id(&pubkey_hex),
                                });
                            },
                            div { class: "shrink-0",
                                if let Some(picture) = &profile.picture {
                                    img {
                                        src: "{picture}",
                                        class: "w-10 h-10 rounded-full object-cover",
                                        alt: "{profile.get_display_name()}",
                                        loading: "lazy",
                                    }
                                } else {
                                    div { class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center text-sm font-bold",
                                        {profile.get_display_name().chars().next().unwrap_or('?').to_string()}
                                    }
                                }
                            }
                            div { class: "flex-1 min-w-0",
                                div { class: "font-semibold text-sm truncate",
                                    {profile.get_display_name()}
                                }
                                if let Some(username) = profile.get_username() {
                                    div { class: "text-xs text-muted-foreground truncate",
                                        "@{username}"
                                    }
                                }
                            }
                            if profile.is_contact {
                                div { class: "shrink-0 text-xs px-2 py-1 bg-primary/10 text-primary rounded-full",
                                    "Following"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
