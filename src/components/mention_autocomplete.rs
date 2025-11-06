use dioxus::prelude::*;
use dioxus::prelude::Event as DioxusEvent;
use nostr_sdk::prelude::*;
use std::rc::Rc;
use wasm_bindgen::JsCast;

use crate::services::profile_search::{search_profiles, search_cached_profiles, get_contact_pubkeys, ProfileSearchResult};

#[derive(Props, Clone, PartialEq)]
pub struct MentionAutocompleteProps {
    /// Current content of the textarea
    pub content: Signal<String>,
    /// Callback when content changes (includes mention insertions)
    pub on_input: EventHandler<String>,
    /// Textarea placeholder
    #[props(default = "What's happening?".to_string())]
    pub placeholder: String,
    /// Number of rows for the textarea
    #[props(default = 2)]
    pub rows: u32,
    /// Additional CSS classes for the textarea
    #[props(default = "w-full p-3 text-lg bg-transparent border border-input rounded-lg focus:outline-none focus:ring-2 focus:ring-ring resize-none".to_string())]
    pub class: String,
    /// Whether the textarea is disabled
    #[props(default = false)]
    pub disabled: bool,
    /// Focus handler
    #[props(default)]
    pub onfocus: Option<EventHandler>,
    /// Optional thread participants (e.g., for reply composers)
    #[props(default = Vec::new())]
    pub thread_participants: Vec<PublicKey>,
}

#[component]
pub fn MentionAutocomplete(props: MentionAutocompleteProps) -> Element {
    let mut show_autocomplete = use_signal(|| false);
    let search_results = use_signal(|| Vec::<ProfileSearchResult>::new());
    let mut selected_index = use_signal(|| 0usize);
    let is_searching = use_signal(|| false);
    let mention_query = use_signal(|| String::new());
    let mention_start_pos = use_signal(|| 0usize);
    let mut cursor_position = use_signal(|| 0usize);

    // Dropdown positioning
    let mut dropdown_top = use_signal(|| 0.0);
    let mut dropdown_left = use_signal(|| 0.0);
    let mut show_below = use_signal(|| true);

    let textarea_id = use_signal(|| Rc::new(format!("mention-textarea-{}", uuid::Uuid::new_v4())));

    // Debounced relay search task
    let relay_search_task = use_signal(|| None::<Task>);

    // Contact list cache
    let mut contact_pubkeys = use_signal(|| Vec::<PublicKey>::new());

    // Fetch contacts on mount
    use_effect(move || {
        spawn(async move {
            let contacts = get_contact_pubkeys().await;
            contact_pubkeys.set(contacts);
        });
    });

    let handle_input = move |evt: DioxusEvent<FormData>| {
        let new_value = evt.value().clone();
        let cursor_pos = get_cursor_position(&**textarea_id.read());
        cursor_position.set(cursor_pos);

        // Update content
        props.on_input.call(new_value.clone());

        // Detect @ mentions
        detect_mention(&new_value, cursor_pos, show_autocomplete, mention_query, mention_start_pos, is_searching, search_results, selected_index, relay_search_task, contact_pubkeys, &props.thread_participants);

        // Update dropdown position if showing
        if *show_autocomplete.read() {
            update_dropdown_position(&**textarea_id.read(), &mut dropdown_top, &mut dropdown_left, &mut show_below);
        }
    };

    let handle_keydown = move |evt: DioxusEvent<KeyboardData>| {
        if !*show_autocomplete.read() {
            return;
        }

        let key = evt.key();
        let results = search_results.read();

        match key {
            Key::ArrowDown => {
                evt.prevent_default();
                let current = *selected_index.read();
                let max = results.len().saturating_sub(1);
                if current < max {
                    selected_index.set(current + 1);
                }
            }
            Key::ArrowUp => {
                evt.prevent_default();
                let current = *selected_index.read();
                if current > 0 {
                    selected_index.set(current - 1);
                }
            }
            Key::Enter => {
                if !results.is_empty() {
                    evt.prevent_default();
                    let selected = results.get(*selected_index.read());
                    if let Some(profile) = selected {
                        insert_mention(
                            profile.clone(),
                            props.content,
                            props.on_input.clone(),
                            *mention_start_pos.read(),
                            mention_query.read().len(),
                            (**textarea_id.read()).clone(),
                            show_autocomplete,
                        );
                    }
                }
            }
            Key::Escape => {
                show_autocomplete.set(false);
            }
            _ => {}
        }
    };

    let handle_focus = move |_| {
        if let Some(handler) = &props.onfocus {
            handler.call(());
        }
    };

    rsx! {
        div {
            class: "relative w-full",

            // Textarea
            textarea {
                id: "{textarea_id}",
                class: "{props.class}",
                placeholder: "{props.placeholder}",
                rows: "{props.rows}",
                value: "{props.content}",
                disabled: props.disabled,
                oninput: handle_input,
                onkeydown: handle_keydown,
                onfocus: handle_focus,
            }

            // Autocomplete dropdown
            if *show_autocomplete.read() {
                {render_dropdown(
                    &search_results.read(),
                    *selected_index.read(),
                    *is_searching.read(),
                    *dropdown_top.read(),
                    *dropdown_left.read(),
                    *show_below.read(),
                    props.content,
                    props.on_input.clone(),
                    *mention_start_pos.read(),
                    mention_query.read().len(),
                    (**textarea_id.read()).clone(),
                    show_autocomplete,
                )}
            }
        }
    }
}

