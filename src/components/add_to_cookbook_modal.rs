//! Add to Cookbook Modal
//! Modal for adding a recipe to an existing cookbook or creating a new one
use crate::components::MediaUploader;
use crate::routes::Route;
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::pin_boards_store::{
    self, PinInput, PinReference, Pinboard, PinboardInput,
};
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use nostr_sdk::{nips::nip01::Coordinate, FromBech32};
/// Modal for adding a recipe to a cookbook
#[component]
pub fn AddToCookbookModal(
    /// The recipe's naddr to pin
    recipe_naddr: String,
    /// Optional recipe title for display
    #[props(default)]
    recipe_title: Option<String>,
    on_close: EventHandler<()>,
) -> Element {
    #[allow(unused_variables)]
    let navigator = use_navigator();
    let mut create_new = use_signal(|| false);
    let mut cookbooks = use_signal(Vec::<Pinboard>::new);
    let mut cookbooks_loading = use_signal(|| true);
    let mut selected_cookbook = use_signal(|| None::<Pinboard>);
    let mut new_title = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut new_image_url = use_signal(|| None::<String>);
    let mut fetch_error = use_signal(|| None::<String>);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut needs_signin = use_signal(|| false);
    let mut navigation_task: Signal<Option<dioxus::dioxus_core::Task>> = use_signal(|| {
        None
    });
    let mut pending_pin_cookbook: Signal<Option<(String, String)>> = use_signal(|| None);
    let mut has_loaded = use_signal(|| false);
    let mut fetch_generation = use_signal(|| 0u64);
    let client_init = *nostr_client::CLIENT_INITIALIZED.read();
    let has_signer = *HAS_SIGNER.read();
    let loaded = *has_loaded.read();
    use_effect(
        use_reactive!(|(client_init, has_signer, loaded)| {
            // Handle signer changes FIRST - allows transition from signed-in to signed-out
            if !has_signer {
                cookbooks_loading.set(false);
                needs_signin.set(true);
                cookbooks.set(Vec::new());
                has_loaded.set(false);
                // Reset create-related state when signer is lost
                create_new.set(false);
                new_title.set(String::new());
                new_description.set(String::new());
                new_image_url.set(None);
                error.set(None);
                return;
            }

            // Then handle loading
            if loaded {
                return;
            }
            if !client_init {
                cookbooks_loading.set(false);
                return;
            }

            // Increment generation token to invalidate any in-flight requests
            let current_generation = *fetch_generation.peek() + 1;
            fetch_generation.set(current_generation);

            needs_signin.set(false);
            has_loaded.set(true);
            cookbooks_loading.set(true);
            spawn(async move {
                let result = pin_boards_store::fetch_user_cookbooks().await;

                // Check staleness before state updates
                if *fetch_generation.peek() != current_generation {
                    return;
                }

                match result {
                    Ok(books) => {
                        cookbooks.set(books);
                        fetch_error.set(None);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch user cookbooks: {}", e);
                        fetch_error.set(Some(
                            "Failed to load cookbooks. Please try again.".to_string(),
                        ));
                        has_loaded.set(false);
                    }
                }
                cookbooks_loading.set(false);
            });
        }),
    );
    let recipe_naddr_for_add = recipe_naddr.clone();
    let recipe_naddr_for_create = recipe_naddr.clone();
    let handle_add_to_existing = move |_| {
        let cookbook = match selected_cookbook.read().clone() {
            Some(c) => c,
            None => {
                error.set(Some("Please select a cookbook".to_string()));
                return;
            }
        };
        let naddr = recipe_naddr_for_add.clone();
        error.set(None);
        is_submitting.set(true);
        spawn(async move {
            let pin_input = PinInput {
                board_addresses: vec![cookbook.a_tag.clone()],
                reference: PinReference::Coordinate {
                    address: naddr,
                    relay_hint: None,
                },
                title: None,
                image: None,
                content: String::new(),
                tags: vec![],
            };
            match pin_boards_store::publish_pin(pin_input).await {
                Ok(_) => {
                    success.set(true);
                    is_submitting.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    is_submitting.set(false);
                }
            }
        });
    };
    let handle_create_and_add = move |_| {
        let title_val = new_title.read().trim().to_string();
        if title_val.is_empty() {
            error.set(Some("Cookbook name is required".to_string()));
            return;
        }
        if title_val.chars().count() > 100 {
            error.set(Some("Cookbook name must be 100 characters or less".to_string()));
            return;
        }
        let naddr = recipe_naddr_for_create.clone();
        error.set(None);
        is_submitting.set(true);
        let description = {
            let desc = new_description.read().trim().to_string();
            if desc.is_empty() {
                None
            } else {
                Some(desc.chars().take(1000).collect::<String>())
            }
        };
        let image = new_image_url.read().clone();
        spawn(async move {
            let cookbook_input = PinboardInput {
                title: title_val,
                description,
                image,
                tags: vec!["cookbook".to_string()],
                collaborative: false,
            };
            match pin_boards_store::publish_pinboard(cookbook_input, None).await {
                Ok(cookbook_naddr) => {
                    let saved_naddr = cookbook_naddr.clone();
                    let board_a_tag = match Coordinate::from_bech32(&cookbook_naddr) {
                        Ok(coord) => {
                            format!(
                                "{}:{}:{}",
                                coord.kind.as_u16(),
                                coord.public_key.to_hex(),
                                coord.identifier,
                            )
                        }
                        Err(e) => {
                            log::error!(
                                "Failed to parse cookbook naddr {}: {}", saved_naddr, e
                            );
                            error
                                .set(
                                    Some(
                                        format!(
                                            "Cookbook created but failed to add recipe. Your cookbook address: {}",
                                            saved_naddr,
                                        ),
                                    ),
                                );
                            pending_pin_cookbook.set(Some((saved_naddr, String::new())));
                            is_submitting.set(false);
                            return;
                        }
                    };
                    let board_a_tag_for_retry = board_a_tag.clone();
                    let pin_input = PinInput {
                        board_addresses: vec![board_a_tag],
                        reference: PinReference::Coordinate {
                            address: naddr,
                            relay_hint: None,
                        },
                        title: None,
                        image: None,
                        content: String::new(),
                        tags: vec![],
                    };
                    match pin_boards_store::publish_pin(pin_input).await {
                        Ok(_) => {
                            success.set(true);
                            pending_pin_cookbook.set(None);
                            #[cfg(target_arch = "wasm32")]
                            {
                                let task = spawn(async move {
                                    gloo_timers::future::TimeoutFuture::new(1500).await;
                                    navigator
                                        .push(Route::PinBoardDetail {
                                            naddr: cookbook_naddr,
                                        });
                                });
                                navigation_task.set(Some(task));
                            }
                            is_submitting.set(false);
                        }
                        Err(e) => {
                            log::error!("Failed to add recipe to new cookbook: {}", e);
                            error
                                .set(
                                    Some(
                                        format!("Cookbook created but failed to add recipe: {}", e),
                                    ),
                                );
                            pending_pin_cookbook
                                .set(Some((cookbook_naddr, board_a_tag_for_retry)));
                            is_submitting.set(false);
                        }
                    }
                }
                Err(e) => {
                    error.set(Some(e));
                    is_submitting.set(false);
                }
            }
        });
    };
    let recipe_naddr_for_retry = recipe_naddr.clone();
    #[allow(unused_variables)]
    let handle_retry_pin = move |_| {
        let (cookbook_naddr, board_a_tag) = match pending_pin_cookbook.read().clone() {
            Some(info) => info,
            None => return,
        };
        let naddr = recipe_naddr_for_retry.clone();
        error.set(None);
        is_submitting.set(true);
        spawn(async move {
            let pin_input = PinInput {
                board_addresses: vec![board_a_tag],
                reference: PinReference::Coordinate {
                    address: naddr,
                    relay_hint: None,
                },
                title: None,
                image: None,
                content: String::new(),
                tags: vec![],
            };
            match pin_boards_store::publish_pin(pin_input).await {
                Ok(_) => {
                    success.set(true);
                    pending_pin_cookbook.set(None);
                    #[cfg(target_arch = "wasm32")]
                    {
                        let task = spawn(async move {
                            gloo_timers::future::TimeoutFuture::new(1500).await;
                            navigator
                                .push(Route::PinBoardDetail {
                                    naddr: cookbook_naddr,
                                });
                        });
                        navigation_task.set(Some(task));
                    }
                    is_submitting.set(false);
                }
                Err(e) => {
                    log::error!("Failed to add recipe to cookbook (retry): {}", e);
                    error.set(Some(format!("Failed to add recipe: {}", e)));
                    is_submitting.set(false);
                }
            }
        });
    };
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| {
                if !*is_submitting.read() {
                    if let Some(task) = navigation_task.write().take() {
                        task.cancel();
                    }
                    on_close.call(());
                }
            },
            div {
                class: "bg-card rounded-xl shadow-xl max-w-md w-full max-h-[85vh] overflow-y-auto",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "sticky top-0 bg-card border-b border-border px-4 py-3 flex items-center justify-between",
                    h2 { class: "text-lg font-bold flex items-center gap-2",
                        span { class: "text-xl", "📚" }
                        "Add to Cookbook"
                    }
                    button {
                        r#type: "button",
                        class: if *is_submitting.read() { "p-1.5 rounded-full opacity-50 cursor-not-allowed" } else { "p-1.5 rounded-full hover:bg-muted transition" },
                        disabled: *is_submitting.read(),
                        onclick: move |_| {
                            if !*is_submitting.read() {
                                if let Some(task) = navigation_task.write().take() {
                                    task.cancel();
                                }
                                on_close.call(());
                            }
                        },
                        svg {
                            class: "w-5 h-5",
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
                div { class: "p-4",
                    if *success.read() {
                        div { class: "flex flex-col items-center justify-center py-8 text-center",
                            span { class: "text-5xl mb-3", "✅" }
                            h3 { class: "text-lg font-semibold mb-1", "Recipe Added!" }
                            p { class: "text-sm text-muted-foreground",
                                "The recipe has been added to your cookbook."
                            }
                        }
                    } else if pending_pin_cookbook.read().is_some() {
                        div { class: "space-y-4",
                            {
                                let has_valid_a_tag = pending_pin_cookbook
                                    .read()
                                    .as_ref()
                                    .map(|(_, a_tag)| !a_tag.is_empty())
                                    .unwrap_or(false);
                                rsx! {
                                    div { class: "p-4 bg-amber-100 dark:bg-amber-900/20 rounded-lg",
                                        div { class: "flex items-start gap-3",
                                            span { class: "text-2xl", "📚" }
                                            div {
                                                p { class: "font-medium text-amber-800 dark:text-amber-200",
                                                    "Cookbook created successfully!"
                                                }
                                                p { class: "text-sm text-amber-700 dark:text-amber-300 mt-1",
                                                    if has_valid_a_tag {
                                                        "However, adding the recipe failed. Click below to retry."
                                                    } else {
                                                        "However, adding the recipe failed. Visit your cookbook to add it manually."
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(ref err) = *error.read() {
                                div { class: "p-3 bg-red-100 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-lg text-sm",
                                    "{err}"
                                }
                            }
                            if let Some(ref title) = recipe_title {
                                div { class: "p-3 bg-muted/50 rounded-lg",
                                    p { class: "text-sm text-muted-foreground", "Recipe to add:" }
                                    p { class: "font-medium truncate", "{title}" }
                                }
                            }
                            if pending_pin_cookbook
                                .read()
                                .as_ref()
                                .map(|(_, a_tag)| !a_tag.is_empty())
                                .unwrap_or(false)
                            {
                                button {
                                    r#type: "button",
                                    class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                                    disabled: *is_submitting.read(),
                                    onclick: handle_retry_pin,
                                    if *is_submitting.read() {
                                        span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-2" }
                                        "Adding Recipe..."
                                    } else {
                                        "Retry Adding Recipe"
                                    }
                                }
                            } else {
                                p { class: "text-sm text-muted-foreground text-center",
                                    "You can add the recipe manually from your cookbook."
                                }
                            }
                            if let Some((ref naddr, _)) = *pending_pin_cookbook.read() {
                                {
                                    let naddr_for_link = naddr.clone();
                                    rsx! {
                                        div { class: "text-center",
                                            Link {
                                                to: Route::PinBoardDetail {
                                                    naddr: naddr_for_link,
                                                },
                                                class: "text-sm text-muted-foreground hover:text-primary transition",
                                                onclick: move |_| on_close.call(()),
                                                "Skip and view cookbook →"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        if let Some(ref title) = recipe_title {
                            div { class: "mb-4 p-3 bg-muted/50 rounded-lg",
                                p { class: "text-sm text-muted-foreground", "Adding recipe:" }
                                p { class: "font-medium truncate", "{title}" }
                            }
                        }
                        if let Some(ref err) = *error.read() {
                            div { class: "mb-4 p-3 bg-red-100 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-lg text-sm",
                                "{err}"
                            }
                        }
                        div { class: "flex gap-2 border-b border-border pb-2 mb-4",
                            button {
                                r#type: "button",
                                class: if !*create_new.read() { "px-3 py-1 text-sm font-medium border-b-2 border-primary" } else { "px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground" },
                                onclick: move |_| create_new.set(false),
                                "Existing Cookbook"
                            }
                            button {
                                r#type: "button",
                                class: if *create_new.read() { "px-3 py-1 text-sm font-medium border-b-2 border-primary" } else if *needs_signin.read() { "px-3 py-1 text-sm font-medium text-muted-foreground opacity-50 cursor-not-allowed" } else { "px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground" },
                                disabled: *needs_signin.read(),
                                onclick: move |_| {
                                    if !*needs_signin.read() {
                                        create_new.set(true);
                                    }
                                },
                                "Create New"
                            }
                        }
                        if !*create_new.read() {
                            div { class: "space-y-4",
                                if *cookbooks_loading.read() {
                                    div { class: "text-sm text-muted-foreground italic py-4 text-center",
                                        "Loading your cookbooks..."
                                    }
                                } else if *needs_signin.read() {
                                    div { class: "text-center py-6",
                                        span { class: "text-4xl mb-2 block", "🔑" }
                                        p { class: "text-sm text-muted-foreground mb-3",
                                            "Sign in to view and manage your cookbooks."
                                        }
                                        p { class: "text-xs text-muted-foreground",
                                            "Sign in to create new cookbooks."
                                        }
                                    }
                                } else if let Some(ref err) = *fetch_error.read() {
                                    div { class: "text-center py-6",
                                        span { class: "text-4xl mb-2 block", "⚠️" }
                                        p { class: "text-sm text-red-600 dark:text-red-400 mb-3",
                                            "{err}"
                                        }
                                        div { class: "flex gap-3 justify-center",
                                            button {
                                                r#type: "button",
                                                class: "text-sm text-primary hover:underline",
                                                onclick: move |_| {
                                                    fetch_error.set(None);
                                                    has_loaded.set(false);
                                                },
                                                "Retry"
                                            }
                                            button {
                                                r#type: "button",
                                                class: "text-sm text-muted-foreground hover:underline",
                                                onclick: move |_| create_new.set(true),
                                                "Create new cookbook instead"
                                            }
                                        }
                                    }
                                } else if cookbooks.read().is_empty() {
                                    div { class: "text-center py-6",
                                        span { class: "text-4xl mb-2 block", "📚" }
                                        p { class: "text-sm text-muted-foreground mb-3",
                                            "You don't have any cookbooks yet."
                                        }
                                        button {
                                            r#type: "button",
                                            class: "text-sm text-primary hover:underline",
                                            onclick: move |_| create_new.set(true),
                                            "Create your first cookbook →"
                                        }
                                    }
                                } else {
                                    div { class: "space-y-2 max-h-60 overflow-y-auto",
                                        for cookbook in cookbooks.read().iter() {
                                            {
                                                let cb = cookbook.clone();
                                                let cb_for_check = cookbook.clone();
                                                let is_selected = selected_cookbook
                                                    .read()
                                                    .as_ref()
                                                    .map(|s| s.naddr == cb_for_check.naddr)
                                                    .unwrap_or(false);
                                                rsx! {
                                                    button {
                                                        key: "{cb.naddr}",
                                                        r#type: "button",
                                                        class: if is_selected { "w-full p-3 border-2 border-primary rounded-lg flex items-center gap-3 text-left bg-primary/5" } else { "w-full p-3 border border-border rounded-lg flex items-center gap-3 text-left hover:bg-muted/50 transition" },
                                                        onclick: move |_| selected_cookbook.set(Some(cb.clone())),
                                                        div { class: "w-12 h-12 rounded-lg overflow-hidden bg-gradient-to-br from-orange-500/60 to-amber-600/60 shrink-0",
                                                            if let Some(ref img) = cb_for_check.image.as_ref().filter(|u| is_valid_http_url(u)) {
                                                                img {
                                                                    src: "{img}",
                                                                    alt: "{cb_for_check.title}",
                                                                    class: "w-full h-full object-cover",
                                                                }
                                                            } else {
                                                                div { class: "w-full h-full flex items-center justify-center text-xl", "📚" }
                                                            }
                                                        }
                                                        div { class: "flex-1 min-w-0",
                                                            p { class: "font-medium truncate", "{cb_for_check.title}" }
                                                            if let Some(ref desc) = cb_for_check.description {
                                                                p { class: "text-xs text-muted-foreground truncate", "{desc}" }
                                                            }
                                                        }
                                                        if is_selected {
                                                            svg {
                                                                class: "w-5 h-5 text-primary shrink-0",
                                                                fill: "none",
                                                                stroke: "currentColor",
                                                                view_box: "0 0 24 24",
                                                                path {
                                                                    stroke_linecap: "round",
                                                                    stroke_linejoin: "round",
                                                                    stroke_width: "2",
                                                                    d: "M5 13l4 4L19 7",
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    div { class: "pt-4",
                                        button {
                                            r#type: "button",
                                            class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                                            disabled: *is_submitting.read() || selected_cookbook.read().is_none(),
                                            onclick: handle_add_to_existing,
                                            if *is_submitting.read() {
                                                span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-2" }
                                                "Adding..."
                                            } else {
                                                "Add to Cookbook"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if *create_new.read() {
                            div { class: "space-y-4",
                                div {
                                    label { class: "block text-sm font-medium mb-1.5",
                                        "Cookbook Name "
                                        span { class: "text-red-500", "*" }
                                    }
                                    input {
                                        class: "w-full px-3 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary text-sm",
                                        r#type: "text",
                                        placeholder: "My Recipe Collection",
                                        value: "{new_title}",
                                        oninput: move |evt| new_title.set(evt.value()),
                                    }
                                }
                                div {
                                    label { class: "block text-sm font-medium mb-1.5",
                                        "Description"
                                    }
                                    textarea {
                                        class: "w-full px-3 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary resize-none text-sm",
                                        rows: "2",
                                        placeholder: "What recipes will this cookbook contain?",
                                        value: "{new_description}",
                                        oninput: move |evt| new_description.set(evt.value()),
                                    }
                                }
                                div {
                                    label { class: "block text-sm font-medium mb-1.5",
                                        "Cover Image"
                                    }
                                    if let Some(ref url) = *new_image_url.read() {
                                        div { class: "relative w-full h-24 rounded-lg overflow-hidden mb-2",
                                            img {
                                                src: "{url}",
                                                alt: "Cover image",
                                                class: "w-full h-full object-cover",
                                            }
                                            button {
                                                r#type: "button",
                                                class: "absolute top-2 right-2 p-1 rounded-full bg-red-500 text-white hover:bg-red-600 text-xs",
                                                onclick: move |_| new_image_url.set(None),
                                                "✕"
                                            }
                                        }
                                    }
                                    MediaUploader {
                                        on_upload: move |url: String| {
                                            if is_valid_http_url(&url) {
                                                new_image_url.set(Some(url));
                                            }
                                        },
                                        button_label: "Upload cover".to_string(),
                                    }
                                }
                                div { class: "pt-2",
                                    button {
                                        r#type: "button",
                                        class: "w-full px-4 py-2.5 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                                        disabled: *is_submitting.read(),
                                        onclick: handle_create_and_add,
                                        if *is_submitting.read() {
                                            span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-2" }
                                            "Creating..."
                                        } else {
                                            "Create & Add Recipe"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
