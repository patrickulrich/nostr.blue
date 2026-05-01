use crate::services::profile_search::{
    get_contact_pubkeys, search_cached_profiles, search_profiles, ProfileSearchResult,
};
use crate::utils::is_valid_http_url;
use crate::utils::text::utf16_to_utf8_index;
use dioxus::prelude::Event as DioxusEvent;
use dioxus::prelude::*;
use dioxus_core::Task;
use nostr_sdk::prelude::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
/// Groups autocomplete-related signals to reduce parameter count in helper functions
#[derive(Clone, Copy)]
struct AutocompleteState {
    show: Signal<bool>,
    query: Signal<String>,
    start_pos: Signal<usize>,
    results: Signal<Vec<ProfileSearchResult>>,
    selected_index: Signal<usize>,
    is_searching: Signal<bool>,
    relay_search_task: Signal<Option<Task>>,
}
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
    #[props(
        default = "w-full p-3 text-lg bg-transparent border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring resize-none".to_string(

        )
    )]
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
    /// Optional signal to track cursor position externally
    #[props(optional)]
    pub cursor_position: Option<Signal<usize>>,
}
#[component]
pub fn MentionAutocomplete(props: MentionAutocompleteProps) -> Element {
    let mut autocomplete = AutocompleteState {
        show: use_signal(|| false),
        query: use_signal(String::new),
        start_pos: use_signal(|| 0usize),
        results: use_signal(Vec::<ProfileSearchResult>::new),
        selected_index: use_signal(|| 0usize),
        is_searching: use_signal(|| false),
        relay_search_task: use_signal(|| None::<Task>),
    };
    #[cfg_attr(not(feature = "web"), allow(unused_mut))]
    let mut dropdown_top = use_signal(|| 0.0);
    #[cfg_attr(not(feature = "web"), allow(unused_mut))]
    let mut dropdown_left = use_signal(|| 0.0);
    #[cfg_attr(not(feature = "web"), allow(unused_mut))]
    let mut show_below = use_signal(|| true);
    #[allow(unused_mut)]
    let mut is_mobile = use_signal(|| false);
    let textarea_id = use_signal(|| Rc::new(format!("mention-textarea-{}", uuid::Uuid::new_v4())));
    let mut contact_pubkeys = use_signal(Vec::<PublicKey>::new);
    use_effect(move || {
        spawn(async move {
            let contacts = get_contact_pubkeys().await;
            contact_pubkeys.set(contacts);
        });
    });
    let handle_input = move |evt: DioxusEvent<FormData>| {
        let new_value = evt.value().clone();
        props.on_input.call(new_value.clone());

        #[cfg(feature = "web")]
        {
            let Some(cursor_pos) = get_cursor_position(&textarea_id.read(), &new_value) else {
                autocomplete.show.set(false);
                return;
            };
            if let Some(mut signal) = props.cursor_position {
                let cursor_utf8 = utf16_to_utf8_index(&new_value, cursor_pos);
                signal.set(cursor_utf8);
            }
            detect_mention(
                &new_value,
                cursor_pos,
                &mut autocomplete,
                contact_pubkeys,
                &props.thread_participants,
            );
            if *autocomplete.show.read() {
                update_dropdown_position(
                    &textarea_id.read(),
                    &mut dropdown_top,
                    &mut dropdown_left,
                    &mut show_below,
                    &mut is_mobile,
                );
            }
        }

        #[cfg(not(feature = "web"))]
        {
            let textarea_id_str = (*textarea_id.read()).clone();
            let mut ac = autocomplete;
            let contacts = contact_pubkeys;
            let thread = props.thread_participants.clone();
            let cursor_signal = props.cursor_position;
            let mut mobile_sig = is_mobile;
            spawn(async move {
                let cursor_pos = get_cursor_position_eval(&textarea_id_str)
                    .await
                    .unwrap_or_else(|| new_value.chars().map(|c| c.len_utf16()).sum());
                if let Some(mut signal) = cursor_signal {
                    signal.set(utf16_to_utf8_index(&new_value, cursor_pos));
                }
                detect_mention(&new_value, cursor_pos, &mut ac, contacts, &thread);
                if *ac.show.read() {
                    mobile_sig.set(true);
                }
            });
        }
    };
    let handle_keydown = move |evt: DioxusEvent<KeyboardData>| {
        if !*autocomplete.show.read() {
            return;
        }
        let key = evt.key();
        let results = autocomplete.results.read();
        match key {
            Key::ArrowDown => {
                evt.prevent_default();
                let current = *autocomplete.selected_index.read();
                let max = results.len().saturating_sub(1);
                if current < max {
                    let new_index = current + 1;
                    autocomplete.selected_index.set(new_index);
                    let _ = document::eval(&format!(
                        r#"document.getElementById('mention-option-{}')?.scrollIntoView({{ block: 'nearest', behavior: 'smooth' }})"#,
                        new_index,
                    ));
                }
            }
            Key::ArrowUp => {
                evt.prevent_default();
                let current = *autocomplete.selected_index.read();
                if current > 0 {
                    let new_index = current - 1;
                    autocomplete.selected_index.set(new_index);
                    let _ = document::eval(&format!(
                        r#"document.getElementById('mention-option-{}')?.scrollIntoView({{ block: 'nearest', behavior: 'smooth' }})"#,
                        new_index,
                    ));
                }
            }
            Key::Enter if !results.is_empty() => {
                evt.prevent_default();
                let selected = results.get(*autocomplete.selected_index.read());
                if let Some(profile) = selected {
                    insert_mention(
                        profile.clone(),
                        props.content,
                        props.on_input,
                        *autocomplete.start_pos.read(),
                        autocomplete.query.read().len(),
                        (**textarea_id.read()).clone(),
                        autocomplete.show,
                        props.cursor_position,
                    );
                }
            }
            Key::Escape => {
                autocomplete.show.set(false);
            }
            _ => {}
        }
    };
    let handle_focus = move |_| {
        if let Some(handler) = &props.onfocus {
            handler.call(());
        }
    };
    let sync_cursor_position = move || {
        let text = props.content.read().clone();
        if let Some(cursor_pos) = get_cursor_position(&textarea_id.read(), &text) {
            if let Some(mut signal) = props.cursor_position {
                let cursor_utf8 = utf16_to_utf8_index(&text, cursor_pos);
                signal.set(cursor_utf8);
            }
        }
    };
    let handle_keyup = move |_| {
        sync_cursor_position();
    };
    let handle_click = move |_| {
        sync_cursor_position();
    };
    rsx! {
        div { class: "relative w-full",
            textarea {
                id: "{textarea_id}",
                class: "{props.class}",
                placeholder: "{props.placeholder}",
                rows: "{props.rows}",
                value: "{props.content}",
                disabled: props.disabled,
                oninput: handle_input,
                onkeydown: handle_keydown,
                onkeyup: handle_keyup,
                onclick: handle_click,
                onfocus: handle_focus,
            }
            if *autocomplete.show.read() {
                {
                    render_dropdown(
                        &autocomplete.results.read(),
                        *autocomplete.selected_index.read(),
                        *autocomplete.is_searching.read(),
                        *dropdown_top.read(),
                        *dropdown_left.read(),
                        *show_below.read(),
                        *is_mobile.read(),
                        props.content,
                        props.on_input,
                        *autocomplete.start_pos.read(),
                        autocomplete.query.read().len(),
                        (**textarea_id.read()).clone(),
                        autocomplete.show,
                        props.cursor_position,
                    )
                }
            }
        }
    }
}
/// Detect @ mention in text at cursor position
fn detect_mention(
    text: &str,
    cursor_pos: usize,
    state: &mut AutocompleteState,
    contact_pubkeys: Signal<Vec<PublicKey>>,
    thread_pubkeys: &[PublicKey],
) {
    let cursor_byte_index = utf16_to_utf8_index(text, cursor_pos);
    let before_cursor = &text[..cursor_byte_index];
    if let Some(at_pos) = before_cursor.rfind('@') {
        let after_at = &before_cursor[at_pos + 1..];
        if after_at.contains(char::is_whitespace) {
            state.show.set(false);
            // Cancel any pending search task
            if let Some(task) = state.relay_search_task.read().as_ref() {
                task.cancel();
            }
            state.relay_search_task.write().take();
            state.is_searching.set(false);
            return;
        }
        let query = after_at.to_string();
        state.query.set(query.clone());
        state.start_pos.set(at_pos);
        state.show.set(true);
        state.selected_index.set(0);
        let contacts = contact_pubkeys.read().clone();
        let cached_results = search_cached_profiles(&query, 10, &contacts, thread_pubkeys);
        state.results.set(cached_results.clone());
        log::debug!(
            "Autocomplete search for '{}': found {} results ({} thread participants)",
            query,
            cached_results.len(),
            thread_pubkeys.len()
        );
        if query.len() >= 3 && cached_results.len() < 5 {
            state.is_searching.set(true);
            if let Some(task) = state.relay_search_task.read().as_ref() {
                task.cancel();
            }
            let query_snapshot = query.clone();
            let query_signal = state.query;
            let mut results_signal = state.results;
            let mut searching_signal = state.is_searching;
            let mut task_signal = state.relay_search_task;
            let thread_pubkeys_for_relay = thread_pubkeys.to_vec();
            let new_task = spawn(async move {
                crate::platform::timer::sleep_ms(300).await;
                let query_relays = query_snapshot.len() >= 3;
                match search_profiles(&query_snapshot, 10, query_relays).await {
                    Ok(results) => {
                        if query_signal.read().as_str() == query_snapshot.as_str() {
                            let cached_by_pubkey: HashMap<_, _> = cached_results
                                .iter()
                                .map(|cached| (cached.pubkey, cached.clone()))
                                .collect();
                            let mut merged: Vec<_> = results
                                .into_iter()
                                .map(|mut relay_result| {
                                    if let Some(cached) = cached_by_pubkey.get(&relay_result.pubkey)
                                    {
                                        if relay_result.picture.is_none() {
                                            relay_result.picture = cached.picture.clone();
                                        }
                                        if relay_result.display_name.is_none() {
                                            relay_result.display_name = cached.display_name.clone();
                                        }
                                        if relay_result.name.is_none() {
                                            relay_result.name = cached.name.clone();
                                        }
                                        if relay_result.nip05.is_none() {
                                            relay_result.nip05 = cached.nip05.clone();
                                        }
                                        relay_result.is_contact |= cached.is_contact;
                                        relay_result.is_thread_participant |=
                                            cached.is_thread_participant;
                                        relay_result.relevance =
                                            relay_result.relevance.max(cached.relevance);
                                    }
                                    if thread_pubkeys_for_relay.contains(&relay_result.pubkey)
                                        && !relay_result.is_thread_participant
                                    {
                                        relay_result.is_thread_participant = true;
                                        relay_result.relevance += 2000;
                                    }
                                    relay_result
                                })
                                .collect();
                            let mut present =
                                merged.iter().map(|r| r.pubkey).collect::<HashSet<_>>();
                            for cached in cached_results.iter() {
                                if !thread_pubkeys_for_relay.contains(&cached.pubkey)
                                    || present.contains(&cached.pubkey)
                                {
                                    continue;
                                }
                                let mut extra = cached.clone();
                                extra.is_thread_participant = true;
                                present.insert(extra.pubkey);
                                merged.push(extra);
                            }
                            merged.sort_by_key(|b| std::cmp::Reverse(b.relevance));
                            merged.truncate(10);
                            results_signal.set(merged);
                            searching_signal.set(false);
                        } else {
                            log::debug!(
                                "Ignoring stale search results for '{}' (current query: '{}')",
                                query_snapshot,
                                query_signal.read()
                            );
                        }
                    }
                    Err(e) => {
                        log::error!("Profile search failed: {}", e);
                        if query_signal.read().as_str() == query_snapshot.as_str() {
                            searching_signal.set(false);
                        }
                    }
                }
            });
            task_signal.set(Some(new_task));
        } else {
            if let Some(task) = state.relay_search_task.read().as_ref() {
                task.cancel();
            }
            state.relay_search_task.write().take();
            state.is_searching.set(false);
        }
    } else {
        // Cancel any pending search task before hiding
        if let Some(task) = state.relay_search_task.read().as_ref() {
            task.cancel();
        }
        state.relay_search_task.write().take();
        state.show.set(false);
    }
}
/// Insert a mention into the textarea
#[allow(clippy::too_many_arguments)]
fn insert_mention(
    profile: ProfileSearchResult,
    content: Signal<String>,
    on_input: EventHandler<String>,
    mention_start_pos: usize,
    query_len: usize,
    #[allow(unused_variables)] textarea_id: String,
    mut show_autocomplete: Signal<bool>,
    external_cursor_position: Option<Signal<usize>>,
) {
    spawn(async move {
        let relay_hints: Vec<nostr_sdk::RelayUrl> = [
            "wss://relay.damus.io",
            "wss://nos.lol",
            "wss://relay.snort.social",
        ]
        .iter()
        .filter_map(|r| nostr_sdk::RelayUrl::parse(r).ok())
        .collect();
        let nprofile = nips::nip19::Nip19Profile::new(profile.pubkey, relay_hints);
        let mention = match nprofile.to_bech32() {
            Ok(bech32) => format!("nostr:{}", bech32),
            Err(e) => {
                log::error!("Failed to encode nprofile: {}", e);
                return;
            }
        };
        let current_content = content.read().to_string();
        if mention_start_pos > current_content.len()
            || !current_content.is_char_boundary(mention_start_pos)
        {
            log::warn!(
                "Mention start position {} is invalid for content of length {}",
                mention_start_pos,
                current_content.len()
            );
            show_autocomplete.set(false);
            return;
        }
        let query_end_pos = mention_start_pos + query_len + 1;
        let safe_query_end = query_end_pos.min(current_content.len());
        let safe_query_end = if current_content.is_char_boundary(safe_query_end) {
            safe_query_end
        } else {
            (safe_query_end..=current_content.len())
                .find(|&i| current_content.is_char_boundary(i))
                .unwrap_or(current_content.len())
        };
        let before = &current_content[..mention_start_pos];
        let after = &current_content[safe_query_end..];
        let new_content = format!("{}{} {}", before, mention, after);
        let new_cursor_byte_pos = before.len() + mention.len() + 1;
        on_input.call(new_content.clone());
        show_autocomplete.set(false);
        if let Some(mut signal) = external_cursor_position {
            signal.set(new_cursor_byte_pos);
        }
        #[cfg(feature = "web")]
        {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) = document.get_element_by_id(&textarea_id) {
                        if let Ok(textarea) = element.dyn_into::<web_sys::HtmlTextAreaElement>() {
                            let new_cursor_utf16_pos =
                                utf8_to_utf16_index(&new_content, new_cursor_byte_pos) as u32;
                            crate::platform::timer::sleep_ms(10).await;
                            let _ = textarea
                                .set_selection_range(new_cursor_utf16_pos, new_cursor_utf16_pos);
                            let _ = textarea.focus();
                        }
                    }
                }
            }
        }
    });
}
/// Convert UTF-8 byte index (from Rust string) to UTF-16 code unit index (for DOM)
#[allow(dead_code)]
fn utf8_to_utf16_index(text: &str, utf8_index: usize) -> usize {
    let utf8_index = utf8_index.min(text.len());
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
/// Get cursor position from textarea (synchronous, web-only)
#[allow(unused_variables)]
fn get_cursor_position(textarea_id: &str, current_text: &str) -> Option<usize> {
    #[cfg(feature = "web")]
    {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(element) = document.get_element_by_id(textarea_id) {
                    if let Ok(textarea) = element.dyn_into::<web_sys::HtmlTextAreaElement>() {
                        return textarea
                            .selection_start()
                            .ok()
                            .flatten()
                            .map(|pos| pos as usize);
                    }
                }
            }
        }
        None
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = textarea_id;
        Some(current_text.chars().map(|c| c.len_utf16()).sum())
    }
}