/// Detect @ mention in text at cursor position
fn detect_mention(
    text: &str,
    cursor_pos: usize,
    mut show_autocomplete: Signal<bool>,
    mut mention_query: Signal<String>,
    mut mention_start_pos: Signal<usize>,
    mut is_searching: Signal<bool>,
    mut search_results: Signal<Vec<ProfileSearchResult>>,
    mut selected_index: Signal<usize>,
    mut relay_search_task: Signal<Option<Task>>,
    contact_pubkeys: Signal<Vec<PublicKey>>,
    thread_pubkeys: &[PublicKey],
) {
    // Convert UTF-16 cursor position (from DOM) to UTF-8 byte index
    let cursor_byte_index = utf16_to_utf8_index(text, cursor_pos);

    // Get text before cursor
    let before_cursor = &text[..cursor_byte_index];

    // Find the last @ symbol before cursor
    if let Some(at_pos) = before_cursor.rfind('@') {
        let after_at = &before_cursor[at_pos + 1..];

        // Check if there's whitespace after @ (if so, don't show autocomplete)
        if after_at.contains(char::is_whitespace) {
            show_autocomplete.set(false);
            return;
        }

        // Valid mention query
        let query = after_at.to_string();
        mention_query.set(query.clone());
        mention_start_pos.set(at_pos);
        show_autocomplete.set(true);
        selected_index.set(0);

        // Search cached profiles immediately (no debounce for instant results)
        let contacts = contact_pubkeys.read().clone();
        let cached_results = search_cached_profiles(&query, 10, &contacts, thread_pubkeys);
        search_results.set(cached_results.clone());

        log::debug!("Autocomplete search for '{}': found {} results ({} thread participants)",
            query, cached_results.len(), thread_pubkeys.len());

        // Only query relays if we don't have enough results and query is long enough
        if query.len() >= 2 && cached_results.len() < 5 {
            is_searching.set(true);

            // Cancel previous relay search task if any
            if let Some(task) = relay_search_task.read().as_ref() {
                task.cancel();
            }

            // Capture query for stale result verification
            let query_snapshot = query.clone();

            // Start new relay search task with debounce
            let new_task = spawn(async move {
                // Debounce: wait 300ms
                #[cfg(target_family = "wasm")]
                {
                    gloo_timers::future::TimeoutFuture::new(300).await;
                }
                #[cfg(not(target_family = "wasm"))]
                {
                    use std::time::Duration;
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }

                // Perform relay search
                let query_relays = query_snapshot.len() >= 3; // Only query relays for 3+ chars
                match search_profiles(&query_snapshot, 10, query_relays).await {
                    Ok(results) => {
                        // Only update results if query hasn't changed (avoid stale results)
                        if mention_query.read().as_str() == query_snapshot.as_str() {
                            search_results.set(results);
                            is_searching.set(false);
                        } else {
                            log::debug!("Ignoring stale search results for '{}' (current query: '{}')",
                                query_snapshot, mention_query.read());
                        }
                    }
                    Err(e) => {
                        log::error!("Profile search failed: {}", e);
                        // Only clear searching state if query hasn't changed
                        if mention_query.read().as_str() == query_snapshot.as_str() {
                            is_searching.set(false);
                        }
                    }
                }
            });

            relay_search_task.set(Some(new_task));
        } else {
            is_searching.set(false);
        }
    } else {
        // No @ found before cursor
        show_autocomplete.set(false);
    }
}

