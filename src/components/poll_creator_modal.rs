use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::components::{PollOptionList, PollOptionData};
use crate::utils::generate_option_id;
use nostr_sdk::{nips::nip19::Nip19Event, nips::nip88::{PollType, PollOption}, Timestamp, EventId, ToBech32};

#[component]
pub fn PollCreatorModal(
    show: Signal<bool>,
    on_poll_created: EventHandler<String>,
) -> Element {
    // Form state
    let mut poll_question = use_signal(|| String::new());
    let mut poll_type = use_signal(|| PollType::SingleChoice);
    let mut options = use_signal(|| vec![
        PollOptionData {
            id: generate_option_id(),
            text: String::new(),
        },
        PollOptionData {
            id: generate_option_id(),
            text: String::new(),
        },
    ]);
    let mut end_time_preset = use_signal(|| String::from("1day"));
    let mut custom_end_time = use_signal(|| String::new());
    let mut hashtags_input = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut show_advanced = use_signal(|| false);

    // Validation
    let can_publish = use_memo(move || {
        let question = poll_question.read();
        let opts = options.read();

        !question.trim().is_empty() &&
        opts.len() >= 2 &&
        opts.len() <= 10 &&
        opts.iter().all(|opt| !opt.text.trim().is_empty()) &&
        !*is_publishing.read()
    });

    // Reset form to initial state - used by both close and successful publish
    let mut reset_form = move || {
        poll_question.set(String::new());
        poll_type.set(PollType::SingleChoice);
        options.set(vec![
            PollOptionData {
                id: generate_option_id(),
                text: String::new(),
            },
            PollOptionData {
                id: generate_option_id(),
                text: String::new(),
            },
        ]);
        end_time_preset.set(String::from("1day"));
        custom_end_time.set(String::new());
        hashtags_input.set(String::new());
        error_message.set(None);
        show_advanced.set(false);
        is_publishing.set(false);
    };

    // Handle close - reset all form state and hide modal
    let handle_close = move |_| {
        reset_form();
        show.set(false);
    };

    // Handle options change
    let handle_options_change = move |new_options: Vec<PollOptionData>| {
        options.set(new_options);
    };

    // Handle publishing
    let handle_publish = move |_| {
        if !*can_publish.read() {
            return;
        }

        let question = poll_question.read().clone();
        let poll_type_val = *poll_type.read();
        let options_val = options.read().clone();
        let hashtags_val = hashtags_input.read().clone();
        let end_time_preset_val = end_time_preset.read().clone();
        let custom_end_time_val = custom_end_time.read().clone();

        // Calculate end time once (avoid race condition from computing twice)
        let ends_at = calculate_end_time(&end_time_preset_val, &custom_end_time_val);

        // Validate custom end time before publishing
        if end_time_preset_val == "custom" && ends_at.is_none() {
            error_message.set(Some("Invalid or past end time. Please select a future date/time.".to_string()));
            return;
        }

        is_publishing.set(true);
        error_message.set(None);

        spawn(async move {

            // Convert options to PollOption
            let poll_options: Vec<PollOption> = options_val
                .iter()
                .map(|opt| PollOption {
                    id: opt.id.clone(),
                    text: opt.text.clone(),
                })
                .collect();

            // Parse hashtags
            let hashtags: Vec<String> = extract_hashtags(&question, &hashtags_val);

            // Get user's relays (empty for now)
            let relays = vec![];

            // Publish poll
            match nostr_client::publish_poll(
                question,
                poll_type_val,
                poll_options,
                relays,
                ends_at,
                hashtags,
            ).await {
                Ok(event_id_hex) => {
                    log::info!("Poll published successfully: {}", event_id_hex);

                    // Convert event ID hex to nostr:nevent1... format
                    let nevent_ref = match EventId::from_hex(&event_id_hex) {
                        Ok(eid) => {
                            let nevent = Nip19Event::new(eid);
                            format!(
                                "nostr:{}",
                                nevent.to_bech32().unwrap_or_else(|_| event_id_hex.clone())
                            )
                        },
                        Err(_) => format!("nostr:{}", event_id_hex),
                    };

                    // Call the callback with the nevent reference
                    on_poll_created.call(nevent_ref);

                    // Reset form and close modal
                    reset_form();
                    show.set(false);
                }
                Err(e) => {
                    log::error!("Failed to publish poll: {}", e);
                    error_message.set(Some(format!("Failed to publish: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };

    if !*show.read() {
        return rsx! {};
    }

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-start justify-center overflow-y-auto",
            onclick: handle_close,

            // Modal content
            div {
                class: "bg-background border border-border rounded-lg shadow-xl w-full max-w-lg m-4 mt-20",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between p-4 border-b border-border",
                    h2 {
                        class: "text-xl font-bold flex items-center gap-2",
                        "📊 Create Poll"
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground transition",
                        onclick: handle_close,
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-6 h-6",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M6 18L18 6M6 6l12 12"
                            }
                        }
                    }
                }

                // Content area
                div {
                    class: "p-4 space-y-4 max-h-[60vh] overflow-y-auto",

                    // Error message
                    if let Some(err) = error_message.read().as_ref() {
                        div {
                            class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                            "{err}"
                        }
                    }

                    // Poll Question
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Question"
                        }
                        textarea {
                            class: "w-full px-3 py-2 rounded-lg border border-border bg-background focus:outline-none focus:ring-2 focus:ring-primary resize-none",
                            placeholder: "What's your question?",
                            rows: "2",
                            value: "{poll_question}",
                            oninput: move |evt| {
                                poll_question.set(evt.value());
                            }
                        }
                    }

                    // Options (simple view - show 2-4 initially)
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Options"
                        }
                        PollOptionList {
                            options,
                            on_change: handle_options_change,
                        }
                    }

                    // Duration (simple presets)
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Duration"
                        }
                        div {
                            class: "flex gap-2 flex-wrap",

                            for preset in ["1hour", "1day", "1week"] {
                                button {
                                    key: "{preset}",
                                    class: if *end_time_preset.read() == preset {
                                        "px-3 py-1.5 rounded-lg border-2 border-primary bg-primary/10 text-primary text-sm font-medium"
                                    } else {
                                        "px-3 py-1.5 rounded-lg border border-border hover:border-primary transition text-sm"
                                    },
                                    onclick: move |_| end_time_preset.set(preset.to_string()),
                                    {match preset {
                                        "1hour" => "1 Hour",
                                        "1day" => "1 Day",
                                        "1week" => "1 Week",
                                        _ => preset,
                                    }}
                                }
                            }

                            // Show custom only if advanced is open
                            if *show_advanced.read() {
                                button {
                                    class: if *end_time_preset.read() == "custom" {
                                        "px-3 py-1.5 rounded-lg border-2 border-primary bg-primary/10 text-primary text-sm font-medium"
                                    } else {
                                        "px-3 py-1.5 rounded-lg border border-border hover:border-primary transition text-sm"
                                    },
                                    onclick: move |_| end_time_preset.set("custom".to_string()),
                                    "Custom"
                                }
                            }
                        }

                        // Custom datetime picker
                        if *show_advanced.read() && *end_time_preset.read() == "custom" {
                            div {
                                class: "mt-2",
                                input {
                                    r#type: "datetime-local",
                                    class: "px-3 py-1.5 rounded-lg border border-border bg-background focus:outline-none focus:ring-2 focus:ring-primary text-sm",
                                    value: "{custom_end_time}",
                                    oninput: move |evt| custom_end_time.set(evt.value()),
                                }
                            }
                        }
                    }

                    // More options toggle
                    button {
                        class: "text-sm text-primary hover:underline flex items-center gap-1",
                        onclick: move |_| {
                            let current = *show_advanced.read();
                            show_advanced.set(!current);
                        },
                        if *show_advanced.read() {
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M5 15l7-7 7 7"
                                }
                            }
                            "Less options"
                        } else {
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M19 9l-7 7-7-7"
                                }
                            }
                            "More options"
                        }
                    }

                    // Advanced options (collapsible)
                    if *show_advanced.read() {
                        div {
                            class: "space-y-4 pt-2 border-t border-border",

                            // Poll Type
                            div {
                                label {
                                    class: "block text-sm font-medium mb-2",
                                    "Poll Type"
                                }
                                div {
                                    class: "flex gap-4",

                                    // Single Choice
                                    label {
                                        class: "flex items-center gap-2 cursor-pointer",
                                        input {
                                            r#type: "radio",
                                            name: "poll-type-modal",
                                            class: "w-4 h-4 text-primary",
                                            checked: *poll_type.read() == PollType::SingleChoice,
                                            onchange: move |_| poll_type.set(PollType::SingleChoice),
                                        }
                                        span {
                                            class: "text-sm",
                                            "Single Choice"
                                        }
                                    }

                                    // Multiple Choice
                                    label {
                                        class: "flex items-center gap-2 cursor-pointer",
                                        input {
                                            r#type: "radio",
                                            name: "poll-type-modal",
                                            class: "w-4 h-4 text-primary",
                                            checked: *poll_type.read() == PollType::MultipleChoice,
                                            onchange: move |_| poll_type.set(PollType::MultipleChoice),
                                        }
                                        span {
                                            class: "text-sm",
                                            "Multiple Choice"
                                        }
                                    }
                                }
                            }

                            // Additional Hashtags
                            div {
                                label {
                                    class: "block text-sm font-medium mb-2",
                                    "Additional Hashtags"
                                }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 rounded-lg border border-border bg-background focus:outline-none focus:ring-2 focus:ring-primary text-sm",
                                    placeholder: "bitcoin, nostr (comma separated)",
                                    value: "{hashtags_input}",
                                    oninput: move |evt| hashtags_input.set(evt.value()),
                                }
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "flex items-center justify-end gap-2 p-4 border-t border-border",

                    button {
                        class: "px-4 py-2 text-sm font-medium hover:bg-accent rounded-lg transition",
                        onclick: handle_close,
                        "Cancel"
                    }

                    button {
                        class: if *can_publish.read() {
                            "px-6 py-2 bg-primary text-primary-foreground font-semibold rounded-lg hover:bg-primary/90 transition"
                        } else {
                            "px-6 py-2 bg-muted text-muted-foreground font-semibold rounded-lg cursor-not-allowed"
                        },
                        disabled: !*can_publish.read(),
                        onclick: handle_publish,

                        if *is_publishing.read() {
                            span {
                                class: "flex items-center gap-2",
                                span {
                                    class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"
                                }
                                "Creating..."
                            }
                        } else {
                            "Create Poll"
                        }
                    }
                }
            }
        }
    }
}

