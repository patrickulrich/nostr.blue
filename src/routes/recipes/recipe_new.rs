//! Create New Recipe Page
//! Form for creating and publishing a new recipe
use crate::components::{ClientInitializing, RecipeForm, RecipeFormData};
use crate::routes::Route;
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::recipe_store;
use dioxus::prelude::*;
#[component]
pub fn RecipeNew() -> Element {
    let navigator = use_navigator();
    let mut is_submitting = use_signal(|| false);
    let mut errors = use_signal(|| None::<Vec<String>>);
    let handle_submit = move |form_data: RecipeFormData| {
        if let Err(validation_errors) = form_data.validate() {
            errors.set(Some(validation_errors));
            return;
        }
        errors.set(None);
        is_submitting.set(true);
        spawn(async move {
            let content = form_data.to_markdown();
            let summary = if form_data.summary.trim().is_empty() {
                None
            } else {
                Some(form_data.summary.as_str())
            };
            match recipe_store::publish_recipe(
                &form_data.title,
                summary,
                &form_data.image_urls,
                &content,
                form_data.tags.clone(),
            )
            .await
            {
                Ok(naddr) => {
                    navigator.push(Route::RecipeDetail { naddr });
                }
                Err(e) => {
                    errors.set(Some(vec![e]));
                    is_submitting.set(false);
                }
            }
        });
    };
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: Route::RecipesHome {},
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition",
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7",
                            }
                        }
                    }
                    h1 { class: "text-xl font-bold flex items-center gap-2",
                        span { class: "text-2xl", "🍳" }
                        "Create Recipe"
                    }
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if !*HAS_SIGNER.read() {
                div { class: "p-4",
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        span { class: "text-5xl mb-4", "🔐" }
                        h2 { class: "text-xl font-semibold mb-2", "Sign In Required" }
                        p { class: "text-muted-foreground mb-4",
                            "You need to sign in to create a recipe"
                        }
                    }
                }
            } else {
                div { class: "p-4 max-w-2xl mx-auto",
                    RecipeForm {
                        submit_text: "Publish Recipe".to_string(),
                        on_submit: handle_submit,
                        is_submitting: *is_submitting.read(),
                        errors: errors.read().clone(),
                    }
                }
            }
        }
    }
}
