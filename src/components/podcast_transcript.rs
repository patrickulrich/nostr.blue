//! Podcast Transcript Component
//!
//! Displays synchronized transcripts for podcast episodes with:
//! - Multiple format support (VTT, SRT, JSON, plain text)
//! - Current line highlighting during playback
//! - Click-to-seek functionality
//! - Language selection when multiple transcripts available
//! - Auto-scroll to current cue (podverse-inspired)
//! - Search/filter functionality

use dioxus::prelude::*;
use crate::utils::podcast::TranscriptRef;
use crate::services::podcast_index::fetch_transcript_proxied;
use crate::services::podcast_rss::format_duration;
use crate::stores::music_player;
use crate::components::icons;

// ============================================================================
// Transcript Types
// ============================================================================

/// A single transcript cue/line
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptCue {
    /// Start time in seconds
    pub start_time: f64,
    /// End time in seconds
    pub end_time: f64,
    /// The text content
    pub text: String,
    /// Speaker name (if available)
    pub speaker: Option<String>,
}

// ============================================================================
// Main Transcript Component
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct PodcastTranscriptProps {
    /// Available transcripts
    pub transcripts: Vec<TranscriptRef>,
    /// Current playback position in seconds
    #[props(default = 0.0)]
    pub current_time: f64,
    /// Callback when a cue is clicked (seeks to that time)
    #[props(default)]
    pub on_seek: Option<EventHandler<f64>>,
    /// Show in compact mode
    #[props(default = false)]
    pub compact: bool,
}

/// Podcast transcript viewer with synchronized highlighting
#[component]
pub fn PodcastTranscript(props: PodcastTranscriptProps) -> Element {
    let mut selected_lang = use_signal(|| 0usize);

    // No transcripts available
    if props.transcripts.is_empty() {
        return rsx! {
            div {
                class: "text-center py-4 text-muted-foreground text-sm",
                "No transcripts available for this episode."
            }
        };
    }

    let transcripts = props.transcripts.clone();
    let has_multiple = transcripts.len() > 1;

    rsx! {
        div {
            class: "space-y-3",

            // Language selector if multiple transcripts
            if has_multiple {
                div {
                    class: "flex items-center gap-2 pb-2 border-b border-border",
                    span {
                        class: "text-sm text-muted-foreground",
                        "Language:"
                    }
                    for (idx, transcript) in transcripts.iter().enumerate() {
                        {
                            let lang = transcript.language.clone().unwrap_or_else(|| "Default".to_string());
                            let is_selected = *selected_lang.read() == idx;
                            let class = if is_selected {
                                "px-2 py-1 text-xs font-medium rounded-full bg-primary text-primary-foreground"
                            } else {
                                "px-2 py-1 text-xs font-medium rounded-full bg-muted hover:bg-muted/80 cursor-pointer"
                            };
                            rsx! {
                                button {
                                    key: "{idx}",
                                    class: "{class}",
                                    onclick: move |_| selected_lang.set(idx),
                                    "{lang}"
                                }
                            }
                        }
                    }
                }
            }

            // Transcript content
            if let Some(transcript) = transcripts.get(*selected_lang.read()) {
                TranscriptContent {
                    transcript: transcript.clone(),
                    current_time: props.current_time,
                    on_seek: props.on_seek,
                    compact: props.compact
                }
            }
        }
    }
}

// ============================================================================
// Transcript Content Loader
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct TranscriptContentProps {
    transcript: TranscriptRef,
    #[props(default = 0.0)]
    current_time: f64,
    #[props(default)]
    on_seek: Option<EventHandler<f64>>,
    #[props(default = false)]
    compact: bool,
}

