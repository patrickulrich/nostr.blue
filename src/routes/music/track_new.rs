// Track Publishing Page
// Allows users to publish Kind 36787 music tracks to nostr

use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_music};

#[component]
pub fn MusicTrackNew() -> Element {
    let navigator = navigator();
    let is_authenticated = auth_store::is_authenticated();

    // Form state
    let mut title = use_signal(String::new);
    let mut audio_url = use_signal(String::new);
    let mut image_url = use_signal(String::new);
    let mut duration = use_signal(|| None::<u32>);
    let mut genres = use_signal(|| Vec::<String>::new());
    let mut genre_input = use_signal(String::new);
    let mut ai_generated = use_signal(|| false);
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
                        "You need to sign in to publish tracks."
                    }
                    a {
                        href: "/",
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                        "Go Home"
                    }
                }
            }
        };
    }

    let mut do_add_genre = move || {
        let genre = genre_input.read().trim().to_string();
        if !genre.is_empty() && !genres.read().contains(&genre) {
            genres.write().push(genre);
            genre_input.set(String::new());
        }
    };

    let mut remove_genre = move |genre: String| {
        genres.write().retain(|g| g != &genre);
    };

    let handle_publish = move |_| {
        let title_val = title.read().trim().to_string();
        let audio_url_val = audio_url.read().trim().to_string();
        let image_url_val = image_url.read().trim().to_string();
        let duration_val = *duration.read();
        let genres_val = genres.read().clone();
        let ai_generated_val = *ai_generated.read();

        // Validation
        if title_val.is_empty() {
            error_msg.set(Some("Title is required".to_string()));
            return;
        }
        if audio_url_val.is_empty() {
            error_msg.set(Some("Audio URL is required".to_string()));
            return;
        }

        is_publishing.set(true);
        error_msg.set(None);

        spawn(async move {
            let image = if image_url_val.is_empty() { None } else { Some(image_url_val) };

            // Generate a unique d-tag based on title and timestamp
            let d_tag = format!("{}-{}",
                title_val.to_lowercase().replace(' ', "-"),
                chrono::Utc::now().timestamp()
            );

            match nostr_music::publish_track(
                d_tag,
                title_val,
                audio_url_val,
                image,
                None, // gradient
                duration_val,
                genres_val,
                ai_generated_val,
            ).await {
                Ok(_event_id) => {
                    navigator.push(crate::routes::Route::MusicHome {});
                }
                Err(e) => {
                    error_msg.set(Some(format!("Failed to publish: {}", e)));
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
                a {
                    href: "/music",
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
                    "Publish Track"
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
                        placeholder: "My Awesome Track",
                        class: "w-full px-4 py-3 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary",
                        value: "{title}",
                        oninput: move |e| title.set(e.value())
                    }
                }

                // Audio URL
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Audio URL *"
                    }
                    input {
                        r#type: "url",
                        placeholder: "https://example.com/track.mp3",
                        class: "w-full px-4 py-3 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary",
                        value: "{audio_url}",
                        oninput: move |e| audio_url.set(e.value())
                    }
                    p {
                        class: "text-xs text-muted-foreground mt-1",
                        "Direct link to your audio file (MP3, WAV, etc.)"
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

                // Duration
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Duration (seconds)"
                    }
                    input {
                        r#type: "number",
                        placeholder: "180",
                        min: "1",
                        class: "w-full px-4 py-3 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary",
                        value: if let Some(d) = *duration.read() { format!("{}", d) } else { String::new() },
                        oninput: move |e| {
                            let val = e.value().parse::<u32>().ok();
                            duration.set(val);
                        }
                    }
                }

                // Genres
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Genres"
                    }
                    div {
                        class: "flex gap-2 mb-2",
                        input {
                            r#type: "text",
                            placeholder: "Add a genre",
                            class: "flex-1 px-4 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary",
                            value: "{genre_input}",
                            oninput: move |e| genre_input.set(e.value()),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    do_add_genre();
                                }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "px-4 py-2 bg-secondary text-secondary-foreground rounded-lg hover:bg-secondary/90 transition",
                            onclick: move |_| do_add_genre(),
                            "Add"
                        }
                    }
                    if !genres.read().is_empty() {
                        div {
                            class: "flex flex-wrap gap-2",
                            for genre in genres.read().iter() {
                                {
                                    let genre_clone = genre.clone();
                                    rsx! {
                                        span {
                                            key: "{genre}",
                                            class: "px-3 py-1 bg-muted rounded-full text-sm flex items-center gap-2",
                                            "{genre}"
                                            button {
                                                r#type: "button",
                                                class: "hover:text-destructive transition",
                                                onclick: move |_| remove_genre(genre_clone.clone()),
                                                "x"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // AI Generated toggle
                div {
                    class: "flex items-center gap-3",
                    input {
                        r#type: "checkbox",
                        id: "ai-generated",
                        class: "w-5 h-5 rounded border-border",
                        checked: *ai_generated.read(),
                        onchange: move |e| ai_generated.set(e.checked())
                    }
                    label {
                        r#for: "ai-generated",
                        class: "text-sm",
                        "This track was AI-generated"
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
                            "Publishing..."
                        } else {
                            "Publish Track"
                        }
                    }
                }
            }
        }
    }
}
