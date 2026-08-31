use crate::hooks::use_profile_typeahead::{
    use_profile_typeahead, TypeaheadOptions,
};
use crate::platform::editor_dom;
use crate::services::profile_search::ProfileSearchResult;
use crate::stores::ui::mention_mru;
use crate::utils::is_valid_http_url;
use crate::utils::mention_ranges::{build_pretty_insert, MentionRange};
use crate::utils::text::utf16_to_utf8_index;
use dioxus::prelude::Event as DioxusEvent;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::rc::Rc;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[derive(Props, Clone, PartialEq)]
pub struct MentionAutocompleteProps {
    /// Current content of the textarea (mirror only — the DOM is authoritative)
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
    /// Optional externally-owned stable textarea id (e.g. from `UseComposerEditor`),
    /// so the owner can perform imperative DOM mutations on the same element.
    #[props(optional)]
    pub textarea_id: Option<Signal<Rc<String>>>,
    /// When provided, mentions insert as pretty `@Name` labels tracked in this
    /// range list (materialized to `nostr:nprofile1…` at publish by the
    /// composer hook). When absent, raw `nostr:nprofile1…` text is inserted.
    #[props(optional)]
    pub mention_ranges: Option<Signal<Vec<MentionRange>>>,
}
#[component]
pub fn MentionAutocomplete(props: MentionAutocompleteProps) -> Element {
    // Dropdown state. The search pipeline itself (local cache scan, debounced
    // relay streaming, NIP-05 / identifier short-circuits) lives in
    // `use_profile_typeahead`.
    let mut show = use_signal(|| false);
    let mut query = use_signal(String::new);
    let mut start_pos = use_signal(|| 0usize);
    let mut selected_index = use_signal(|| 0usize);
    // IME composition state: while composing (Android autocorrect, CJK input),
    // the DOM text is intermediate — never treat Enter/arrows as dropdown
    // navigation/selection and never write the textarea value mid-composition.
    let mut composing = use_signal(|| false);
    let participants = use_signal(|| props.thread_participants.clone());
    let typeahead = use_profile_typeahead(query, show, participants, TypeaheadOptions::default());
    #[cfg_attr(not(feature = "web"), allow(unused_mut))]
    let mut dropdown_top = use_signal(|| 0.0);
    #[cfg_attr(not(feature = "web"), allow(unused_mut))]
    let mut dropdown_left = use_signal(|| 0.0);
    #[cfg_attr(not(feature = "web"), allow(unused_mut))]
    let mut show_below = use_signal(|| true);
    #[allow(unused_mut)]
    let mut is_mobile = use_signal(|| false);
    let internal_textarea_id =
        use_signal(|| Rc::new(format!("mention-textarea-{}", uuid::Uuid::new_v4())));
    let textarea_id = props.textarea_id.unwrap_or(internal_textarea_id);

    let handle_input = move |evt: DioxusEvent<FormData>| {
        let new_value = evt.value().clone();
        props.on_input.call(new_value.clone());

        #[cfg(feature = "web")]
        {
            let Some(cursor_pos) = get_cursor_position(&textarea_id.read(), &new_value) else {
                show.set(false);
                return;
            };
            if let Some(mut signal) = props.cursor_position {
                let cursor_utf8 = utf16_to_utf8_index(&new_value, cursor_pos);
                signal.set(cursor_utf8);
            }
            detect_mention(&new_value, cursor_pos, &mut show, &mut query, &mut start_pos, &mut selected_index);
            if *show.read() {
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
            let cursor_signal = props.cursor_position;
            spawn(async move {
                let cursor_pos = get_cursor_position_eval(&textarea_id_str)
                    .await
                    .unwrap_or_else(|| new_value.chars().map(|c| c.len_utf16()).sum());
                if let Some(mut signal) = cursor_signal {
                    signal.set(utf16_to_utf8_index(&new_value, cursor_pos));
                }
                detect_mention(&new_value, cursor_pos, &mut show, &mut query, &mut start_pos, &mut selected_index);
            });
        }
    };
    let results = typeahead.results();
    let is_searching = typeahead.is_searching();
    let typeahead_for_keys = typeahead;
    let handle_keydown = move |evt: DioxusEvent<KeyboardData>| {
        // Never intercept keys while the IME is composing — Enter commits a
        // candidate, arrows move the composition cursor.
        if evt.is_composing() || *composing.read() {
            return;
        }
        if !*show.read() {
            return;
        }
        let key = evt.key();
        let results = typeahead_for_keys.results();
        match key {
            Key::ArrowDown => {
                evt.prevent_default();
                let current = *selected_index.read();
                let max = results.len().saturating_sub(1);
                if current < max {
                    let new_index = current + 1;
                    selected_index.set(new_index);
                    let _ = document::eval(&format!(
                        r#"document.getElementById('mention-option-{}')?.scrollIntoView({{ block: 'nearest', behavior: 'smooth' }})"#,
                        new_index,
                    ));
                }
            }
            Key::ArrowUp => {
                evt.prevent_default();
                let current = *selected_index.read();
                if current > 0 {
                    let new_index = current - 1;
                    selected_index.set(new_index);
                    let _ = document::eval(&format!(
                        r#"document.getElementById('mention-option-{}')?.scrollIntoView({{ block: 'nearest', behavior: 'smooth' }})"#,
                        new_index,
                    ));
                }
            }
            Key::Enter if !results.is_empty() => {
                evt.prevent_default();
                if let Some(profile) = results.get(*selected_index.read()) {
                    insert_mention(
                        profile.clone(),
                        props.content,
                        props.on_input,
                        *start_pos.read(),
                        query.read().len(),
                        (**textarea_id.read()).clone(),
                        show,
                        props.cursor_position,
                        props.mention_ranges,
                    );
                }
            }
            Key::Escape => {
                show.set(false);
            }
            _ => {}
        }
    };
    let handle_focus = move |_| {
        if let Some(handler) = &props.onfocus {
            handler.call(());
        }
    };
    // Blur-dismiss: option buttons suppress mousedown default so tapping an
    // option keeps focus in the textarea; any other blur means the user moved
    // on — dismiss the dropdown.
    let handle_blur = move |_| {
        show.set(false);
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
                // Uncontrolled: the DOM owns the text; `initial_value` seeds it at
                // mount (and mirrors the content on remount). Programmatic changes
                // write the DOM imperatively via `platform::editor_dom`, which keeps
                // the caret intact — a controlled `value:` binding would reset the
                // caret to the end whenever a stale render diff lands.
                r#initial_value: "{props.content}",
                disabled: props.disabled,
                oninput: handle_input,
                onkeydown: handle_keydown,
                onkeyup: handle_keyup,
                onclick: handle_click,
                onfocus: handle_focus,
                onblur: handle_blur,
                oncompositionstart: move |_| {
                    composing.set(true);
                },
                oncompositionend: move |_| {
                    // The input event that follows carries the final value; the
                    // mirror syncs through `handle_input`.
                    composing.set(false);
                },
            }
            if *show.read() {
                {
                    render_dropdown(
                        &results,
                        *selected_index.read(),
                        is_searching,
                        *dropdown_top.read(),
                        *dropdown_left.read(),
                        *show_below.read(),
                        *is_mobile.read(),
                        props.content,
                        props.on_input,
                        *start_pos.read(),
                        query.read().len(),
                        (**textarea_id.read()).clone(),
                        show,
                        props.cursor_position,
                        props.mention_ranges,
                    )
                }
            }
        }
    }
}
/// Detect @ mention in text at cursor position. Pure state extraction — the
/// actual search cascade lives in `use_profile_typeahead` (keyed on `query`).
fn detect_mention(
    text: &str,
    cursor_pos: usize,
    show: &mut Signal<bool>,
    query: &mut Signal<String>,
    start_pos: &mut Signal<usize>,
    selected_index: &mut Signal<usize>,
) {
    let cursor_byte_index = utf16_to_utf8_index(text, cursor_pos);
    let before_cursor = &text[..cursor_byte_index];
    if let Some(at_pos) = before_cursor.rfind('@') {
        let after_at = &before_cursor[at_pos + 1..];
        if after_at.contains(char::is_whitespace) {
            show.set(false);
            return;
        }
        query.set(after_at.to_string());
        start_pos.set(at_pos);
        show.set(true);
        selected_index.set(0);
    } else {
        show.set(false);
    }
}
/// Collect nprofile relay hints for a pubkey: outbox coverage map ∪
/// session-learned nostr.json hints, capped at 3 relays.
fn mention_relay_hints(pubkey: &PublicKey) -> Vec<nostr_sdk::RelayUrl> {
    let hex = pubkey.to_hex();
    let mut urls: Vec<String> =
        crate::stores::relay::coverage::get_known_user_relays(&hex).unwrap_or_default();
    for hint in mention_mru::get_hints(&hex) {
        if !urls.contains(&hint) {
            urls.push(hint);
        }
    }
    urls.truncate(3);
    urls.into_iter()
        .filter_map(|u| nostr_sdk::RelayUrl::parse(&u).ok())
        .collect()
}

