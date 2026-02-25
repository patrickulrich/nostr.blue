//! Label Picker Component
//!
//! Reusable label selection component with color-coded chips.
//! Pre-populates from existing labels and allows adding new labels inline.
use dioxus::prelude::*;

/// Default label suggestions for new repositories
const DEFAULT_LABELS: &[&str] = &[
    "bug",
    "enhancement",
    "documentation",
    "good first issue",
    "help wanted",
    "question",
    "wontfix",
    "duplicate",
];

/// Get a deterministic color class for a label based on its name.
/// Uses the project's semantic theme tokens instead of hardcoded colors.
fn label_color(label: &str) -> &'static str {
    let hash: u32 = label.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    match hash % 8 {
        0 => "bg-muted text-foreground border-border",
        1 => "bg-accent text-foreground border-border",
        2 => "bg-muted text-muted-foreground border-border",
        3 => "bg-accent text-foreground border-border",
        4 => "bg-muted text-foreground border-border",
        5 => "bg-accent text-muted-foreground border-border",
        6 => "bg-muted text-foreground border-border",
        _ => "bg-background text-foreground border border-border",
    }
}

/// Label picker component with inline adding
#[component]
pub fn LabelPicker(
    selected_labels: Vec<String>,
    on_change: EventHandler<Vec<String>>,
    #[props(default = vec![])]
    existing_labels: Vec<String>,
) -> Element {
    let mut new_label_input = use_signal(String::new);
    let mut show_suggestions = use_signal(|| false);
    let mut focus_gen = use_signal(|| 0u32);

    // Build suggestion list: existing labels + defaults, excluding already selected
    let suggestions: Vec<String> = {
        let mut all: Vec<String> = existing_labels.clone();
        for default in DEFAULT_LABELS {
            let s = default.to_string();
            if !all.contains(&s) {
                all.push(s);
            }
        }
        all.retain(|l| !selected_labels.contains(l));
        all.sort();
        all.dedup();
        all
    };

    let input_val = new_label_input.read().clone();
    let filtered_suggestions: Vec<String> = if input_val.is_empty() {
        suggestions.clone()
    } else {
        let q = input_val.to_lowercase();
        suggestions
            .iter()
            .filter(|l| l.to_lowercase().contains(&q))
            .cloned()
            .collect()
    };

    rsx! {
        div { class: "space-y-2",
            // Selected labels as removable chips
            if !selected_labels.is_empty() {
                div { class: "flex flex-wrap gap-1.5",
                    for label in selected_labels.iter() {
                        {
                            let color = label_color(label);
                            let label_clone = label.clone();
                            let labels_for_remove = selected_labels.clone();
                            rsx! {
                                button {
                                    key: "{label}",
                                    r#type: "button",
                                    class: "inline-flex items-center gap-1 px-2 py-0.5 text-xs rounded-full border {color} font-medium hover:opacity-80 transition",
                                    onclick: move |_| {
                                        let mut updated = labels_for_remove.clone();
                                        updated.retain(|l| *l != label_clone);
                                        on_change.call(updated);
                                    },
                                    "{label}"
                                    svg {
                                        class: "w-3 h-3",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        width: "24",
                                        height: "24",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                        line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Input for adding labels
            div { class: "relative",
                input {
                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-none focus:ring-1 focus:ring-primary",
                    r#type: "text",
                    placeholder: "Add a label...",
                    value: "{new_label_input}",
                    oninput: move |e| {
                        new_label_input.set(e.value());
                        show_suggestions.set(true);
                    },
                    onfocusin: move |_| {
                        let next = focus_gen.peek().wrapping_add(1);
                        focus_gen.set(next);
                        show_suggestions.set(true);
                    },
                    onfocusout: move |_| {
                        // Delay hiding to check if focus moved within the component
                        let gen = *focus_gen.peek();
                        spawn(async move {
                            gloo_timers::future::TimeoutFuture::new(200).await;
                            if *focus_gen.peek() == gen {
                                show_suggestions.set(false);
                            }
                        });
                    },
                    onkeypress: {
                        let selected = selected_labels.clone();
                        move |e: KeyboardEvent| {
                            if e.key() == Key::Enter {
                                let val = new_label_input.read().trim().to_string();
                                if !val.is_empty() && !selected.contains(&val) {
                                    let mut updated = selected.clone();
                                    updated.push(val);
                                    on_change.call(updated);
                                    new_label_input.set(String::new());
                                }
                            }
                        }
                    },
                }
                // Suggestions dropdown
                if *show_suggestions.read() && !filtered_suggestions.is_empty() {
                    div { class: "absolute z-30 mt-1 w-full bg-background border border-border rounded-lg shadow-lg max-h-48 overflow-y-auto",
                        for suggestion in filtered_suggestions.iter().take(10) {
                            {
                                let color = label_color(suggestion);
                                let suggestion_clone = suggestion.clone();
                                let labels_for_add = selected_labels.clone();
                                rsx! {
                                    button {
                                        key: "{suggestion}",
                                        r#type: "button",
                                        class: "w-full text-left px-3 py-1.5 text-sm hover:bg-accent transition flex items-center gap-2",
                                        onmousedown: move |_| {
                                            let mut updated = labels_for_add.clone();
                                            if !updated.contains(&suggestion_clone) {
                                                updated.push(suggestion_clone.clone());
                                                on_change.call(updated);
                                            }
                                            new_label_input.set(String::new());
                                        },
                                        span { class: "px-1.5 py-0.5 text-xs rounded-full border {color}", "{suggestion}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            p { class: "text-xs text-muted-foreground", "Press Enter to add a custom label, or select from suggestions" }
        }
    }
}