/// Get cursor position from textarea via async JS eval (non-web WebView)
#[cfg(not(feature = "web"))]
async fn get_cursor_position_eval(textarea_id: &str) -> Option<usize> {
    let script = format!(
        "return document.getElementById('{}')?.selectionStart ?? -1",
        textarea_id
    );
    let result = document::eval(&script).await;
    result
        .ok()
        .and_then(|v| v.as_f64())
        .filter(|&v| v >= 0.0)
        .map(|v| v as usize)
}
/// Update dropdown position based on cursor
#[cfg(feature = "web")]
#[allow(unused_variables)]
fn update_dropdown_position(
    textarea_id: &str,
    dropdown_top: &mut Signal<f64>,
    dropdown_left: &mut Signal<f64>,
    show_below: &mut Signal<bool>,
    is_mobile: &mut Signal<bool>,
) {
    let has_cursor_position = get_cursor_position(textarea_id, "").is_some();
    #[cfg(feature = "web")]
    {
        if !has_cursor_position {
            return;
        }
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(element) = document.get_element_by_id(textarea_id) {
                    let rect = element.get_bounding_client_rect();
                    let viewport_width = window
                        .inner_width()
                        .ok()
                        .and_then(|w| w.as_f64())
                        .unwrap_or(1024.0);
                    let viewport_height = window
                        .inner_height()
                        .ok()
                        .and_then(|h| h.as_f64())
                        .unwrap_or(600.0);
                    let is_mobile_view = viewport_width < 640.0;
                    is_mobile.set(is_mobile_view);
                    let bottom_space = viewport_height - rect.bottom();
                    let top_space = rect.top();
                    let dropdown_height = if is_mobile_view { 200.0 } else { 300.0 };
                    if bottom_space >= dropdown_height {
                        show_below.set(true);
                        dropdown_top.set(rect.bottom() + window.scroll_y().unwrap_or(0.0));
                    } else if top_space >= dropdown_height {
                        show_below.set(false);
                        dropdown_top
                            .set(rect.top() + window.scroll_y().unwrap_or(0.0) - dropdown_height);
                    } else {
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
#[allow(clippy::too_many_arguments)]
fn render_dropdown(
    results: &[ProfileSearchResult],
    selected_index: usize,
    is_searching: bool,
    top: f64,
    left: f64,
    _show_below: bool,
    is_mobile: bool,
    content: Signal<String>,
    on_input: EventHandler<String>,
    mention_start_pos: usize,
    query_len: usize,
    textarea_id: String,
    show_autocomplete: Signal<bool>,
    external_cursor_position: Option<Signal<usize>>,
) -> Element {
    let textarea_id_rc = Rc::new(textarea_id);
    let result_count = results.len();
    rsx! {
        div {
            role: "listbox",
            aria_label: "Profile suggestions",
            class: if cfg!(feature = "web") {
                "fixed bg-white dark:bg-gray-800 shadow-lg rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden z-50"
            } else {
                "absolute top-full left-0 right-0 mt-1 bg-white dark:bg-gray-800 shadow-lg rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden z-50"
            },
            class: if cfg!(feature = "web") {
                "w-[calc(100vw-2rem)] sm:w-[300px] max-h-[50vh] sm:max-h-[300px]"
            } else {
                "w-full max-h-[50vh]"
            },
            style: if cfg!(feature = "web") {
                if is_mobile { format!("top: {}px; left: 1rem; right: 1rem;", top) } else { format!("top: {}px; left: {}px;", top, left) }
            } else {
                String::new()
            },
            if !is_searching && !results.is_empty() {
                {
                    let plural = if result_count == 1 { "" } else { "s" };
                    rsx! {
                        div { class: "px-3 py-1.5 text-xs text-gray-500 dark:text-gray-400 bg-gray-50 dark:bg-gray-750 border-b border-gray-200 dark:border-gray-700",
                            "{result_count} profile{plural} found"
                        }
                    }
                }
            }
            if is_searching {
                div { class: "px-4 py-3 text-sm text-gray-500 dark:text-gray-400", "Searching..." }
            } else if results.is_empty() {
                div { class: "px-4 py-3 text-sm text-gray-500 dark:text-gray-400", "No profiles found" }
            } else {
                div { class: "overflow-y-auto max-h-[calc(50vh-6rem)] sm:max-h-[240px]",
                    for (index , profile) in results.iter().enumerate() {
                        {
                            let profile_clone = profile.clone();
                            let is_selected = index == selected_index;
                            let option_id = format!("mention-option-{}", index);
                            rsx! {
                                button {
                                    key: "{profile.pubkey.to_hex()}",
                                    id: "{option_id}",
                                    role: "option",
                                    aria_selected: if is_selected { "true" } else { "false" },
                                    class: if is_selected { "w-full px-4 py-2 flex items-center gap-3 hover:bg-blue-50 dark:hover:bg-blue-900 bg-blue-50 dark:bg-blue-900 cursor-pointer transition" } else { "w-full px-4 py-2 flex items-center gap-3 hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer transition" },
                                    onclick: {
                                        let textarea_id_clone = textarea_id_rc.clone();
                                        move |_| {
                                            insert_mention(
                                                profile_clone.clone(),
                                                content,
                                                on_input,
                                                mention_start_pos,
                                                query_len,
                                                (*textarea_id_clone).clone(),
                                                show_autocomplete,
                                                external_cursor_position,
                                            );
                                        }
                                    },
                                    div { class: "shrink-0",
                                        if let Some(picture) = &profile.picture {
                                            if is_valid_http_url(picture) {
                                                img {
                                                    src: "{picture}",
                                                    class: "w-8 h-8 rounded-full",
                                                    alt: "{profile.get_display_name()}",
                                                    loading: "lazy",
                                                }
                                            } else {
                                                div { class: "w-8 h-8 rounded-full bg-gray-300 dark:bg-gray-600 flex items-center justify-center text-xs font-bold",
                                                    {profile.get_display_name().chars().next().unwrap_or('?').to_string()}
                                                }
                                            }
                                        } else {
                                            div { class: "w-8 h-8 rounded-full bg-gray-300 dark:bg-gray-600 flex items-center justify-center text-xs font-bold",
                                                {profile.get_display_name().chars().next().unwrap_or('?').to_string()}
                                            }
                                        }
                                    }
                                    div { class: "flex-1 text-left min-w-0",
                                        div { class: "font-semibold text-sm text-gray-900 dark:text-gray-100 truncate",
                                            {profile.get_display_name()}
                                        }
                                        if let Some(username) = profile.get_username() {
                                            div { class: "text-xs text-gray-500 dark:text-gray-400 truncate", "@{username}" }
                                        }
                                    }
                                    if profile.is_thread_participant {
                                        div { class: "shrink-0 text-xs px-2 py-1 bg-purple-100 dark:bg-purple-900 text-purple-700 dark:text-purple-300 rounded-full",
                                            "Thread"
                                        }
                                    } else if profile.is_contact {
                                        div { class: "shrink-0 text-xs px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded-full",
                                            "Contact"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "px-3 py-1.5 text-xs text-gray-400 dark:text-gray-500 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-750",
                    "↑↓ navigate • Enter select • Esc close"
                }
            }
        }
    }
}