/// Insert a mention into the textarea
fn insert_mention(
    profile: ProfileSearchResult,
    content: Signal<String>,
    on_input: EventHandler<String>,
    mention_start_pos: usize,
    query_len: usize,
    textarea_id: String,
    mut show_autocomplete: Signal<bool>,
) {
    spawn(async move {
        // With gossip, relay hints are not needed - the client handles routing automatically
        let nprofile = nips::nip19::Nip19Profile::new(profile.pubkey, vec![]);

        // Encode to bech32
        let mention = match nprofile.to_bech32() {
            Ok(bech32) => format!("nostr:{}", bech32),
            Err(e) => {
                log::error!("Failed to encode nprofile: {}", e);
                return;
            }
        };

        // Calculate positions
        let current_content = content.read().to_string();
        let query_end_pos = mention_start_pos + query_len + 1; // +1 for the @ symbol

        // Build new content
        let before = &current_content[..mention_start_pos];
        let after = &current_content[query_end_pos.min(current_content.len())..];
        let new_content = format!("{}{} {}", before, mention, after);

        // Update content
        on_input.call(new_content.clone());

        // Hide autocomplete
        show_autocomplete.set(false);

        // Restore focus and cursor position
        #[cfg(target_family = "wasm")]
        {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) = document.get_element_by_id(&textarea_id) {
                        if let Ok(textarea) = element.dyn_into::<web_sys::HtmlTextAreaElement>() {
                            // Calculate UTF-8 byte position
                            let new_cursor_byte_pos = before.len() + mention.len() + 1; // +1 for space
                            // Convert to UTF-16 code unit index for DOM
                            let new_cursor_utf16_pos = utf8_to_utf16_index(&new_content, new_cursor_byte_pos) as u32;
                            let _ = textarea.set_selection_range(new_cursor_utf16_pos, new_cursor_utf16_pos);
                            let _ = textarea.focus();
                        }
                    }
                }
            }
        }
    });
}

/// Convert UTF-16 code unit index (from DOM) to UTF-8 byte index (for Rust string slicing)
fn utf16_to_utf8_index(text: &str, utf16_index: usize) -> usize {
    let mut utf16_count = 0;
    let mut utf8_byte_index = 0;

    for ch in text.chars() {
        if utf16_count >= utf16_index {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_byte_index += ch.len_utf8();
    }

    // Clamp to valid UTF-8 byte boundaries
    utf8_byte_index.min(text.len())
}

/// Convert UTF-8 byte index (from Rust string) to UTF-16 code unit index (for DOM)
#[allow(dead_code)]
fn utf8_to_utf16_index(text: &str, utf8_index: usize) -> usize {
    let mut utf16_count = 0;
    let mut utf8_byte_index = 0;

    for ch in text.chars() {
        if utf8_byte_index >= utf8_index {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_byte_index += ch.len_utf8();
    }

    utf16_count
}

/// Get cursor position from textarea
#[allow(unused_variables)]
fn get_cursor_position(textarea_id: &str) -> usize {
    #[cfg(target_family = "wasm")]
    {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(element) = document.get_element_by_id(textarea_id) {
                    if let Ok(textarea) = element.dyn_into::<web_sys::HtmlTextAreaElement>() {
                        return textarea.selection_start().unwrap_or(None).unwrap_or(0) as usize;
                    }
                }
            }
        }
    }
    0
}

