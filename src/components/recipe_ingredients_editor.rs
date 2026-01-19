//! Recipe Ingredients Editor Component
//! Dynamic list editor for recipe ingredients

use dioxus::prelude::*;

/// Ingredients list editor with add/remove/reorder
#[component]
pub fn RecipeIngredientsEditor(
    /// Current ingredients list
    ingredients: Signal<Vec<String>>,
    /// Callback when list changes
    on_change: EventHandler<Vec<String>>,
) -> Element {
    // Add a new ingredient
    let add_ingredient = move |_| {
        let mut current = ingredients.read().clone();
        current.push(String::new());
        on_change.call(current);
    };

    // Remove ingredient at index
    let remove_ingredient = move |index: usize| {
        let mut current = ingredients.read().clone();
        if current.len() > 1 {
            current.remove(index);
            on_change.call(current);
        }
    };

    // Update ingredient text
    let update_ingredient = move |index: usize, text: String| {
        let mut current = ingredients.read().clone();
        if let Some(ingredient) = current.get_mut(index) {
            *ingredient = text;
            on_change.call(current);
        }
    };

    // Move ingredient up
    let move_up = move |index: usize| {
        if index > 0 {
            let mut current = ingredients.read().clone();
            current.swap(index, index - 1);
            on_change.call(current);
        }
    };

    // Move ingredient down
    let move_down = move |index: usize| {
        let current_len = ingredients.read().len();
        if index < current_len - 1 {
            let mut current = ingredients.read().clone();
            current.swap(index, index + 1);
            on_change.call(current);
        }
    };

    let items = ingredients.read();
    let can_remove = items.len() > 1;
    let item_count = items.len();
    let items_text = if item_count == 1 { "item" } else { "items" };

    rsx! {
        div {
            class: "space-y-2",

            // Label
            div {
                class: "flex items-center justify-between mb-2",
                label {
                    class: "text-sm font-medium flex items-center gap-2",
                    span { "Ingredients" }
                    span {
                        class: "text-xs text-muted-foreground font-normal",
                        "(at least 1 required)"
                    }
                }
                span {
                    class: "text-xs text-muted-foreground",
                    "{item_count} {items_text}"
                }
            }

            // Ingredients list
            for (index, ingredient) in items.iter().enumerate() {
                {
                    let idx = index;
                    let text = ingredient.clone();

                    rsx! {
                        div {
                            key: "{idx}",
                            class: "group flex items-center gap-2",

                            // Drag handle / reorder buttons
                            div {
                                class: "flex flex-col gap-0.5 opacity-0 group-hover:opacity-100 transition",

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
                                            d: "M5 15l7-7 7 7"
                                        }
                                    }
                                }

                                button {
                                    r#type: "button",
                                    class: "p-0.5 rounded hover:bg-muted disabled:opacity-30",
                                    disabled: idx >= item_count - 1,
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
                                            d: "M19 9l-7 7-7-7"
                                        }
                                    }
                                }
                            }

                            // Bullet point
                            span {
                                class: "text-primary font-bold",
                                "-"
                            }

                            // Input field
                            input {
                                r#type: "text",
                                class: "flex-1 px-3 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary text-sm",
                                placeholder: "e.g., 2 cups all-purpose flour",
                                value: "{text}",
                                oninput: move |evt| {
                                    update_ingredient(idx, evt.value().clone());
                                },
                            }

                            // Remove button
                            button {
                                r#type: "button",
                                class: "p-2 rounded-lg hover:bg-destructive/10 text-destructive transition opacity-0 group-hover:opacity-100 disabled:opacity-30 disabled:cursor-not-allowed",
                                disabled: !can_remove,
                                onclick: move |_| remove_ingredient(idx),
                                title: if can_remove { "Remove ingredient" } else { "At least 1 ingredient required" },

                                svg {
                                    class: "w-4 h-4",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M6 18L18 6M6 6l12 12"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add ingredient button
            button {
                r#type: "button",
                class: "w-full px-4 py-2 rounded-lg border-2 border-dashed border-border hover:border-primary hover:bg-primary/5 transition text-muted-foreground hover:text-primary text-sm",
                onclick: add_ingredient,

                div {
                    class: "flex items-center justify-center gap-2",
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M12 4v16m8-8H4"
                        }
                    }
                    "Add Ingredient"
                }
            }
        }
    }
}
