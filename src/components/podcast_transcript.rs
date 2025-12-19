//! Podcast Transcript Component
//!
//! Displays synchronized transcripts for podcast episodes with:
//! - Multiple format support (VTT, SRT, JSON, plain text)
//! - Current line highlighting during playback
//! - Click-to-seek functionality
//! - Language selection when multiple transcripts available

use dioxus::prelude::*;
use crate::utils::podcast::TranscriptRef;
use crate::services::podcast_rss::{fetch_transcript, format_duration};
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
                    on_seek: props.on_seek.clone(),
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

    // Fetch transcript content
    let content = use_resource(move || {
        let url = transcript_url.clone();
        async move { fetch_transcript(&TranscriptRef {
            url,
            transcript_type: "text/plain".to_string(),
            language: None,
            rel: None,
        }).await }
    });

    let content_read = content.read();
    match &*content_read {
        Some(Ok(text)) => {
            // Parse based on type
            let cues = parse_transcript(text, &transcript_type);
            drop(content_read);
            rsx! {
                TranscriptView {
                    cues: cues,
                    current_time: props.current_time,
                    on_seek: props.on_seek.clone(),
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
// Transcript View
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
    // Find current cue index
    let current_idx = find_current_cue(&props.cues, props.current_time);

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
            class: "{max_height} overflow-y-auto space-y-1 pr-2 scrollbar-thin scrollbar-thumb-muted",

            for (idx, cue) in props.cues.iter().enumerate() {
                TranscriptCueItem {
                    key: "{idx}",
                    cue: cue.clone(),
                    is_current: Some(idx) == current_idx,
                    on_click: props.on_seek.clone(),
                    compact: props.compact
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
}

#[component]
fn TranscriptCueItem(props: TranscriptCueItemProps) -> Element {
    let cue = &props.cue;

    let handle_click = {
        let start_time = cue.start_time;
        let on_click = props.on_click.clone();
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
            onclick: handle_click,

            // Timestamp
            span {
                class: "text-xs text-muted-foreground font-mono flex-shrink-0 w-12",
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

                // Text content
                span {
                    class: if props.is_current { "text-sm text-foreground" } else { "text-sm text-muted-foreground" },
                    "{cue.text}"
                }
            }

            // Current indicator
            if props.is_current {
                div {
                    class: "text-primary flex-shrink-0",
                    dangerous_inner_html: icons::VOLUME_2
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
                    div { class: "w-12 h-4 bg-muted rounded flex-shrink-0" }
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
    parse_vtt(&content.replace(',', "."))
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
        // Only treat as speaker if it's relatively short (name-like)
        if potential_speaker.len() < 30 && !potential_speaker.contains(' ') {
            return (
                Some(potential_speaker.to_string()),
                text[colon_pos + 2..].to_string(),
            );
        }
    }

    (None, text.to_string())
}
