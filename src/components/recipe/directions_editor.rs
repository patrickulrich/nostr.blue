//! Recipe Directions Editor Component
//! Numbered steps editor for recipe directions
use dioxus::prelude::*;
/// Directions list editor with numbered steps
#[component]
pub fn RecipeDirectionsEditor(
    /// Current directions list
    directions: Signal<Vec<String>>,
    /// Callback when list changes
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let add_step = move |_| {
        let mut current = directions.read().clone();
        current.push(String::new());
        on_change.call(current);
    };
    let remove_step = move |index: usize| {
        let mut current = directions.read().clone();
        if current.len() > 1 {
            current.remove(index);
            on_change.call(current);
        }
    };
    let update_step = move |index: usize, text: String| {
        let mut current = directions.read().clone();
        if let Some(step) = current.get_mut(index) {
            *step = text;
            on_change.call(current);
        }
    };
    let move_up = move |index: usize| {
        if index > 0 {
            let mut current = directions.read().clone();
            current.swap(index, index - 1);
            on_change.call(current);
        }
    };
    let move_down = move |index: usize| {
        let current_len = directions.read().len();
        if index < current_len - 1 {
            let mut current = directions.read().clone();
            current.swap(index, index + 1);
            on_change.call(current);
        }
    };
    let steps = directions.read();
    let can_remove = steps.len() > 1;
    let step_count = steps.len();
    let steps_text = if step_count == 1 { "step" } else { "steps" };
    rsx! {
        div { class: "space-y-3",
            div { class: "flex items-center justify-between mb-2",
                label { class: "text-sm font-medium flex items-center gap-2",
                    span { "Directions" }
                    span { class: "text-xs text-muted-foreground font-normal", "(at least 1 required)" }
                }
                span { class: "text-xs text-muted-foreground", "{step_count} {steps_text}" }
            }
            for (index , step) in steps.iter().enumerate() {
                {
                    let idx = index;
                    let text = step.clone();
                    rsx! {
                        div { key: "{idx}", class: "group flex items-start gap-2",
                            div { class: "flex flex-col gap-0.5 opacity-0 group-hover:opacity-100 transition pt-2",
                                button {
                                    r#type: "button",
                                    class: "p-0.5 rounded hover:bg-muted disabled:opacity-30",
                                    disabled: idx == 0,
                                    onclick: move |_| move_up(idx),
                                    title: "Move up",
                                    svg {
                                        class: "w-3 h-3",
                                        fill: "none",
                                        stroke: "currentColor",
                                        view_box: "0 0 24 24",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            stroke_width: "2",
                                            d: "M5 15l7-7 7 7",
                                        }
                                    }
                                }
                                button {
                                    r#type: "button",
                                    class: "p-0.5 rounded hover:bg-muted disabled:opacity-30",
                                    disabled: idx >= step_count - 1,
                                    onclick: move |_| move_down(idx),
                                    title: "Move down",
                                    svg {
                                        class: "w-3 h-3",
                                        fill: "none",
                                        stroke: "currentColor",
                                        view_box: "0 0 24 24",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            stroke_width: "2",
                                            d: "M19 9l-7 7-7-7",
                                        }
                                    }
                                }
                            }
                            div { class: "shrink-0 w-8 h-8 rounded-full bg-primary/20 text-primary flex items-center justify-center font-semibold text-sm mt-1",
                                "{idx + 1}"
                            }
                            textarea {
                                class: "flex-1 px-3 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary text-sm resize-none min-h-[60px]",
                                placeholder: "Describe step {idx + 1}...",
                                value: "{text}",
                                oninput: move |evt| {
                                    update_step(idx, evt.value().clone());
                                },
                            }
                            button {
                                r#type: "button",
                                class: "p-2 rounded-lg hover:bg-destructive/10 text-destructive transition opacity-0 group-hover:opacity-100 disabled:opacity-30 disabled:cursor-not-allowed mt-1",
                                disabled: !can_remove,
                                onclick: move |_| remove_step(idx),
                                title: if can_remove { "Remove step" } else { "At least 1 step required" },
                                svg {
                                    class: "w-4 h-4",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M6 18L18 6M6 6l12 12",
                                    }
                                }
                            }
                        }
                    }
                }
            }
            button {
                r#type: "button",
                class: "w-full px-4 py-3 rounded-lg border-2 border-dashed border-border hover:border-primary hover:bg-primary/5 transition text-muted-foreground hover:text-primary text-sm",
                onclick: add_step,
                div { class: "flex items-center justify-center gap-2",
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M12 4v16m8-8H4",
                        }
                    }
                    "Add Step"
                }
            }
        }
    }
}
