//! Recipe Form Component
//! Complete form for creating or editing recipes

use dioxus::prelude::*;
use crate::components::recipe_ingredients_editor::RecipeIngredientsEditor;
use crate::components::recipe_directions_editor::RecipeDirectionsEditor;
use crate::components::recipe_tag_selector::RecipeTagSelector;
use crate::components::media_uploader::MediaUploader;
use crate::stores::recipe_store::CachedRecipe;

/// Form data for recipe creation/editing
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeFormData {
    pub title: String,
    pub summary: String,
    /// Multiple images supported
    pub image_urls: Vec<String>,
    pub chef_notes: String,
    pub prep_time: String,
    pub cook_time: String,
    pub servings: String,
    pub ingredients: Vec<String>,
    pub directions: Vec<String>,
    pub tags: Vec<String>,
    pub additional_resources: String,
}

impl RecipeFormData {
    /// Create form data from an existing recipe (for forking/editing)
    pub fn from_cached_recipe(recipe: &CachedRecipe) -> Self {
        let parsed = recipe.parsed.as_ref();

        Self {
            title: recipe.metadata.title.clone(),
            summary: recipe.metadata.summary.clone().unwrap_or_default(),
            image_urls: recipe.metadata.images.clone(),
            chef_notes: parsed.and_then(|p| p.chef_notes.clone()).unwrap_or_default(),
            prep_time: parsed.and_then(|p| p.details.prep_time.clone()).unwrap_or_default(),
            cook_time: parsed.and_then(|p| p.details.cook_time.clone()).unwrap_or_default(),
            servings: parsed.and_then(|p| p.details.servings.clone()).unwrap_or_default(),
            ingredients: parsed.map(|p| p.ingredients.clone()).unwrap_or_else(|| vec![String::new()]),
            directions: parsed.map(|p| p.directions.clone()).unwrap_or_else(|| vec![String::new()]),
            tags: recipe.metadata.tags.clone(),
            additional_resources: parsed.and_then(|p| p.additional_resources.clone()).unwrap_or_default(),
        }
    }

    /// Validate the form data
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.title.trim().is_empty() {
            errors.push("Title is required".to_string());
        }

        if self.ingredients.iter().all(|i| i.trim().is_empty()) {
            errors.push("At least one ingredient is required".to_string());
        }

