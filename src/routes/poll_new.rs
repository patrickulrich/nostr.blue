use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::components::{PollOptionList, PollOptionData};
use nostr_sdk::{nips::nip88::{PollType, PollOption}, Timestamp};
use once_cell::sync::Lazy;

#[component]
pub fn PollNew() -> Element {
    let navigator = navigator();
    let nav_close = navigator.clone();
    let nav_publish = navigator.clone();
    let nav_effect = navigator.clone();

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

    // Check if user is authenticated
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

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

    // Handle close
    let handle_close = move |_| {
        nav_close.go_back();
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

        is_publishing.set(true);
        error_message.set(None);

        let nav_spawn = nav_publish.clone();
        spawn(async move {
            // Calculate end time
            let ends_at = calculate_end_time(&end_time_preset_val, &custom_end_time_val);

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

            // Get user's relays (for now, empty - could fetch from relay metadata)
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
                Ok(event_id) => {
                    log::info!("Poll published successfully: {}", event_id);
                    is_publishing.set(false);
                    nav_spawn.push(crate::routes::Route::Polls {});
                }
                Err(e) => {
                    log::error!("Failed to publish poll: {}", e);
                    error_message.set(Some(format!("Failed to publish: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };

    // Redirect if not authenticated - hoist use_effect to maintain hook order
    use_effect(move || {
        if !*is_authenticated.read() {
            nav_effect.push(crate::routes::Route::Home {});
        }
    });

    // Conditional UI render
    if !*is_authenticated.read() {
        return rsx! {
            div { class: "flex items-center justify-center h-screen",
                "Redirecting..."
            }
        };
    }

    rsx! {
        div {
            class: "min-h-screen bg-background",

            // Header
            div {
                class: "border-b border-border bg-background sticky top-0 z-10",
                div {
                    class: "max-w-4xl mx-auto px-4 py-4 flex items-center justify-between",

                    div {
                        class: "flex items-center gap-4",
                        button {
                            class: "text-muted-foreground hover:text-foreground transition",
                            onclick: handle_close,
                            svg {
                                class: "w-6 h-6",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M15 19l-7-7 7-7"
                                }
                            }
                        }
                        h1 {
                            class: "text-2xl font-bold",
                            "📊 Create Poll"
                        }
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
                            "Publishing..."
                        } else {
                            "Publish Poll"
                        }
                    }
                }
            }

            // Main content
            div {
                class: "max-w-4xl mx-auto px-4 py-8",

                // Error message
                if let Some(err) = error_message.read().as_ref() {
                    div {
                        class: "mb-4 p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive",
                        "{err}"
                    }
                }

                // Form
                div {
                    class: "space-y-6",

                    // Poll Question
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Poll Question"
                        }
                        textarea {
                            class: "w-full px-4 py-3 rounded-lg border border-border bg-background focus:outline-none focus:ring-2 focus:ring-primary resize-none",
                            placeholder: "What's your question?",
                            rows: "3",
                            value: "{poll_question}",
                            oninput: move |evt| {
                                poll_question.set(evt.value());
                            }
                        }
                        p {
                            class: "mt-1 text-sm text-muted-foreground",
                            "Hashtags in the question will be automatically extracted (e.g., #nostr #bitcoin)"
                        }
                    }

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
                                    name: "poll-type",
                                    class: "w-4 h-4 text-primary",
                                    checked: *poll_type.read() == PollType::SingleChoice,
                                    onchange: move |_| poll_type.set(PollType::SingleChoice),
                                }
                                span {
                                    class: "flex items-center gap-2",
                                    svg {
                                        class: "w-5 h-5",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        circle { cx: "12", cy: "12", r: "10", stroke_width: "2" }
                                        circle { cx: "12", cy: "12", r: "3", fill: "currentColor" }
                                    }
                                    "Single Choice"
                                }
                            }

                            // Multiple Choice
                            label {
                                class: "flex items-center gap-2 cursor-pointer",
                                input {
                                    r#type: "radio",
                                    name: "poll-type",
                                    class: "w-4 h-4 text-primary",
                                    checked: *poll_type.read() == PollType::MultipleChoice,
                                    onchange: move |_| poll_type.set(PollType::MultipleChoice),
                                }
                                span {
                                    class: "flex items-center gap-2",
                                    svg {
                                        class: "w-5 h-5",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        rect { x: "3", y: "3", width: "18", height: "18", rx: "2", stroke_width: "2" }
                                        path { d: "M9 12l2 2 4-4", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
                                    }
                                    "Multiple Choice"
                                }
                            }
                        }
                    }

                    // Options
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Options (2-10)"
                        }
                        PollOptionList {
                            options,
                            on_change: handle_options_change,
                        }
                    }

                    // End Time
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Poll Duration"
                        }
                        div {
                            class: "flex gap-2 flex-wrap",

                            for preset in ["1hour", "1day", "3days", "1week", "custom"] {
                                button {
                                    key: "{preset}",
                                    class: if *end_time_preset.read() == preset {
                                        "px-4 py-2 rounded-lg border-2 border-primary bg-primary/10 text-primary font-medium"
                                    } else {
                                        "px-4 py-2 rounded-lg border border-border hover:border-primary transition"
                                    },
                                    onclick: move |_| end_time_preset.set(preset.to_string()),
                                    {match preset {
                                        "1hour" => "1 Hour",
                                        "1day" => "1 Day",
                                        "3days" => "3 Days",
                                        "1week" => "1 Week",
                                        "custom" => "Custom",
                                        _ => preset,
                                    }}
                                }
                            }
                        }

                        // Custom end time input
                        if *end_time_preset.read() == "custom" {
                            div {
                                class: "mt-3",
                                input {
                                    r#type: "datetime-local",
                                    class: "px-4 py-2 rounded-lg border border-border bg-background focus:outline-none focus:ring-2 focus:ring-primary",
                                    value: "{custom_end_time}",
                                    oninput: move |evt| custom_end_time.set(evt.value()),
                                }
                                p {
                                    class: "mt-1 text-sm text-muted-foreground",
                                    "Set a custom end date and time for this poll"
                                }
                            }
                        }
                    }

                    // Additional Hashtags
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Additional Hashtags (optional)"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 rounded-lg border border-border bg-background focus:outline-none focus:ring-2 focus:ring-primary",
                            placeholder: "bitcoin, nostr, poll (comma separated)",
                            value: "{hashtags_input}",
                            oninput: move |evt| hashtags_input.set(evt.value()),
                        }
                        p {
                            class: "mt-1 text-sm text-muted-foreground",
                            "These will be added in addition to hashtags found in your question"
                        }
                    }

                    // Help text
                    div {
                        class: "p-4 bg-muted/30 rounded-lg",
                        h3 {
                            class: "font-semibold mb-2",
                            "💡 Tips for creating great polls"
                        }
                        ul {
                            class: "space-y-1 text-sm text-muted-foreground list-disc list-inside",
                            li { "Keep your question clear and concise" }
                            li { "Provide balanced, mutually exclusive options for single-choice polls" }
                            li { "Use multiple-choice for polls where users can select more than one option" }
                            li { "Set an appropriate duration based on your audience's timezone and activity" }
                        }
                    }
                }
            }
        }
    }
}