#[component]
fn TranscriptContent(props: TranscriptContentProps) -> Element {
    let transcript_url = props.transcript.url.clone();
    let transcript_type = props.transcript.transcript_type.clone();
    let transcript_type_for_parse = transcript_type.clone();

    // Fetch transcript content through proxy to avoid CORS issues
    let content = use_resource(move || {
        let url = transcript_url.clone();
        let _ttype = transcript_type.clone(); // Keep for potential future use
        async move { fetch_transcript_proxied(&url).await }
    });

    let content_read = content.read();
    match &*content_read {
        Some(Ok(text)) => {
            // Parse based on type
            let cues = parse_transcript(text, &transcript_type_for_parse);
            drop(content_read);
            rsx! {
                TranscriptView {
                    cues: cues,
                    current_time: props.current_time,
                    on_seek: props.on_seek,
                    compact: props.compact
                }
            }
        }
        Some(Err(e)) => {
            let err = e.clone();
            drop(content_read);
            rsx! {
                div {
                    class: "text-center py-4 text-destructive text-sm",
                    "Failed to load transcript: {err}"
                }
            }
        }
        None => {
            drop(content_read);
            rsx! {
                TranscriptSkeleton {}
            }
        }
    }
}

// ============================================================================
// Transcript View with Auto-scroll and Search
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct TranscriptViewProps {
    cues: Vec<TranscriptCue>,
    #[props(default = 0.0)]
    current_time: f64,
    #[props(default)]
    on_seek: Option<EventHandler<f64>>,
    #[props(default = false)]
    compact: bool,
}