        if self.directions.iter().all(|d| d.trim().is_empty()) {
            errors.push("At least one direction step is required".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get non-empty ingredients
    pub fn clean_ingredients(&self) -> Vec<String> {
        self.ingredients
            .iter()
            .map(|i| i.trim().to_string())
            .filter(|i| !i.is_empty())
            .collect()
    }

    /// Get non-empty directions
    pub fn clean_directions(&self) -> Vec<String> {
        self.directions
            .iter()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty())
            .collect()
    }

    /// Convert form data to markdown content
    pub fn to_markdown(&self) -> String {
        let mut content = String::new();

        // Chef's notes (optional)
        if !self.chef_notes.trim().is_empty() {
            content.push_str("## Chef's notes\n\n");
            content.push_str(&self.chef_notes);
            content.push_str("\n\n");
        }

        // Details (optional)
        let has_details = !self.prep_time.trim().is_empty()
            || !self.cook_time.trim().is_empty()
            || !self.servings.trim().is_empty();

        if has_details {
            content.push_str("## Details\n\n");
            if !self.prep_time.trim().is_empty() {
                content.push_str(&format!("- ⏲️ Prep time: {}\n", self.prep_time.trim()));
            }
            if !self.cook_time.trim().is_empty() {
                content.push_str(&format!("- 🍳 Cook time: {}\n", self.cook_time.trim()));
            }
            if !self.servings.trim().is_empty() {
                content.push_str(&format!("- 🍽️ Servings: {}\n", self.servings.trim()));
            }
            content.push('\n');
        }

        // Ingredients (required)
        content.push_str("## Ingredients\n\n");
        for ingredient in self.clean_ingredients() {
            content.push_str(&format!("- {}\n", ingredient));
        }
        content.push('\n');

        // Directions (required)
        content.push_str("## Directions\n\n");
        for (i, step) in self.clean_directions().iter().enumerate() {
            content.push_str(&format!("{}. {}\n", i + 1, step));
        }
        content.push('\n');

        // Additional resources (optional)
        if !self.additional_resources.trim().is_empty() {
            content.push_str("## Additional Resources\n\n");
            content.push_str(&self.additional_resources);
            content.push('\n');
        }

        content
    }
}

/// Recipe creation/editing form
#[component]
pub fn RecipeForm(
    /// Initial form data (for editing/forking)
    #[props(default)]
    initial_data: Option<RecipeFormData>,
    /// Submit button text
    #[props(default = "Publish Recipe".to_string())]
    submit_text: String,
    /// Callback when form is submitted
    on_submit: EventHandler<RecipeFormData>,
    /// Whether form is submitting
    #[props(default = false)]
    is_submitting: bool,
    /// Validation errors to display
    #[props(default)]
    errors: Option<Vec<String>>,
) -> Element {
    // Form state
    let initial = initial_data.unwrap_or_default();

    let mut title = use_signal(|| initial.title.clone());
    let mut summary = use_signal(|| initial.summary.clone());
    let mut image_urls = use_signal(|| initial.image_urls.clone());
    let mut chef_notes = use_signal(|| initial.chef_notes.clone());
    let mut prep_time = use_signal(|| initial.prep_time.clone());
    let mut cook_time = use_signal(|| initial.cook_time.clone());
    let mut servings = use_signal(|| initial.servings.clone());
    let mut ingredients = use_signal(|| {
        if initial.ingredients.is_empty() {
            vec![String::new()]
        } else {
            initial.ingredients.clone()
        }
    });
    let mut directions = use_signal(|| {
        if initial.directions.is_empty() {
            vec![String::new()]
        } else {
            initial.directions.clone()
        }
    });
    let mut tags = use_signal(|| initial.tags.clone());
    let mut additional_resources = use_signal(|| initial.additional_resources.clone());

    // Image upload state
    let mut show_image_uploader = use_signal(|| false);

    // Handle form submission
    let handle_submit = move |_| {
        let form_data = RecipeFormData {
            title: title.read().clone(),
            summary: summary.read().clone(),
            image_urls: image_urls.read().clone(),
            chef_notes: chef_notes.read().clone(),
            prep_time: prep_time.read().clone(),
            cook_time: cook_time.read().clone(),
            servings: servings.read().clone(),
            ingredients: ingredients.read().clone(),
            directions: directions.read().clone(),
            tags: tags.read().clone(),
            additional_resources: additional_resources.read().clone(),
        };
        on_submit.call(form_data);
    };

    rsx! {
        form {
            class: "space-y-6",
            onsubmit: move |evt| {
                evt.prevent_default();
                handle_submit(());
            },

            // Validation errors
            if let Some(ref errs) = errors {
                if !errs.is_empty() {
                    div {
                        class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg",
                        h4 {
                            class: "font-medium text-destructive mb-2",
                            "Please fix the following errors:"
                        }
                        ul {
                            class: "list-disc list-inside text-sm text-destructive",
                            for err in errs.iter() {
                                li { "{err}" }
                            }
                        }
                    }
                }
            }

            // Title
            div {
                label {
                    class: "block text-sm font-medium mb-2",
                    "Title "
                    span { class: "text-destructive", "*" }
                }
                input {
                    r#type: "text",
                    class: "w-full px-4 py-3 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary text-lg",
                    placeholder: "What are you cooking?",
                    value: "{title}",
                    oninput: move |evt| title.set(evt.value().clone()),
                    required: true,
                }
            }

            // Summary
            div {
                label {
                    class: "block text-sm font-medium mb-2",
                    "Summary"
                }
                textarea {
                    class: "w-full px-4 py-3 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                    placeholder: "A brief description of your recipe...",
                    rows: "2",
                    value: "{summary}",
                    oninput: move |evt| summary.set(evt.value().clone()),
                }
            }

            // Cover images (multiple supported)
            div {
                label {
                    class: "block text-sm font-medium mb-2",
                    "Images"
                    if !image_urls.read().is_empty() {
                        span {
                            class: "ml-2 text-xs text-muted-foreground font-normal",
                            "({image_urls.read().len()} uploaded)"
                        }
                    }
                }

                // Display existing images
                if !image_urls.read().is_empty() {
                    div {
                        class: "grid grid-cols-2 md:grid-cols-3 gap-3 mb-3",
                        for (idx, url) in image_urls.read().iter().enumerate() {
                            div {
                                key: "{idx}",
                                class: "relative aspect-video rounded-lg overflow-hidden group",
                                img {
                                    src: "{url}",
                                    alt: "Recipe image {idx + 1}",
                                    class: "w-full h-full object-cover",
                                }
                                // Image controls overlay
                                div {
                                    class: "absolute inset-0 bg-black/50 opacity-0 group-hover:opacity-100 transition flex items-center justify-center gap-2",
                                    // Move up button
                                    if idx > 0 {
                                        button {
                                            r#type: "button",
                                            class: "p-2 bg-white/20 rounded-full text-white hover:bg-white/30 transition",
                                            onclick: move |_| {
                                                let mut current = image_urls.read().clone();
                                                if idx > 0 {
                                                    current.swap(idx, idx - 1);
                                                    image_urls.set(current);
                                                }
                                            },
                                            title: "Move left",
                                            "←"
                                        }
                                    }
                                    // Remove button
                                    button {
                                        r#type: "button",
                                        class: "p-2 bg-red-500/80 rounded-full text-white hover:bg-red-500 transition",
                                        onclick: move |_| {
                                            let mut current = image_urls.read().clone();
                                            current.remove(idx);
                                            image_urls.set(current);
                                        },
                                        title: "Remove image",
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
                                    // Move down button
                                    if idx < image_urls.read().len() - 1 {
                                        button {
                                            r#type: "button",
                                            class: "p-2 bg-white/20 rounded-full text-white hover:bg-white/30 transition",
                                            onclick: move |_| {
                                                let mut current = image_urls.read().clone();
                                                let len = current.len();
                                                if idx < len - 1 {
                                                    current.swap(idx, idx + 1);
                                                    image_urls.set(current);
                                                }
                                            },
                                            title: "Move right",
                                            "→"
                                        }
                                    }
                                }
                                // Primary badge for first image
                                if idx == 0 {
                                    span {
                                        class: "absolute top-2 left-2 px-2 py-0.5 bg-primary text-primary-foreground text-xs rounded",
                                        "Cover"
                                    }
                                }
                            }
                        }
                    }
                }

                // Add image button/uploader
                if *show_image_uploader.read() {
                    div {
                        class: "space-y-2",
                        MediaUploader {
                            on_upload: move |url: String| {
                                let mut current = image_urls.read().clone();
                                current.push(url);
                                image_urls.set(current);
                                show_image_uploader.set(false);
                            },
                        }
                        button {
                            r#type: "button",
                            class: "text-sm text-muted-foreground hover:text-foreground",
                            onclick: move |_| show_image_uploader.set(false),
                            "Cancel"
                        }
                    }
                } else {
                    button {
                        r#type: "button",
                        class: if image_urls.read().is_empty() {
                            "w-full aspect-video rounded-lg border-2 border-dashed border-border hover:border-primary hover:bg-primary/5 transition flex flex-col items-center justify-center gap-2 text-muted-foreground hover:text-primary"
                        } else {
                            "w-full py-3 rounded-lg border-2 border-dashed border-border hover:border-primary hover:bg-primary/5 transition flex items-center justify-center gap-2 text-muted-foreground hover:text-primary"
                        },
                        onclick: move |_| show_image_uploader.set(true),

                        svg {
                            class: "w-6 h-6",
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
                        span {
                            if image_urls.read().is_empty() {
                                "Add cover image"
                            } else {
                                "Add another image"
                            }
                        }
                    }
                }
            }

            // Tags
            RecipeTagSelector {
                selected_tags: tags,
                on_change: move |new_tags| tags.set(new_tags),
            }

            // Chef's notes (optional)
            div {
                label {
                    class: "block text-sm font-medium mb-2",
                    "Chef's Notes "
                    span { class: "text-xs text-muted-foreground font-normal", "(optional)" }
                }
                textarea {
                    class: "w-full px-4 py-3 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                    placeholder: "Share tips, story behind the recipe, or any special notes...",
                    rows: "3",
                    value: "{chef_notes}",
                    oninput: move |evt| chef_notes.set(evt.value().clone()),
                }
            }

            // Details (prep time, cook time, servings)
            div {
                class: "grid grid-cols-3 gap-4",

                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Prep Time"
                    }
                    input {
                        r#type: "text",
                        class: "w-full px-3 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary text-sm",
                        placeholder: "e.g., 15 mins",
                        value: "{prep_time}",
                        oninput: move |evt| prep_time.set(evt.value().clone()),
                    }
                }

                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Cook Time"
                    }
                    input {
                        r#type: "text",
                        class: "w-full px-3 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary text-sm",
                        placeholder: "e.g., 30 mins",
                        value: "{cook_time}",
                        oninput: move |evt| cook_time.set(evt.value().clone()),
                    }
                }

                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Servings"
                    }
                    input {
                        r#type: "text",
                        class: "w-full px-3 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary text-sm",
                        placeholder: "e.g., 4",
                        value: "{servings}",
                        oninput: move |evt| servings.set(evt.value().clone()),
                    }
                }
            }

            // Ingredients
            RecipeIngredientsEditor {
                ingredients: ingredients,
                on_change: move |new_ingredients| ingredients.set(new_ingredients),
            }

            // Directions
            RecipeDirectionsEditor {
                directions: directions,
                on_change: move |new_directions| directions.set(new_directions),
            }

            // Additional resources (optional)
            div {
                label {
                    class: "block text-sm font-medium mb-2",
                    "Additional Resources "
                    span { class: "text-xs text-muted-foreground font-normal", "(optional)" }
                }
                textarea {
                    class: "w-full px-4 py-3 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                    placeholder: "Links, references, or related recipes...",
                    rows: "2",
                    value: "{additional_resources}",
                    oninput: move |evt| additional_resources.set(evt.value().clone()),
                }
            }

            // Submit button
            div {
                class: "flex justify-end gap-4 pt-4",

                button {
                    r#type: "submit",
                    class: "px-6 py-3 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                    disabled: is_submitting,

                    if is_submitting {
                        // Loading spinner
                        svg {
                            class: "w-5 h-5 animate-spin",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            circle {
                                class: "opacity-25",
                                cx: "12",
                                cy: "12",
                                r: "10",
                                stroke: "currentColor",
                                stroke_width: "4",
                            }
                            path {
                                class: "opacity-75",
                                fill: "currentColor",
                                d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                            }
                        }
                        "Publishing..."
                    } else {
                        "{submit_text}"
                    }
                }
            }
        }
    }
}