/// Generate a random alphanumeric option ID
fn generate_option_id() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..9)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

/// Calculate end timestamp based on preset or custom time
fn calculate_end_time(preset: &str, custom_time: &str) -> Option<Timestamp> {
    let now = Timestamp::now();

    match preset {
        "1hour" => Some(Timestamp::from(now.as_secs() + 3600)),
        "1day" => Some(Timestamp::from(now.as_secs() + 86400)),
        "3days" => Some(Timestamp::from(now.as_secs() + 259200)),
        "1week" => Some(Timestamp::from(now.as_secs() + 604800)),
        "custom" => {
            if custom_time.is_empty() {
                return None;
            }
            // Parse datetime-local format (YYYY-MM-DDTHH:MM)
            use chrono::{NaiveDateTime, Local, TimeZone, Utc};

            if let Ok(naive_dt) = NaiveDateTime::parse_from_str(custom_time, "%Y-%m-%dT%H:%M") {
                // Convert from local time to UTC
                if let Some(local_dt) = Local.from_local_datetime(&naive_dt).single() {
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

/// Cached compiled regex for hashtag extraction
static HASHTAG_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"#(\w+)").expect("Failed to compile hashtag regex")
});

/// Extract hashtags from question and additional input
fn extract_hashtags(question: &str, additional: &str) -> Vec<String> {
    use std::collections::HashSet;
    let mut hashtags = HashSet::new();

    // Extract from question (format: #hashtag) using cached regex
    for cap in HASHTAG_REGEX.captures_iter(question) {
        if let Some(tag) = cap.get(1) {
            hashtags.insert(tag.as_str().to_lowercase());
        }
    }

    // Extract from additional input (comma separated, no # required)
    for tag in additional.split(',') {
        let cleaned = tag.trim().trim_start_matches('#').to_lowercase();
        if !cleaned.is_empty() {
            hashtags.insert(cleaned);
        }
    }

    hashtags.into_iter().collect()
}