/// Update dropdown position based on cursor
#[allow(unused_variables)]
fn update_dropdown_position(
    textarea_id: &str,
    dropdown_top: &mut Signal<f64>,
    dropdown_left: &mut Signal<f64>,
    show_below: &mut Signal<bool>,
) {
    #[cfg(target_family = "wasm")]
    {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(element) = document.get_element_by_id(textarea_id) {
                    let rect = element.get_bounding_client_rect();

                    // For now, position below the textarea
                    // TODO: Calculate exact cursor pixel position for more accurate placement
                    let viewport_height = window
                        .inner_height()
                        .ok()
                        .and_then(|h| h.as_f64())
                        .unwrap_or(600.0);

                    let bottom_space = viewport_height - rect.bottom();
                    let top_space = rect.top();

                    // Show below if there's enough space (300px for dropdown)
                    if bottom_space >= 300.0 {
                        show_below.set(true);
                        dropdown_top.set(rect.bottom() + window.scroll_y().unwrap_or(0.0));
                    } else if top_space >= 300.0 {
                        show_below.set(false);
                        dropdown_top.set(rect.top() + window.scroll_y().unwrap_or(0.0) - 300.0);
                    } else {
                        // Default to below
                        show_below.set(true);
                        dropdown_top.set(rect.bottom() + window.scroll_y().unwrap_or(0.0));
                    }

                    dropdown_left.set(rect.left() + window.scroll_x().unwrap_or(0.0));
                }
            }
        }
    }
}

/// Render the autocomplete dropdown
fn render_dropdown(
    results: &[ProfileSearchResult],
    selected_index: usize,
    is_searching: bool,
    top: f64,
    left: f64,
    _show_below: bool,
    content: Signal<String>,
    on_input: EventHandler<String>,
    mention_start_pos: usize,
    query_len: usize,
    textarea_id: String,
    show_autocomplete: Signal<bool>,
) -> Element {
    // Wrap in Rc for cheap cloning
    let textarea_id_rc = Rc::new(textarea_id);

    rsx! {
        div {
            class: "fixed bg-white dark:bg-gray-800 shadow-lg rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden z-50",
            style: "top: {top}px; left: {left}px; max-height: 300px; width: 300px;",

            if is_searching {
                div {
                    class: "px-4 py-3 text-sm text-gray-500 dark:text-gray-400",
                    "Searching..."
                }
            } else if results.is_empty() {
                div {
                    class: "px-4 py-3 text-sm text-gray-500 dark:text-gray-400",
                    "No profiles found"
                }
            } else {
                div {
                    class: "overflow-y-auto max-h-[300px]",
                    for (index , profile) in results.iter().enumerate() {
                        {
                            let profile_clone = profile.clone();
                            let is_selected = index == selected_index;

                            rsx! {
                                button {
                                    key: "{profile.pubkey.to_hex()}",
                                    class: if is_selected {
                                        "w-full px-4 py-2 flex items-center gap-3 hover:bg-blue-50 dark:hover:bg-blue-900 bg-blue-50 dark:bg-blue-900 cursor-pointer transition"
                                    } else {
                                        "w-full px-4 py-2 flex items-center gap-3 hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer transition"
                                    },
                                    onclick: {
                                        let textarea_id_clone = textarea_id_rc.clone();
                                        move |_| {
                                            insert_mention(
                                                profile_clone.clone(),
                                                content,
                                                on_input.clone(),
                                                mention_start_pos,
                                                query_len,
                                                (*textarea_id_clone).clone(),
                                                show_autocomplete,
                                            );
                                        }
                                    },

                                    // Avatar
                                    div {
                                        class: "flex-shrink-0",
                                        if let Some(picture) = &profile.picture {
                                            img {
                                                src: "{picture}",
                                                class: "w-8 h-8 rounded-full",
                                                alt: "{profile.get_display_name()}",
                                            }
                                        } else {
                                            div {
                                                class: "w-8 h-8 rounded-full bg-gray-300 dark:bg-gray-600 flex items-center justify-center text-xs font-bold",
                                                {profile.get_display_name().chars().next().unwrap_or('?').to_string()}
                                            }
                                        }
                                    }

                                    // Profile info
                                    div {
                                        class: "flex-1 text-left min-w-0",
                                        div {
                                            class: "font-semibold text-sm text-gray-900 dark:text-gray-100 truncate",
                                            {profile.get_display_name()}
                                        }
                                        if let Some(username) = profile.get_username() {
                                            div {
                                                class: "text-xs text-gray-500 dark:text-gray-400 truncate",
                                                "@{username}"
                                            }
                                        }
                                    }

                                    // Thread/Contact badge
                                    if profile.is_thread_participant {
                                        div {
                                            class: "flex-shrink-0 text-xs px-2 py-1 bg-purple-100 dark:bg-purple-900 text-purple-700 dark:text-purple-300 rounded-full",
                                            "Thread"
                                        }
                                    } else if profile.is_contact {
                                        div {
                                            class: "flex-shrink-0 text-xs px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded-full",
                                            "Contact"
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
