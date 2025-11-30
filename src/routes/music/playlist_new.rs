// Playlist Creation Page
// Allows users to create Kind 34139 playlists on nostr

use dioxus::prelude::*;
use crate::routes::Route;
use crate::stores::{auth_store, nostr_music};

/// Slugify a string for use as a d-tag
fn slugify(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[component]
pub fn MusicPlaylistNew() -> Element {
    let navigator = navigator();
    let is_authenticated = auth_store::is_authenticated();

    // Form state
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut image_url = use_signal(String::new);
    let mut is_public = use_signal(|| true);
    let mut is_collaborative = use_signal(|| false);
    let mut categories = use_signal(|| Vec::<String>::new());
    let mut category_input = use_signal(String::new);
    let mut is_publishing = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    // Redirect if not authenticated
    if !is_authenticated {
        return rsx! {
            div {
                class: "max-w-2xl mx-auto p-6",
                div {
                    class: "text-center py-16",
                    h1 {
                        class: "text-2xl font-bold mb-4",
                        "Sign In Required"
                    }
                    p {
                        class: "text-muted-foreground mb-6",
                        "You need to sign in to create playlists."
                    }
                    Link {
                        to: Route::Home {},
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                        "Go Home"
                    }
                }
            }
        };
    }

    let mut do_add_category = move || {
        let cat = category_input.read().trim().to_string();
        if !cat.is_empty() && !categories.read().contains(&cat) {
            categories.write().push(cat);
            category_input.set(String::new());
        }
    };

    let mut remove_category = move |cat: String| {
        categories.write().retain(|c| c != &cat);
    };

    let handle_publish = move |_| {
        let title_val = title.read().trim().to_string();
        let description_val = description.read().trim().to_string();
        let image_url_val = image_url.read().trim().to_string();
        let is_public_val = *is_public.read();
        let is_collaborative_val = *is_collaborative.read();
        let categories_val = categories.read().clone();

        // Validation
        if title_val.is_empty() {
            error_msg.set(Some("Title is required".to_string()));
            return;
        }

        is_publishing.set(true);
        error_msg.set(None);

        spawn(async move {
            let description = if description_val.is_empty() { None } else { Some(description_val) };
            let image = if image_url_val.is_empty() { None } else { Some(image_url_val) };

            // Generate a unique d-tag based on title and timestamp
            let d_tag = format!("{}-{}",
                slugify(&title_val),
                chrono::Utc::now().timestamp()
            );

            match nostr_music::publish_playlist(
                d_tag,
                title_val,
                description,
                image,
                Vec::new(), // Empty track list initially - user can add tracks later
                categories_val,
                is_public_val,
                is_collaborative_val,
            ).await {
                Ok(_event_id) => {
                    navigator.push(crate::routes::Route::MusicHome {});
                }
                Err(e) => {
                    error_msg.set(Some(format!("Failed to create playlist: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "max-w-2xl mx-auto p-6",

            // Header
            div {
                class: "flex items-center gap-4 mb-8",
                Link {
                    to: Route::MusicHome {},
                    class: "p-2 hover:bg-muted rounded-full transition",
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "w-5 h-5",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M10 19l-7-7m0 0l7-7m-7 7h18"
                        }
                    }
                }
                h1 {
                    class: "text-2xl font-bold",
                    "Create Playlist"
                }
            }

            // Form
            div {
                class: "space-y-6",

                // Error message
                if let Some(err) = error_msg.read().clone() {
                    div {
                        class: "p-4 bg-destructive/10 border border-destructive rounded-lg text-destructive text-sm",
                        "{err}"
                    }
                }

                // Title
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Title *"
                    }
                    input {
                        r#type: "text",
                        placeholder: "My Awesome Playlist",
                        class: "w-full px-4 py-3 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary",
                        value: "{title}",
                        oninput: move |e| title.set(e.value())
                    }
                }

                // Description
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Description"
                    }
                    textarea {
                        placeholder: "What's this playlist about?",
                        rows: "3",
                        class: "w-full px-4 py-3 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary resize-none",
                        value: "{description}",
                        oninput: move |e| description.set(e.value())
                    }
                }

                // Cover Image URL
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Cover Image URL"
                    }
                    input {
                        r#type: "url",
                        placeholder: "https://example.com/cover.jpg",
                        class: "w-full px-4 py-3 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary",
                        value: "{image_url}",
                        oninput: move |e| image_url.set(e.value())
                    }
                }

                // Categories
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Categories"
                    }
                    div {
                        class: "flex gap-2 mb-2",
                        input {
                            r#type: "text",
                            placeholder: "Add a category",
                            class: "flex-1 px-4 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary",
                            value: "{category_input}",
                            oninput: move |e| category_input.set(e.value()),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    do_add_category();
                                }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "px-4 py-2 bg-secondary text-secondary-foreground rounded-lg hover:bg-secondary/90 transition",
                            onclick: move |_| do_add_category(),
                            "Add"
                        }
                    }
                    if !categories.read().is_empty() {
                        div {
                            class: "flex flex-wrap gap-2",
                            for cat in categories.read().iter() {
                                {
                                    let cat_clone = cat.clone();
                                    rsx! {
                                        span {
                                            key: "{cat}",
                                            class: "px-3 py-1 bg-muted rounded-full text-sm flex items-center gap-2",
                                            "{cat}"
                                            button {
                                                r#type: "button",
                                                class: "hover:text-destructive transition",
                                                onclick: move |_| remove_category(cat_clone.clone()),
                                                "x"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Visibility options
                div {
                    class: "space-y-3",
                    h3 {
                        class: "text-sm font-medium",
                        "Visibility"
                    }

                    div {
                        class: "flex items-center gap-3",
                        input {
                            r#type: "checkbox",
                            id: "is-public",
                            class: "w-5 h-5 rounded border-border",
                            checked: *is_public.read(),
                            onchange: move |e| is_public.set(e.checked())
                        }
                        label {
                            r#for: "is-public",
                            class: "text-sm",
                            "Public playlist"
                        }
                    }

                    div {
                        class: "flex items-center gap-3",
                        input {
                            r#type: "checkbox",
                            id: "is-collaborative",
                            class: "w-5 h-5 rounded border-border",
                            checked: *is_collaborative.read(),
                            onchange: move |e| is_collaborative.set(e.checked())
                        }
                        label {
                            r#for: "is-collaborative",
                            class: "text-sm",
                            "Allow others to add tracks (collaborative)"
                        }
                    }
                }

                // Info box
                div {
                    class: "p-4 bg-muted rounded-lg text-sm text-muted-foreground",
                    p {
                        "You can add tracks to your playlist after creating it. Visit the playlist page to add tracks from your library or discover new ones."
                    }
                }

                // Publish button
                div {
                    class: "pt-4",
                    button {
                        r#type: "button",
                        class: "w-full px-6 py-3 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: *is_publishing.read(),
                        onclick: handle_publish,
                        if *is_publishing.read() {
                            "Creating..."
                        } else {
                            "Create Playlist"
                        }
                    }
                }
            }
        }
    }
}
