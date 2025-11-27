use dioxus::prelude::*;
use crate::utils::generate_option_id;

#[derive(Clone, Debug, PartialEq)]
pub struct PollOptionData {
    pub id: String,
    pub text: String,
}

#[component]
pub fn PollOptionList(
    options: Signal<Vec<PollOptionData>>,
    on_change: EventHandler<Vec<PollOptionData>>,
) -> Element {
    // Handler to add a new option
    let add_option = move |_| {
        let mut current = options.read().clone();
        if current.len() < 10 {
            current.push(PollOptionData {
                id: generate_option_id(),
                text: String::new(),
            });
            on_change.call(current);
        }
    };

    // Handler to remove an option
    let remove_option = move |index: usize| {
        let mut current = options.read().clone();
        if current.len() > 2 {  // Min 2 options
            current.remove(index);
            on_change.call(current);
        }
    };

    // Handler to update option text
    let update_option = move |index: usize, text: String| {
        let mut current = options.read().clone();
        if let Some(option) = current.get_mut(index) {
            option.text = text;
            on_change.call(current);
        }
    };

    let opts = options.read();
    let can_add = opts.len() < 10;
    let can_remove = opts.len() > 2;

    rsx! {
        div {
            class: "space-y-3",

            // List of options
            for (index, option) in opts.iter().enumerate() {
                {
                    let option_index = index;
                    let option_text = option.text.clone();

                    rsx! {
                        div {
                            key: "{option.id}",
                            class: "flex items-start gap-2",

                            // Option number indicator
                            div {
                                class: "flex-shrink-0 w-8 h-10 flex items-center justify-center text-muted-foreground font-medium",
                                "{index + 1}."
                            }

                            // Text input
                            textarea {
                                class: "flex-1 px-3 py-2 rounded-lg border border-border bg-background focus:outline-none focus:ring-2 focus:ring-primary resize-none",
                                placeholder: "Option {index + 1}",
                                rows: "1",
                                value: "{option_text}",
                                oninput: move |evt| {
                                    update_option(option_index, evt.value().clone());
                                },
                                // Auto-resize textarea (wasm32 only)
                                onmounted: move |evt| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        if let Some(element) = evt.data.downcast::<web_sys::HtmlTextAreaElement>() {
                                            let _ = element.set_attribute("style", "height: auto;");
                                            let scroll_height = element.scroll_height();
                                            let _ = element.set_attribute("style", &format!("height: {}px;", scroll_height));
                                        }
                                    }
                                }
                            }

                            // Delete button
                            button {
                                class: "flex-shrink-0 w-10 h-10 flex items-center justify-center rounded-lg hover:bg-destructive/10 text-destructive transition disabled:opacity-30 disabled:cursor-not-allowed",
                                disabled: !can_remove,
                                onclick: move |_| remove_option(option_index),
                                title: if can_remove { "Remove option" } else { "At least 2 options required" },

                                svg {
                                    class: "w-5 h-5",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add option button
            button {
                class: "w-full px-4 py-3 rounded-lg border-2 border-dashed border-border hover:border-primary hover:bg-primary/5 transition text-muted-foreground hover:text-primary disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:border-border disabled:hover:bg-transparent disabled:hover:text-muted-foreground",
                disabled: !can_add,
                onclick: add_option,

                div {
                    class: "flex items-center justify-center gap-2",
                    svg {
                        class: "w-5 h-5",
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M12 4v16m8-8H4"
                        }
                    }
                    if can_add {
                        "Add Option ({opts.len()}/10)"
                    } else {
                        "Maximum 10 options"
                    }
                }
            }
        }
    }
}
