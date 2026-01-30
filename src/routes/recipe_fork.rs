//! Fork/Edit Recipe Page
//! Form for forking (copying) or editing an existing recipe
use crate::components::{
    ClientInitializing, RecipeDetailViewSkeleton, RecipeForm, RecipeFormData,
};
use crate::routes::Route;
use crate::stores::auth_store;
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::recipe_store::{self, CachedRecipe};
use dioxus::prelude::*;
#[component]
pub fn RecipeFork(naddr: String) -> Element {
    let naddr_for_effect = naddr.clone();
    let navigator = use_navigator();
    let mut original_recipe = use_signal(|| None::<CachedRecipe>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut is_submitting = use_signal(|| false);
    let mut form_errors = use_signal(|| None::<Vec<String>>);
    let is_owner = use_memo(move || {
        if let Some(ref r) = *original_recipe.read() {
            if let Some(pubkey) = auth_store::get_pubkey() {
                return r.event.pubkey.to_hex() == pubkey;
            }
        }
        false
    });
    use_effect(
        use_reactive(
            (&*nostr_client::CLIENT_INITIALIZED.read(), &naddr_for_effect),
            move |(client_initialized, naddr_str)| {
                if !client_initialized {
                    return;
                }
                let naddr_clone = naddr_str.clone();
                loading.set(true);
                error.set(None);
                spawn(async move {
                    match recipe_store::fetch_recipe_by_naddr(&naddr_clone).await {
                        Ok(Some(r)) => {
                            original_recipe.set(Some(r));
                            loading.set(false);
                        }
                        Ok(None) => {
                            error.set(Some("Recipe not found".to_string()));
                            loading.set(false);
                        }
                        Err(e) => {
                            error.set(Some(e));
                            loading.set(false);
                        }
                    }
                });
            },
        ),
    );
    let handle_submit = move |form_data: RecipeFormData| {
        if let Err(validation_errors) = form_data.validate() {
            form_errors.set(Some(validation_errors));
            return;
        }
        form_errors.set(None);
        is_submitting.set(true);
        let is_editing = *is_owner.peek();
        let original = original_recipe.peek().clone();
        spawn(async move {
            let content = form_data.to_markdown();
            let summary = if form_data.summary.trim().is_empty() {
                None
            } else {
                Some(form_data.summary.as_str())
            };
            let result = if is_editing {
                if let Some(ref orig) = original {
                    recipe_store::update_recipe(
                            orig.metadata.identifier.as_deref().unwrap_or(""),
                            &form_data.title,
                            summary,
                            &form_data.image_urls,
                            &content,
                            form_data.tags.clone(),
                        )
                        .await
                } else {
                    Err("Original recipe not found".to_string())
                }
            } else if let Some(ref orig) = original {
                recipe_store::fork_recipe(
                        orig,
                        &form_data.title,
                        &content,
                        form_data.tags.clone(),
                    )
                    .await
            } else {
                Err("Original recipe not found".to_string())
            };
            match result {
                Ok(naddr) => {
                    navigator.push(Route::RecipeDetail { naddr });
                }
                Err(e) => {
                    form_errors.set(Some(vec![e]));
                    is_submitting.set(false);
                }
            }
        });
    };
    let initial_data = use_memo(move || {
        original_recipe.read().as_ref().map(RecipeFormData::from_cached_recipe)
    });
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: Route::RecipeDetail {
                            naddr: naddr.clone(),
                        },
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
                        if *is_owner.read() {
                            span { class: "text-2xl", "✏️" }
                            "Edit Recipe"
                        } else {
                            span { class: "text-2xl", "🍴" }
                            "Fork Recipe"
                        }
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
                            "You need to sign in to fork or edit a recipe"
                        }
                    }
                }
            } else if *loading.read() {
                div { class: "p-4", RecipeDetailViewSkeleton {} }
            } else if let Some(err) = error.read().as_ref() {
                div { class: "p-4",
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        span { class: "text-5xl mb-4", "🍽️" }
                        h2 { class: "text-xl font-semibold mb-2", "Recipe Not Found" }
                        p { class: "text-muted-foreground mb-4", "{err}" }
                        Link {
                            to: Route::RecipesHome {},
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition",
                            "Browse Recipes"
                        }
                    }
                }
            } else if let Some(data) = initial_data.read().clone() {
                div { class: "p-4 max-w-2xl mx-auto",
                    if !*is_owner.read() {
                        div { class: "mb-6 p-4 bg-muted/50 rounded-lg",
                            div { class: "flex items-start gap-3",
                                span { class: "text-2xl", "🍴" }
                                div {
                                    h3 { class: "font-medium mb-1", "Forking Recipe" }
                                    p { class: "text-sm text-muted-foreground",
                                        "You're creating your own copy of this recipe. Make any changes you like - the original will remain unchanged."
                                    }
                                }
                            }
                        }
                    }
                    RecipeForm {
                        initial_data: Some(data),
                        submit_text: if *is_owner.read() { "Save Changes".to_string() } else { "Publish Fork".to_string() },
                        on_submit: handle_submit,
                        is_submitting: *is_submitting.read(),
                        errors: form_errors.read().clone(),
                    }
                }
            }
        }
    }
}