/// Calculate end timestamp based on preset or custom time
fn calculate_end_time(preset: &str, custom_time: &str) -> Option<Timestamp> {
    let now = Timestamp::now();

    match preset {
        "1hour" => Some(Timestamp::from(now.as_secs() + 3600)),
        "1day" => Some(Timestamp::from(now.as_secs() + 86400)),
        "1week" => Some(Timestamp::from(now.as_secs() + 604800)),
        "custom" => {
            if custom_time.is_empty() {
                return None;
            }
            // Parse datetime-local format (YYYY-MM-DDTHH:MM)
            use chrono::{NaiveDateTime, Local, TimeZone, Utc};

            if let Ok(naive_dt) = NaiveDateTime::parse_from_str(custom_time, "%Y-%m-%dT%H:%M") {
                // Convert from local time to UTC
                // Use .earliest() for deterministic behavior during DST transitions
                if let Some(local_dt) = Local.from_local_datetime(&naive_dt).earliest() {
                    let utc_dt = local_dt.with_timezone(&Utc);
                    let timestamp = utc_dt.timestamp();

                    // Verify timestamp is valid (non-negative and in the future)
                    if timestamp >= 0 && timestamp > Utc::now().timestamp() {
                        Some(Timestamp::from(timestamp as u64))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => Some(Timestamp::from(now.as_secs() + 86400)), // Default to 1 day
    }
}

/// Extract hashtags from question and additional input
/// - Uses ASCII-only pattern for Nostr compatibility
/// - Limits to max 10 hashtags, max 50 chars each
fn extract_hashtags(question: &str, additional: &str) -> Vec<String> {
    use std::collections::HashSet;
    use once_cell::sync::Lazy;

    // ASCII-only hashtags for Nostr compatibility
    static HASHTAG_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"#([A-Za-z0-9_]+)").expect("Failed to compile hashtag regex")
    });

    let mut hashtags = HashSet::new();

    // Extract from question (format: #hashtag)
    for cap in HASHTAG_REGEX.captures_iter(question) {
        if let Some(tag) = cap.get(1) {
            let tag_str = tag.as_str().to_lowercase();
            // Max 50 chars per tag
            if tag_str.len() <= 50 {
                hashtags.insert(tag_str);
            }
        }
    }

    // Extract from additional input (comma separated, no # required)
    for tag in additional.split(',') {
        let cleaned = tag.trim().trim_start_matches('#').to_lowercase();
        // ASCII alphanumeric + underscore only, max 50 chars
        if !cleaned.is_empty()
            && cleaned.len() <= 50
            && cleaned.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            hashtags.insert(cleaned);
        }
    }

    // Limit to first 10 unique hashtags
    hashtags.into_iter().take(10).collect()
}