#[component]
fn TranscriptView(props: TranscriptViewProps) -> Element {
    // Auto-scroll state
    let mut auto_scroll_enabled = use_signal(|| true);

    // Search state
    let mut search_query = use_signal(String::new);
    let mut show_search = use_signal(|| false);

    // Track previous current_idx to detect changes - use a non-reactive cell to avoid loops
    let prev_idx = use_hook(|| std::cell::RefCell::new(None::<usize>));

    // Signal to track current cue index (effect subscribes to this for reactivity)
    let mut current_idx_signal = use_signal(|| None::<usize>);

    // Find current cue index and update signal if changed
    let current_idx = find_current_cue(&props.cues, props.current_time);
    if current_idx != *current_idx_signal.peek() {
        current_idx_signal.set(current_idx);
    }

    // Compute filtered indices directly during render
    // (reads current props.cues and search_query - updates when either changes)
    let search_text = search_query.read().to_lowercase();
    let filtered_indices: Vec<usize> = if search_text.is_empty() {
        (0..props.cues.len()).collect()
    } else {
        props.cues.iter().enumerate()
            .filter(|(_, cue)| {
                cue.text.to_lowercase().contains(&search_text) ||
                cue.speaker.as_ref().map(|s| s.to_lowercase().contains(&search_text)).unwrap_or(false)
            })
            .map(|(idx, _)| idx)
            .collect()
    };

    let match_count = if search_text.is_empty() { 0 } else { filtered_indices.len() };

    // Auto-scroll effect - scroll current cue into view
    // Subscribes to both auto_scroll_enabled and current_idx_signal
    use_effect(move || {
        let auto_scroll = *auto_scroll_enabled.read();
        let current = *current_idx_signal.read(); // Subscribe to current_idx changes

        // Get previous without reactive subscription (use peek to avoid extra subscription)
        let previous = *prev_idx.borrow();

        // Only scroll if auto-scroll is enabled and index changed
        if auto_scroll && current != previous {
            // Update previous index (non-reactive)
            *prev_idx.borrow_mut() = current;

            if let Some(idx) = current {
                // Use JavaScript to scroll the element into view
                // Note: document::eval returns Eval (async), errors handled internally by Dioxus
                let _ = document::eval(&format!(
                    r#"
                    (function() {{
                        const el = document.querySelector('[data-cue-index="{}"]');
                        if (el) {{
                            el.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
                        }}
                    }})();
                    "#,
                    idx
                ));
            }
        }
    });

    if props.cues.is_empty() {
        return rsx! {
            div {
                class: "text-center py-4 text-muted-foreground text-sm",
                "Transcript is empty."
            }
        };
    }

    let max_height = if props.compact { "max-h-48" } else { "max-h-96" };

    rsx! {
        div {
            class: "space-y-2",

            // Controls bar
            div {
                class: "flex items-center justify-between gap-2",

                // Search toggle and input
                div {
                    class: "flex items-center gap-2 flex-1",

                    // Search toggle button
                    button {
                        class: if *show_search.read() {
                            "p-1.5 rounded-lg bg-primary/10 text-primary hover:bg-primary/20 transition"
                        } else {
                            "p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition"
                        },
                        title: "Toggle search",
                        onclick: move |_| {
                            let new_state = !*show_search.read();
                            show_search.set(new_state);
                            if !new_state {
                                search_query.set(String::new());
                            }
                        },
                        dangerous_inner_html: icons::SEARCH
                    }

                    // Search input (shown when search is toggled)
                    if *show_search.read() {
                        div {
                            class: "flex-1 flex items-center gap-2",
                            input {
                                class: "flex-1 px-2 py-1 text-sm bg-muted rounded-lg border border-border focus:outline-hidden focus:ring-1 focus:ring-primary",
                                r#type: "text",
                                placeholder: "Search transcript...",
                                value: "{search_query}",
                                oninput: move |e| search_query.set(e.value()),
                            }
                            if match_count > 0 {
                                span {
                                    class: "text-xs text-muted-foreground whitespace-nowrap",
                                    "{match_count} matches"
                                }
                            }
                            if !search_query.read().is_empty() {
                                button {
                                    class: "p-1 rounded hover:bg-muted text-muted-foreground",
                                    onclick: move |_| search_query.set(String::new()),
                                    svg {
                                        class: "w-3 h-3",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        view_box: "0 0 24 24",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M6 18L18 6M6 6l12 12"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Auto-scroll toggle
                button {
                    class: if *auto_scroll_enabled.read() {
                        "flex items-center gap-1.5 px-2 py-1 rounded-lg text-xs font-medium bg-primary/10 text-primary hover:bg-primary/20 transition"
                    } else {
                        "flex items-center gap-1.5 px-2 py-1 rounded-lg text-xs font-medium bg-muted text-muted-foreground hover:bg-muted/80 transition"
                    },
                    title: if *auto_scroll_enabled.read() { "Auto-scroll enabled" } else { "Auto-scroll disabled" },
                    onclick: move |_| {
                        let current = *auto_scroll_enabled.read();
                        auto_scroll_enabled.set(!current);
                    },

                    // Auto-scroll icon
                    svg {
                        class: "w-3.5 h-3.5",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M19 14l-7 7m0 0l-7-7m7 7V3"
                        }
                    }
                    "Auto"
                }
            }

            // Transcript cues
            div {
                class: "{max_height} overflow-y-auto space-y-1 pr-2 scrollbar-thin scrollbar-thumb-muted",

                if search_text.is_empty() {
                    // Show all cues
                    for (idx, cue) in props.cues.iter().enumerate() {
                        TranscriptCueItem {
                            key: "{idx}",
                            cue: cue.clone(),
                            is_current: Some(idx) == current_idx,
                            on_click: props.on_seek,
                            compact: props.compact,
                            cue_index: idx,
                            highlight_text: None
                        }
                    }
                } else {
                    // Show filtered cues with highlighting
                    // Compute highlight text outside RSX (let bindings not allowed inside RSX)
                    {
                        let highlight = search_query.read().clone();
                        rsx! {
                            if filtered_indices.is_empty() {
                                div {
                                    class: "text-center py-4 text-muted-foreground text-sm",
                                    "No matches found for \"{search_query}\""
                                }
                            } else {
                                for idx in filtered_indices.iter() {
                                    if let Some(cue) = props.cues.get(*idx) {
                                        TranscriptCueItem {
                                            key: "{idx}",
                                            cue: cue.clone(),
                                            is_current: Some(*idx) == current_idx,
                                            on_click: props.on_seek,
                                            compact: props.compact,
                                            cue_index: *idx,
                                            highlight_text: Some(highlight.clone())
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

/// Find the index of the currently playing cue
fn find_current_cue(cues: &[TranscriptCue], current_time: f64) -> Option<usize> {
    for (idx, cue) in cues.iter().enumerate() {
        if current_time >= cue.start_time && current_time < cue.end_time {
            return Some(idx);
        }
    }
    // If not in a cue, find the last cue before current time
    let mut last_idx = None;
    for (idx, cue) in cues.iter().enumerate() {
        if cue.start_time <= current_time {
            last_idx = Some(idx);
        } else {
            break;
        }
    }
    last_idx
}

// ============================================================================
// Single Cue Item
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct TranscriptCueItemProps {
    cue: TranscriptCue,
    #[props(default = false)]
    is_current: bool,
    on_click: Option<EventHandler<f64>>,
    #[props(default = false)]
    compact: bool,
    /// Index for auto-scroll targeting
    cue_index: usize,
    /// Text to highlight in search results
    #[props(default)]
    highlight_text: Option<String>,
}

#[component]
fn TranscriptCueItem(props: TranscriptCueItemProps) -> Element {
    let cue = &props.cue;

    let handle_click = {
        let start_time = cue.start_time;
        let on_click = props.on_click;
        move |_| {
            if let Some(handler) = &on_click {
                handler.call(start_time);
            } else {
                music_player::seek_to(start_time);
            }
        }
    };

    let base_class = if props.is_current {
        "flex gap-2 p-2 rounded-lg bg-primary/10 border-l-2 border-primary cursor-pointer hover:bg-primary/15 transition"
    } else {
        "flex gap-2 p-2 rounded-lg hover:bg-muted/50 cursor-pointer transition"
    };

    let timestamp = format_duration(cue.start_time as u64);

    rsx! {
        div {
            class: "{base_class}",
            "data-cue-index": "{props.cue_index}",
            onclick: handle_click,

            // Timestamp
            span {
                class: "text-xs text-muted-foreground font-mono shrink-0 w-12",
                "{timestamp}"
            }

            // Content
            div {
                class: "flex-1 min-w-0",

                // Speaker name if available
                if let Some(ref speaker) = cue.speaker {
                    span {
                        class: "text-xs font-semibold text-primary mr-2",
                        "{speaker}:"
                    }
                }

                // Text content (with optional highlighting)
                if let Some(ref highlight) = props.highlight_text {
                    HighlightedText {
                        text: cue.text.clone(),
                        highlight: highlight.clone(),
                        is_current: props.is_current
                    }
                } else {
                    span {
                        class: if props.is_current { "text-sm text-foreground" } else { "text-sm text-muted-foreground" },
                        "{cue.text}"
                    }
                }
            }

            // Current indicator
            if props.is_current {
                div {
                    class: "text-primary shrink-0",
                    dangerous_inner_html: icons::VOLUME_2
                }
            }
        }
    }
}

// ============================================================================
// Highlighted Text Component
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct HighlightedTextProps {
    text: String,
    highlight: String,
    is_current: bool,
}

#[component]
fn HighlightedText(props: HighlightedTextProps) -> Element {
    let base_class = if props.is_current { "text-sm text-foreground" } else { "text-sm text-muted-foreground" };

    if props.highlight.is_empty() {
        return rsx! {
            span { class: "{base_class}", "{props.text}" }
        };
    }

    // Create lowercase versions for matching
    let text_lower = props.text.to_lowercase();
    let highlight_lower = props.highlight.to_lowercase();

    // Build two mappings from lowercase byte offsets to original byte offsets.
    // This handles cases where lowercasing changes byte lengths (e.g., 'İ' -> 'i̇').
    // - lower_to_orig_start: maps each lowercase byte to the original character's START
    // - lower_to_orig_end: maps each lowercase byte to the original character's END
    let mut lower_to_orig_start: Vec<usize> = Vec::with_capacity(text_lower.len() + 1);
    let mut lower_to_orig_end: Vec<usize> = Vec::with_capacity(text_lower.len() + 1);
    let orig_char_iter = props.text.char_indices();
    let mut lower_char_iter = text_lower.char_indices().peekable();

    for (orig_byte, orig_char) in orig_char_iter {
        // Get the lowercase version of this original character
        let orig_char_lower: String = orig_char.to_lowercase().collect();
        let orig_char_lower_len = orig_char_lower.chars().count();
        let orig_char_end = orig_byte + orig_char.len_utf8();

        // Map each byte of the lowercase char(s) back to the original byte positions
        for _ in 0..orig_char_lower_len {
            if let Some((_lower_byte, lower_char)) = lower_char_iter.next() {
                // Fill mapping for all bytes in this lowercase character
                let char_byte_len = lower_char.len_utf8();
                for _ in 0..char_byte_len {
                    lower_to_orig_start.push(orig_byte);
                    lower_to_orig_end.push(orig_char_end);
                }
            }
        }
    }
    // Add end sentinels for slicing to end of string
    lower_to_orig_start.push(props.text.len());
    lower_to_orig_end.push(props.text.len());

    // Find matches in lowercase string using byte indices
    let mut parts = Vec::new();
    let mut last_orig_end = 0;

    for (lower_start, matched) in text_lower.match_indices(&highlight_lower) {
        let lower_end = lower_start + matched.len();

        // Map lowercase byte positions to original byte positions
        // Use start map for orig_start, end map for orig_end (at last matched byte)
        let orig_start = lower_to_orig_start.get(lower_start).copied().unwrap_or(0);
        let orig_end = lower_to_orig_end
            .get(lower_end.saturating_sub(1))
            .copied()
            .unwrap_or(props.text.len());

        // Non-matching part before this match
        if orig_start > last_orig_end {
            if let Some(slice) = props.text.get(last_orig_end..orig_start) {
                parts.push((slice.to_string(), false));
            }
        }

        // Matching part (use original text to preserve case)
        if let Some(slice) = props.text.get(orig_start..orig_end) {
            parts.push((slice.to_string(), true));
        }

        last_orig_end = orig_end;
    }

    // Remaining text after last match
    if last_orig_end < props.text.len() {
        if let Some(slice) = props.text.get(last_orig_end..) {
            parts.push((slice.to_string(), false));
        }
    }

    // If no parts were created (no matches), show the whole text
    if parts.is_empty() {
        parts.push((props.text.clone(), false));
    }

    rsx! {
        span {
            class: "{base_class}",
            for (idx, (part, is_match)) in parts.iter().enumerate() {
                if *is_match {
                    mark {
                        key: "{idx}",
                        class: "bg-yellow-300/50 text-foreground rounded px-0.5",
                        "{part}"
                    }
                } else {
                    span {
                        key: "{idx}",
                        "{part}"
                    }
                }
            }
        }
    }
}

// ============================================================================
// Skeleton Loader
// ============================================================================

#[component]
pub fn TranscriptSkeleton() -> Element {
    rsx! {
        div {
            class: "space-y-2 animate-pulse",

            for i in 0..6 {
                div {
                    key: "{i}",
                    class: "flex gap-2 p-2",
                    div { class: "w-12 h-4 bg-muted rounded shrink-0" }
                    div { class: "flex-1 h-4 bg-muted rounded" }
                }
            }
        }
    }
}

// ============================================================================
// Transcript Parsing
// ============================================================================

/// Parse transcript content based on type
fn parse_transcript(content: &str, transcript_type: &str) -> Vec<TranscriptCue> {
    match transcript_type {
        "text/vtt" => parse_vtt(content),
        "application/x-subrip" => parse_srt(content),
        "application/json" => parse_json_transcript(content),
        _ => parse_plain_text(content),
    }
}

/// Parse WebVTT format
fn parse_vtt(content: &str) -> Vec<TranscriptCue> {
    let mut cues = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Look for timestamp line: "00:00:00.000 --> 00:00:05.000"
        if line.contains("-->") {
            let parts: Vec<&str> = line.split("-->").collect();
            if parts.len() >= 2 {
                let start_time = parse_vtt_timestamp(parts[0].trim());
                let end_time = parse_vtt_timestamp(parts[1].split_whitespace().next().unwrap_or(""));

                // Collect text lines until empty line
                let mut text_lines = Vec::new();
                i += 1;
                while i < lines.len() && !lines[i].trim().is_empty() {
                    text_lines.push(lines[i].trim());
                    i += 1;
                }

                let text = text_lines.join(" ");
                if !text.is_empty() {
                    // Check for speaker pattern: "<v Speaker>text</v>" or "Speaker: text"
                    let (speaker, clean_text) = extract_speaker(&text);
                    cues.push(TranscriptCue {
                        start_time,
                        end_time,
                        text: clean_text,
                        speaker,
                    });
                }
            }
        }
        i += 1;
    }

    cues
}

/// Parse VTT timestamp to seconds
fn parse_vtt_timestamp(ts: &str) -> f64 {
    let parts: Vec<&str> = ts.split(':').collect();
    match parts.len() {
        3 => {
            // HH:MM:SS.mmm
            let hours: f64 = parts[0].parse().unwrap_or(0.0);
            let mins: f64 = parts[1].parse().unwrap_or(0.0);
            let secs: f64 = parts[2].replace(',', ".").parse().unwrap_or(0.0);
            hours * 3600.0 + mins * 60.0 + secs
        }
        2 => {
            // MM:SS.mmm
            let mins: f64 = parts[0].parse().unwrap_or(0.0);
            let secs: f64 = parts[1].replace(',', ".").parse().unwrap_or(0.0);
            mins * 60.0 + secs
        }
        _ => 0.0,
    }
}

/// Parse SRT format (similar to VTT)
fn parse_srt(content: &str) -> Vec<TranscriptCue> {
    // SRT is similar to VTT but uses comma instead of period for milliseconds
    // in timestamps (HH:MM:SS,mmm format). The timestamp parser already handles
    // this conversion, so we can pass the content directly without corrupting
    // commas in the actual transcript text.
    parse_vtt(content)
}

/// Parse JSON transcript format
fn parse_json_transcript(content: &str) -> Vec<TranscriptCue> {
    // Try Podcasting 2.0 JSON format
    #[derive(serde::Deserialize)]
    struct JsonSegment {
        #[serde(rename = "startTime")]
        start_time: Option<f64>,
        #[serde(rename = "endTime")]
        end_time: Option<f64>,
        body: Option<String>,
        speaker: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct JsonTranscript {
        segments: Option<Vec<JsonSegment>>,
    }

    if let Ok(transcript) = serde_json::from_str::<JsonTranscript>(content) {
        if let Some(segments) = transcript.segments {
            return segments
                .into_iter()
                .filter_map(|s| {
                    Some(TranscriptCue {
                        start_time: s.start_time?,
                        end_time: s.end_time.unwrap_or(s.start_time? + 5.0),
                        text: s.body?,
                        speaker: s.speaker,
                    })
                })
                .collect();
        }
    }

    Vec::new()
}

/// Parse plain text (one line = one cue, no timing)
fn parse_plain_text(content: &str) -> Vec<TranscriptCue> {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(idx, line)| TranscriptCue {
            start_time: idx as f64 * 5.0, // Rough 5-second intervals
            end_time: (idx + 1) as f64 * 5.0,
            text: line.trim().to_string(),
            speaker: None,
        })
        .collect()
}

/// Extract speaker name from VTT voice tag or "Speaker: " prefix
fn extract_speaker(text: &str) -> (Option<String>, String) {
    // Check for VTT voice tag: <v Speaker>text</v>
    if text.starts_with("<v ") {
        if let Some(end_tag) = text.find('>') {
            let speaker = text[3..end_tag].to_string();
            let rest = &text[end_tag + 1..];
            let clean = rest.replace("</v>", "").trim().to_string();
            return (Some(speaker), clean);
        }
    }

    // Check for "Speaker: " prefix
    if let Some(colon_pos) = text.find(": ") {
        let potential_speaker = &text[..colon_pos];
        // Only treat as speaker if it's name-like:
        // - Relatively short (under 40 chars for multi-word names like "Dr. John Smith")
        // - Has at most 4 words (to avoid treating sentences as speakers)
        // - Doesn't start with common sentence starters
        let word_count = potential_speaker.split_whitespace().count();
        let is_sentence_start = potential_speaker.starts_with("The ")
            || potential_speaker.starts_with("A ")
            || potential_speaker.starts_with("This ")
            || potential_speaker.starts_with("That ")
            || potential_speaker.starts_with("It ");
        if potential_speaker.len() < 40 && word_count <= 4 && !is_sentence_start {
            return (
                Some(potential_speaker.to_string()),
                text[colon_pos + 2..].to_string(),
            );
        }
    }

    (None, text.to_string())
}