/// Insert a mention into the textarea
#[allow(clippy::too_many_arguments)]
fn insert_mention(
    profile: ProfileSearchResult,
    content: Signal<String>,
    on_input: EventHandler<String>,
    mention_start_pos: usize,
    query_len: usize,
    textarea_id: String,
    mut show_autocomplete: Signal<bool>,
    external_cursor_position: Option<Signal<usize>>,
    mention_ranges: Option<Signal<Vec<MentionRange>>>,
) {
    spawn(async move {
        mention_mru::record_mention(&profile.pubkey);
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

        let (new_content, new_cursor_byte_pos, range) = if mention_ranges.is_some() {
            // Pretty path: `@Label ` in the editor text, range recorded for
            // materialization at publish.
            let hints = mention_relay_hints(&profile.pubkey);
            let (new_content, caret, range) = build_pretty_insert(
                &current_content,
                mention_start_pos,
                query_len,
                profile.pubkey,
                &profile.get_display_name(),
                hints,
            );
            (new_content, caret, Some(range))
        } else {
            // Raw path: canonical nostr:nprofile URI directly in the text.
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
            let caret = before.len() + mention.len() + 1;
            (new_content, caret, None)
        };

        on_input.call(new_content.clone());
        if let Some(mut ranges) = mention_ranges {
            if let Some(range) = range {
                ranges.write().push(range);
            }
        }
        show_autocomplete.set(false);
        if let Some(mut signal) = external_cursor_position {
            signal.set(new_cursor_byte_pos);
        }
        // Write the DOM value + caret imperatively on every platform — the
        // textarea is uncontrolled, so this is the only write path.
        editor_dom::write_value_and_caret(&textarea_id, &new_content, new_cursor_byte_pos).await;
    });
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
        editor_dom::js_string_literal(textarea_id),
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
                    // The dropdown is `position: fixed`, so its top/left are relative to the
                    // viewport. `get_bounding_client_rect()` already returns viewport-relative
                    // coordinates, so we must NOT add `window.scroll_y()`/`scroll_x()` here —
                    // that double-counts the scroll offset and shoves the dropdown far below the
                    // viewport when the body is scrolled (e.g. the reply/comment modal opened
                    // after scrolling a feed).
                    if bottom_space >= dropdown_height {
                        show_below.set(true);
                        dropdown_top.set(rect.bottom());
                    } else if top_space >= dropdown_height {
                        show_below.set(false);
                        dropdown_top.set(rect.top() - dropdown_height);
                    } else {
                        show_below.set(true);
                        dropdown_top.set(rect.bottom());
                    }
                    dropdown_left.set(rect.left());
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
    mention_ranges: Option<Signal<Vec<MentionRange>>>,
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
            if is_searching && results.is_empty() {
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
                                    // Prevent the mousedown from stealing focus so the
                                    // textarea blur-dismiss never races the click.
                                    onmousedown: move |e: MouseEvent| {
                                        e.prevent_default();
                                    },
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
                                                mention_ranges,
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
