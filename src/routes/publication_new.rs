//! Publication New/Edit Route
//! Create NKBIP-01 publications (Kind 30040/30041)

use crate::components::icons::{AlertTriangleIcon, ArrowLeftIcon, CheckIcon, PenSquareIcon};
use crate::routes::Route;
use crate::stores::publication_store::{self, PublicationType};
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;

/// Publication creator/editor
#[component]
pub fn PublicationNew() -> Element {
    let nav = use_navigator();
    let auth = auth_store::AUTH_STATE.read();

    // Publication metadata
    let mut title = use_signal(String::new);
    let mut identifier = use_signal(String::new);
    let mut summary = use_signal(String::new);
    let mut author_name = use_signal(String::new);
    let mut cover_image = use_signal(String::new);
    let mut pub_type = use_signal(|| PublicationType::Book);
    let mut auto_identifier = use_signal(|| true);

    // Section management
    let mut sections = use_signal(Vec::<SectionDraft>::new);
    let mut current_section_idx = use_signal(|| None::<usize>);
    let mut show_preview = use_signal(|| false);

    // Saving state
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    // Redirect if not logged in
    if auth.pubkey.is_none() {
        return rsx! {
            div {
                class: "max-w-4xl mx-auto px-4 py-16 text-center",
                AlertTriangleIcon { class: "w-16 h-16 text-muted-foreground mx-auto mb-4" }
                h2 {
                    class: "text-xl font-semibold text-foreground mb-2",
                    "Login Required"
                }
                p {
                    class: "text-muted-foreground mb-6",
                    "You need to be logged in to create publications."
                }
                Link {
                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                    to: Route::PublicationsHome {},
                    "Back to Publications"
                }
            }
        };
    }

    // Auto-generate identifier from title
    use_effect(move || {
        if *auto_identifier.read() {
            let normalized = title
                .read()
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() {
                        c.to_lowercase().next().unwrap()
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
                .split('-')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("-");
            identifier.set(normalized);
        }
    });

    let go_back = move |_| {
        nav.push(Route::PublicationsHome {});
    };

    let toggle_preview = move |_| {
        let current = *show_preview.read();
        show_preview.set(!current);
    };

    let add_section = move |_| {
        let mut secs = sections.read().clone();
        secs.push(SectionDraft::default());
        sections.set(secs);
        current_section_idx.set(Some(sections.read().len() - 1));
    };

    let handle_submit = move |_| {
        if !*nostr_client::CLIENT_INITIALIZED.read() {
            error.set(Some("Client not initialized yet".to_string()));
            return;
        }

        let title_val = title.read().clone();
        let identifier_val = identifier.read().clone();

        // Validation
        if title_val.trim().is_empty() {
            error.set(Some("Title is required".to_string()));
            return;
        }
        if identifier_val.trim().is_empty() {
            error.set(Some("Identifier is required".to_string()));
            return;
        }
        if sections.read().is_empty() {
            error.set(Some("Add at least one section".to_string()));
            return;
        }

        let summary_val = summary.read().clone();
        let cover_val = cover_image.read().clone();
        let type_val = pub_type.read().clone();
        let sections_val = sections.read().clone();

        spawn(async move {
            saving.set(true);
            error.set(None);

            let summary_opt = if summary_val.trim().is_empty() {
                None
            } else {
                Some(summary_val.as_str())
            };
            let cover_opt = if cover_val.trim().is_empty() {
                None
            } else {
                Some(cover_val.as_str())
            };

            // First publish all sections and collect their references
            let mut section_refs = Vec::new();
            for (idx, section) in sections_val.iter().enumerate() {
                match publication_store::publish_publication_section(
                    &section.title,
                    &section.content,
                    Some(&format!("{}-{}", identifier_val, idx)),
                )
                .await
                {
                    Ok(address) => {
                        // Parse address to create SectionReference
                        section_refs.push(publication_store::SectionReference {
                            address,
                            relay_hint: None,
                            event_id: None,
                        });
                    }
                    Err(e) => {
                        error.set(Some(format!(
                            "Failed to publish section {}: {}",
                            idx + 1,
                            e
                        )));
                        saving.set(false);
                        return;
                    }
                }
            }

            // Then publish the index
            match publication_store::publish_publication_index(
                &title_val,
                summary_opt,
                cover_opt,
                type_val,
                &[], // Empty topics for now
                &section_refs,
            )
            .await
            {
                Ok(naddr) => {
                    log::info!("Published publication: {}", naddr);
                    nav.push(Route::PublicationDetail { naddr });
                }
                Err(e) => {
                    error.set(Some(e));
                    saving.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "max-w-4xl mx-auto px-4 py-6",

            // Header
            div {
                class: "flex items-center justify-between mb-6",
                button {
                    class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: go_back,
                    ArrowLeftIcon { class: "w-5 h-5" }
                    "Back"
                }
                div {
                    class: "flex items-center gap-2",
                    button {
                        class: format!(
                            "flex items-center gap-2 px-3 py-1.5 text-sm rounded-md transition-colors {}",
                            if *show_preview.read() { "bg-primary text-primary-foreground" } else { "bg-accent hover:bg-accent/80" }
                        ),
                        onclick: toggle_preview,
                        if *show_preview.read() {
                            PenSquareIcon { class: "w-4 h-4" }
                            "Edit"
                        } else {
                            "Preview"
                        }
                    }
                    button {
                        class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors disabled:opacity-50",
                        disabled: *saving.read(),
                        onclick: handle_submit,
                        CheckIcon { class: "w-5 h-5" }
                        if *saving.read() { "Publishing..." } else { "Publish" }
                    }
                }
            }

            // Error display
            if let Some(ref e) = *error.read() {
                div {
                    class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                    "{e}"
                }
            }

            // Form
            div {
                class: "space-y-6",

                // Title
                div {
                    label {
                        class: "block text-sm font-medium text-foreground mb-2",
                        "Title"
                    }
                    input {
                        class: "w-full px-4 py-2 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring",
                        r#type: "text",
                        placeholder: "Publication title",
                        value: "{title}",
                        oninput: move |e| title.set(e.value().clone()),
                    }
                }

                // Identifier
                div {
                    label {
                        class: "block text-sm font-medium text-foreground mb-2",
                        "Identifier (d-tag)"
                    }
                    input {
                        class: "w-full px-4 py-2 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring font-mono text-sm",
                        r#type: "text",
                        placeholder: "publication-identifier",
                        value: "{identifier}",
                        oninput: move |e| {
                            auto_identifier.set(false);
                            identifier.set(e.value().clone());
                        },
                    }
                }

                // Two-column row
                div {
                    class: "grid grid-cols-1 md:grid-cols-2 gap-4",

                    // Author
                    div {
                        label {
                            class: "block text-sm font-medium text-foreground mb-2",
                            "Author Name (optional)"
                        }
                        input {
                            class: "w-full px-4 py-2 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring",
                            r#type: "text",
                            placeholder: "Author name",
                            value: "{author_name}",
                            oninput: move |e| author_name.set(e.value().clone()),
                        }
                    }

                    // Type
                    div {
                        label {
                            class: "block text-sm font-medium text-foreground mb-2",
                            "Publication Type"
                        }
                        select {
                            class: "w-full px-4 py-2 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring",
                            value: "{pub_type.read().as_str()}",
                            onchange: move |e| {
                                pub_type.set(match e.value().as_str() {
                                    "book" => PublicationType::Book,
                                    "illustrated" => PublicationType::Illustrated,
                                    "magazine" => PublicationType::Magazine,
                                    "documentation" => PublicationType::Documentation,
                                    "academic" => PublicationType::Academic,
                                    "blog" => PublicationType::Blog,
                                    _ => PublicationType::Book,
                                });
                            },
                            option { value: "book", "Book" }
                            option { value: "illustrated", "Illustrated" }
                            option { value: "magazine", "Magazine" }
                            option { value: "documentation", "Documentation" }
                            option { value: "academic", "Academic" }
                            option { value: "blog", "Blog" }
                        }
                    }
                }

                // Summary
                div {
                    label {
                        class: "block text-sm font-medium text-foreground mb-2",
                        "Summary (optional)"
                    }
                    textarea {
                        class: "w-full h-24 px-4 py-3 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring resize-y",
                        placeholder: "Brief description of the publication",
                        value: "{summary}",
                        oninput: move |e| summary.set(e.value().clone()),
                    }
                }

                // Cover image URL
                div {
                    label {
                        class: "block text-sm font-medium text-foreground mb-2",
                        "Cover Image URL (optional)"
                    }
                    input {
                        class: "w-full px-4 py-2 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring",
                        r#type: "url",
                        placeholder: "https://example.com/cover.jpg",
                        value: "{cover_image}",
                        oninput: move |e| cover_image.set(e.value().clone()),
                    }
                }

                // Sections
                div {
                    class: "border-t border-border pt-6",
                    div {
                        class: "flex items-center justify-between mb-4",
                        h3 {
                            class: "text-lg font-semibold text-foreground",
                            "Sections"
                        }
                        button {
                            class: "px-3 py-1.5 text-sm bg-accent hover:bg-accent/80 rounded-md transition-colors",
                            onclick: add_section,
                            "+ Add Section"
                        }
                    }

                    if sections.read().is_empty() {
                        div {
                            class: "text-center py-8 text-muted-foreground border border-dashed border-border rounded-lg",
                            "No sections yet. Add sections to build your publication."
                        }
                    } else {
                        div {
                            class: "space-y-4",
                            for (idx, section) in sections.read().iter().enumerate() {
                                SectionEditor {
                                    key: "{idx}",
                                    index: idx,
                                    section: section.clone(),
                                    on_update: {
                                        let mut sections = sections;
                                        move |(idx, updated): (usize, SectionDraft)| {
                                            let mut secs = sections.read().clone();
                                            if idx < secs.len() {
                                                secs[idx] = updated;
                                                sections.set(secs);
                                            }
                                        }
                                    },
                                    on_remove: {
                                        let mut sections = sections;
                                        move |idx: usize| {
                                            let mut secs = sections.read().clone();
                                            if idx < secs.len() {
                                                secs.remove(idx);
                                                sections.set(secs);
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Section draft for editing
#[derive(Clone, Debug, Default, PartialEq)]
struct SectionDraft {
    title: String,
    content: String,
}

/// Section editor component
#[component]
fn SectionEditor(
    index: usize,
    section: SectionDraft,
    on_update: EventHandler<(usize, SectionDraft)>,
    on_remove: EventHandler<usize>,
) -> Element {
    let mut expanded = use_signal(|| true);

    rsx! {
        div {
            class: "border border-border rounded-lg overflow-hidden",

            // Section header
            div {
                class: "flex items-center justify-between px-4 py-2 bg-accent/50",
                button {
                    class: "flex items-center gap-2 text-sm font-medium text-foreground",
                    onclick: move |_| {
                        let current = *expanded.read();
                        expanded.set(!current);
                    },
                    span { class: "text-muted-foreground", "#{index + 1}" }
                    if section.title.is_empty() {
                        span { class: "text-muted-foreground italic", "Untitled Section" }
                    } else {
                        span { "{section.title}" }
                    }
                }
                button {
                    class: "text-sm text-destructive hover:text-destructive/80",
                    onclick: move |_| on_remove.call(index),
                    "Remove"
                }
            }

            // Section content (expandable)
            if *expanded.read() {
                div {
                    class: "p-4 space-y-4",

                    // Section title
                    div {
                        label {
                            class: "block text-sm font-medium text-foreground mb-2",
                            "Section Title"
                        }
                        input {
                            class: "w-full px-3 py-2 bg-background border border-input rounded-md focus:outline-hidden focus:ring-2 focus:ring-ring",
                            r#type: "text",
                            placeholder: "Chapter title",
                            value: "{section.title}",
                            oninput: {
                                let section = section.clone();
                                move |e| {
                                    on_update.call((index, SectionDraft {
                                        title: e.value().clone(),
                                        content: section.content.clone(),
                                    }));
                                }
                            },
                        }
                    }

                    // Section content
                    div {
                        label {
                            class: "block text-sm font-medium text-foreground mb-2",
                            "Content (AsciiDoc)"
                        }
                        textarea {
                            class: "w-full h-48 px-3 py-2 bg-background border border-input rounded-md focus:outline-hidden focus:ring-2 focus:ring-ring font-mono text-sm resize-y",
                            placeholder: "Section content in AsciiDoc format...",
                            value: "{section.content}",
                            oninput: {
                                let section = section.clone();
                                move |e| {
                                    on_update.call((index, SectionDraft {
                                        title: section.title.clone(),
                                        content: e.value().clone(),
                                    }));
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
