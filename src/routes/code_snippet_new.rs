//! New Code Snippet Page
//!
//! Create a new NIP-C0 code snippet (Kind 1337).

use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::publish_snippet;
use crate::stores::auth_store;
use dioxus::prelude::*;

/// Popular programming languages for the dropdown
const LANGUAGES: &[(&str, &str)] = &[
    ("rust", "Rust"),
    ("javascript", "JavaScript"),
    ("typescript", "TypeScript"),
    ("python", "Python"),
    ("go", "Go"),
    ("java", "Java"),
    ("c", "C"),
    ("cpp", "C++"),
    ("csharp", "C#"),
    ("ruby", "Ruby"),
    ("php", "PHP"),
    ("swift", "Swift"),
    ("kotlin", "Kotlin"),
    ("scala", "Scala"),
    ("bash", "Bash/Shell"),
    ("sql", "SQL"),
    ("html", "HTML"),
    ("css", "CSS"),
    ("json", "JSON"),
    ("yaml", "YAML"),
    ("toml", "TOML"),
    ("markdown", "Markdown"),
    ("other", "Other"),
];

/// New code snippet page component
#[component]
pub fn CodeSnippetNew() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let nav = use_navigator();

    // Form state
    let mut code = use_signal(String::new);
    let mut language = use_signal(|| "rust".to_string());
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut extension = use_signal(String::new);
    let mut dependencies = use_signal(String::new);
    let mut license = use_signal(String::new);
    let mut repo_url = use_signal(String::new);

    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut show_advanced = use_signal(|| false);

    // Check authentication
    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState {}
        };
    }

    let handle_submit = move |_| {
        let code_val = code.read().clone();
        let lang_val = language.read().clone();
        let name_val = name.read().clone();
        let desc_val = description.read().clone();
        let ext_val = extension.read().clone();
        let deps_val = dependencies.read().clone();
        let license_val = license.read().clone();
        let repo_val = repo_url.read().clone();

        spawn(async move {
            // Validate
            if code_val.trim().is_empty() {
                error_message.set(Some("Please enter some code".to_string()));
                return;
            }

            is_publishing.set(true);
            error_message.set(None);

            // Convert dependencies to Vec<&str>
            let deps_list: Vec<&str> = if deps_val.is_empty() {
                vec![]
            } else {
                deps_val.split(',').map(|s| s.trim()).collect()
            };

            match publish_snippet(
                &code_val,
                if lang_val.is_empty() {
                    None
                } else {
                    Some(lang_val.as_str())
                },
                if name_val.is_empty() {
                    None
                } else {
                    Some(name_val.as_str())
                },
                if ext_val.is_empty() {
                    None
                } else {
                    Some(ext_val.as_str())
                },
                if desc_val.is_empty() {
                    None
                } else {
                    Some(desc_val.as_str())
                },
                None, // runtime
                if license_val.is_empty() {
                    None
                } else {
                    Some(license_val.as_str())
                },
                &deps_list,
                if repo_val.is_empty() {
                    None
                } else {
                    Some(repo_val.as_str())
                },
            )
            .await
            {
                Ok(event_id) => {
                    // Navigate to the new snippet
                    nav.push(Route::CodeSnippetDetail {
                        note_id: event_id.to_hex(),
                    });
                }
                Err(e) => {
                    error_message.set(Some(e));
                    is_publishing.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "p-4 flex items-center justify-between",
                    div {
                        class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeSnippets {},
                            class: "text-muted-foreground hover:text-foreground",
                            dangerous_inner_html: icons::ARROW_LEFT
                        }
                        h1 {
                            class: "text-xl font-bold flex items-center gap-2",
                            svg {
                                class: "w-5 h-5",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                polyline { points: "16 18 22 12 16 6" }
                                polyline { points: "8 6 2 12 8 18" }
                            }
                            "New Snippet"
                        }
                    }

                    // Publish button
                    button {
                        class: "px-4 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                        disabled: *is_publishing.read(),
                        onclick: handle_submit,
                        if *is_publishing.read() {
                            svg {
                                class: "w-4 h-4 animate-spin",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                circle {
                                    class: "opacity-25",
                                    cx: "12",
                                    cy: "12",
                                    r: "10",
                                    stroke: "currentColor",
                                    stroke_width: "4"
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                                }
                            }
                            "Publishing..."
                        } else {
                            "Publish"
                        }
                    }
                }
            }

            // Content
            div {
                class: "p-4 space-y-6",

                // Error message
                if let Some(error) = error_message.read().as_ref() {
                    div {
                        class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }

                // NIP-C0 info
                div {
                    class: "p-4 bg-green-500/10 rounded-lg border border-green-500/20",
                    div {
                        class: "flex items-start gap-3",
                        div {
                            class: "w-8 h-8 rounded-lg bg-green-500/20 flex items-center justify-center shrink-0",
                            svg {
                                class: "w-4 h-4 text-green-500",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                circle { cx: "12", cy: "12", r: "10" }
                                path { d: "M12 16v-4" }
                                path { d: "M12 8h.01" }
                            }
                        }
                        div {
                            p {
                                class: "text-sm",
                                span { class: "font-medium", "NIP-C0 Code Snippet" }
                                span { class: "text-muted-foreground", " - Your snippet will be published as a Kind 1337 event. It will be publicly visible and can be shared via nostr: links." }
                            }
                        }
                    }
                }

                // Language selector
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Language"
                    }
                    select {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        value: "{language}",
                        onchange: move |e| language.set(e.value()),
                        for (value, label) in LANGUAGES.iter() {
                            option {
                                value: "{value}",
                                "{label}"
                            }
                        }
                    }
                }

                // Name (optional)
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Name "
                        span { class: "text-muted-foreground font-normal", "(optional)" }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "e.g., hello_world.rs",
                        value: "{name}",
                        oninput: move |e| name.set(e.value())
                    }
                }

                // Description (optional)
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Description "
                        span { class: "text-muted-foreground font-normal", "(optional)" }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "A brief description of what this code does",
                        value: "{description}",
                        oninput: move |e| description.set(e.value())
                    }
                }

                // Code editor
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Code "
                        span { class: "text-destructive", "*" }
                    }
                    textarea {
                        class: "w-full h-64 px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary resize-y",
                        placeholder: "Paste or type your code here...",
                        value: "{code}",
                        oninput: move |e| code.set(e.value())
                    }
                    p {
                        class: "text-xs text-muted-foreground mt-1",
                        "Tip: You can paste formatted code from your editor"
                    }
                }

                // Advanced options toggle
                {
                    let is_advanced = *show_advanced.read();
                    rsx! {
                        button {
                            class: "flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| show_advanced.set(!is_advanced),
                            svg {
                                class: if is_advanced { "w-4 h-4 transform rotate-90 transition" } else { "w-4 h-4 transition" },
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                                polyline { points: "9 18 15 12 9 6" }
                            }
                            "Advanced Options"
                        }
                    }
                }

                // Advanced options
                if *show_advanced.read() {
                    div {
                        class: "space-y-4 pl-4 border-l-2 border-border",

                        // File extension
                        div {
                            label {
                                class: "block text-sm font-medium mb-2",
                                "File Extension"
                            }
                            input {
                                class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "e.g., rs, js, py",
                                value: "{extension}",
                                oninput: move |e| extension.set(e.value())
                            }
                        }

                        // Dependencies
                        div {
                            label {
                                class: "block text-sm font-medium mb-2",
                                "Dependencies"
                            }
                            input {
                                class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "e.g., tokio, serde",
                                value: "{dependencies}",
                                oninput: move |e| dependencies.set(e.value())
                            }
                            p {
                                class: "text-xs text-muted-foreground mt-1",
                                "Comma-separated list of required packages"
                            }
                        }

                        // License
                        div {
                            label {
                                class: "block text-sm font-medium mb-2",
                                "License"
                            }
                            input {
                                class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "e.g., MIT, Apache-2.0, GPL-3.0",
                                value: "{license}",
                                oninput: move |e| license.set(e.value())
                            }
                        }

                        // Repository URL
                        div {
                            label {
                                class: "block text-sm font-medium mb-2",
                                "Repository URL"
                            }
                            input {
                                class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "url",
                                placeholder: "https://github.com/user/repo",
                                value: "{repo_url}",
                                oninput: move |e| repo_url.set(e.value())
                            }
                            p {
                                class: "text-xs text-muted-foreground mt-1",
                                "Link to the full repository if this snippet is part of a larger project"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NotAuthenticatedState() -> Element {
    rsx! {
        div {
            class: "min-h-screen flex items-center justify-center p-4",
            div {
                class: "text-center max-w-md",
                div {
                    class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
                    svg {
                        class: "w-10 h-10 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                        circle { cx: "12", cy: "7", r: "4" }
                    }
                }
                h2 {
                    class: "font-semibold text-xl mb-2",
                    "Sign In Required"
                }
                p {
                    class: "text-muted-foreground mb-6",
                    "Connect with your Nostr identity to create code snippets."
                }
                Link {
                    to: Route::CodeSnippets {},
                    class: "text-primary hover:underline",
                    "← Back to Snippets"
                }
            }
        }
    }
}
