//! Citation Editor Modal
//! Create and edit NKBIP-03 citations (kinds 30-33)
use crate::components::icons::{BookOpenIcon, GlobeIcon, Link2Icon, SettingsIcon, XIcon};
use crate::stores::auth_store;
use crate::stores::citation_store::{
    fetch_citations_by_author, publish_external_citation, publish_hardcopy_citation,
    publish_internal_citation, publish_prompt_citation, CachedCitation,
};
use crate::utils::is_valid_http_url;
use crate::utils::nkbip03::Citation;
use dioxus::prelude::*;
/// Tab for citation type selection
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum CitationEditorTab {
    #[default]
    Internal,
    External,
    Hardcopy,
    Prompt,
}
impl CitationEditorTab {
    fn label(&self) -> &'static str {
        match self {
            CitationEditorTab::Internal => "Nostr",
            CitationEditorTab::External => "Web",
            CitationEditorTab::Hardcopy => "Book",
            CitationEditorTab::Prompt => "AI",
        }
    }
    fn description(&self) -> &'static str {
        match self {
            CitationEditorTab::Internal => "Reference another Nostr event",
            CitationEditorTab::External => "Cite a web page or article",
            CitationEditorTab::Hardcopy => "Cite a book or journal",
            CitationEditorTab::Prompt => "Cite an AI conversation",
        }
    }
}
#[derive(Props, Clone, PartialEq)]
pub struct CitationEditorModalProps {
    /// Signal controlling modal visibility
    pub show: Signal<bool>,
    /// Optional citation to edit (None = create new)
    #[props(default = None)]
    pub citation_to_edit: Option<CachedCitation>,
    /// Callback when citation is saved (returns naddr)
    #[props(default = None)]
    pub on_save: Option<EventHandler<String>>,
}
#[component]
pub fn CitationEditorModal(mut props: CitationEditorModalProps) -> Element {
    let mut active_tab = use_signal(|| CitationEditorTab::Internal);
    let mut title = use_signal(String::new);
    let mut author = use_signal(String::new);
    let mut cited_text = use_signal(String::new);
    let mut coordinate = use_signal(String::new);
    let mut url = use_signal(String::new);
    let mut page_range = use_signal(String::new);
    let mut publisher = use_signal(String::new);
    let mut doi = use_signal(String::new);
    let mut llm = use_signal(String::new);
    let mut conversation_summary = use_signal(String::new);
    let mut prompt_url = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut session_token = use_signal(|| 0u64);
    let mut save_request_id = use_signal(|| 0u64);
    let is_editing = props.citation_to_edit.is_some();
    use_effect(use_reactive(
        (&*props.show.read(), &props.citation_to_edit),
        move |(is_shown, citation)| {
            if is_shown {
                // Clear all fields first to prevent stale data when switching tabs
                title.set(String::new());
                author.set(String::new());
                cited_text.set(String::new());
                coordinate.set(String::new());
                url.set(String::new());
                page_range.set(String::new());
                publisher.set(String::new());
                doi.set(String::new());
                llm.set(String::new());
                conversation_summary.set(String::new());
                prompt_url.set(String::new());

                if let Some(ref cached) = citation {
                    // Populate fields for the specific citation type being edited
                    match &cached.citation {
                        Citation::Internal(c) => {
                            active_tab.set(CitationEditorTab::Internal);
                            coordinate.set(c.coordinate.clone());
                            title.set(c.base.title.clone());
                            author.set(c.base.author.clone());
                            cited_text.set(c.base.content.clone());
                        }
                        Citation::ExternalWeb(c) => {
                            active_tab.set(CitationEditorTab::External);
                            url.set(c.url.clone());
                            title.set(c.base.title.clone());
                            author.set(c.base.author.clone());
                            cited_text.set(c.base.content.clone());
                        }
                        Citation::Hardcopy(c) => {
                            active_tab.set(CitationEditorTab::Hardcopy);
                            title.set(c.base.title.clone());
                            author.set(c.base.author.clone());
                            cited_text.set(c.base.content.clone());
                            page_range.set(c.page_range.clone().unwrap_or_default());
                            publisher.set(c.published_by.clone().unwrap_or_default());
                            doi.set(c.doi.clone().unwrap_or_default());
                        }
                        Citation::Prompt(c) => {
                            active_tab.set(CitationEditorTab::Prompt);
                            llm.set(c.llm.clone());
                            title.set(c.base.title.clone());
                            author.set(c.base.author.clone());
                            cited_text.set(c.base.content.clone());
                            conversation_summary.set(c.base.summary.clone().unwrap_or_default());
                            prompt_url.set(c.url.clone().unwrap_or_default());
                        }
                    }
                } else {
                    active_tab.set(CitationEditorTab::Internal);
                }
                error.set(None);
            }
        },
    ));
    use_effect(use_reactive(&*props.show.read(), move |is_shown| {
        if is_shown {
            let new_token = *session_token.peek() + 1;
            session_token.set(new_token);
        }
    }));
    let close_modal = move |_| {
        props.show.set(false);
    };
    let is_valid = use_memo(move || {
        let has_cited_text = !cited_text.read().trim().is_empty();
        match *active_tab.read() {
            CitationEditorTab::Internal => has_cited_text && !coordinate.read().trim().is_empty(),
            CitationEditorTab::External => has_cited_text && is_valid_http_url(url.read().trim()),
            CitationEditorTab::Hardcopy => {
                has_cited_text
                    && !title.read().trim().is_empty()
                    && !author.read().trim().is_empty()
            }
            CitationEditorTab::Prompt => {
                let prompt_url_str = prompt_url.read();
                let prompt_url_valid =
                    prompt_url_str.trim().is_empty() || is_valid_http_url(prompt_url_str.trim());
                has_cited_text && !llm.read().trim().is_empty() && prompt_url_valid
            }
        }
    });
    let handle_save = move |_| {
        if !*is_valid.read() {
            return;
        }
        saving.set(true);
        error.set(None);
        let current_tab = *active_tab.read();
        let title_val = title.read().trim().to_string();
        let author_val = author.read().trim().to_string();
        let cited_text_val = cited_text.read().clone();
        let coordinate_val = coordinate.read().clone();
        let url_val = url.read().trim().to_string();
        let page_range_val = page_range.read().trim().to_string();
        let publisher_val = publisher.read().trim().to_string();
        let doi_val = doi.read().trim().to_string();
        let llm_val = llm.read().trim().to_string();
        let summary_val = conversation_summary.read().trim().to_string();
        let prompt_url_val = prompt_url.read().trim().to_string();
        let on_save = props.on_save;
        let mut show = props.show;
        let save_request = save_request_id.with_mut(|request_id| {
            *request_id = request_id.wrapping_add(1);
            *request_id
        });
        // Compute fresh from current props
        let existing_d_tag = props
            .citation_to_edit
            .as_ref()
            .and_then(|c| c.event.tags.identifier().map(|s| s.to_string()));
        let my_token = *session_token.read();
        spawn(async move {
            let result = match current_tab {
                CitationEditorTab::Internal => {
                    publish_internal_citation(
                        &coordinate_val,
                        &cited_text_val,
                        if title_val.is_empty() {
                            None
                        } else {
                            Some(&title_val)
                        },
                        if author_val.is_empty() {
                            None
                        } else {
                            Some(&author_val)
                        },
                        existing_d_tag.as_deref(),
                    )
                    .await
                }
                CitationEditorTab::External => {
                    publish_external_citation(
                        &url_val,
                        &cited_text_val,
                        if title_val.is_empty() {
                            None
                        } else {
                            Some(&title_val)
                        },
                        if author_val.is_empty() {
                            None
                        } else {
                            Some(&author_val)
                        },
                        existing_d_tag.as_deref(),
                    )
                    .await
                }
                CitationEditorTab::Hardcopy => {
                    publish_hardcopy_citation(
                        &title_val,
                        &author_val,
                        &cited_text_val,
                        if page_range_val.is_empty() {
                            None
                        } else {
                            Some(&page_range_val)
                        },
                        if publisher_val.is_empty() {
                            None
                        } else {
                            Some(&publisher_val)
                        },
                        if doi_val.is_empty() {
                            None
                        } else {
                            Some(&doi_val)
                        },
                        existing_d_tag.as_deref(),
                    )
                    .await
                }
                CitationEditorTab::Prompt => {
                    publish_prompt_citation(
                        &llm_val,
                        &cited_text_val,
                        if summary_val.is_empty() {
                            None
                        } else {
                            Some(&summary_val)
                        },
                        if prompt_url_val.is_empty() {
                            None
                        } else {
                            Some(&prompt_url_val)
                        },
                        if title_val.is_empty() {
                            None
                        } else {
                            Some(&title_val)
                        },
                        if author_val.is_empty() {
                            None
                        } else {
                            Some(&author_val)
                        },
                        existing_d_tag.as_deref(),
                    )
                    .await
                }
            };
            if *session_token.read() != my_token {
                if *save_request_id.peek() == save_request {
                    saving.set(false);
                }
                return;
            }
            match result {
                Ok(event_id) => {
                    log::info!("Citation published: {}", event_id);
                    if let Some(pk) = auth_store::get_pubkey() {
                        if let Err(e) = fetch_citations_by_author(&pk, 200).await {
                            crate::utils::log_fetch_error("citations refresh", e);
                        }
                    }
                    if *session_token.read() != my_token {
                        if *save_request_id.peek() == save_request {
                            saving.set(false);
                        }
                        return;
                    }
                    if let Some(ref handler) = on_save {
                        handler.call(event_id);
                    }
                    show.set(false);
                }
                Err(e) => {
                    log::error!("Failed to publish citation: {}", e);
                    error.set(Some(e));
                }
            }
            if *save_request_id.peek() == save_request {
                saving.set(false);
            }
        });
    };
    if !*props.show.read() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-center justify-center p-4",
            onclick: close_modal,
            div {
                class: "bg-background border border-border rounded-xl shadow-xl w-full max-w-lg max-h-[85vh] overflow-hidden flex flex-col",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between px-6 py-4 border-b border-border",
                    h2 { class: "text-lg font-semibold",
                        if is_editing {
                            "Edit Citation"
                        } else {
                            "New Citation"
                        }
                    }
                    button {
                        class: "p-2 text-muted-foreground hover:text-foreground rounded-lg hover:bg-accent transition-colors",
                        r#type: "button",
                        onclick: close_modal,
                        aria_label: "Close",
                        title: "Close",
                        XIcon { class: "w-5 h-5".to_string() }
                    }
                }
                div { class: "flex border-b border-border px-4",
                    {
                        [
                            CitationEditorTab::Internal,
                            CitationEditorTab::External,
                            CitationEditorTab::Hardcopy,
                            CitationEditorTab::Prompt,
                        ]
                            .iter()
                            .map(|tab| {
                                let is_active = *active_tab.read() == *tab;
                                let tab_clone = *tab;
                                rsx! {
                                    button {
                                        key: "{tab.label()}",
                                        class: if is_active { "flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 border-primary text-primary" } else { "flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 border-transparent text-muted-foreground hover:text-foreground transition-colors" },
                                        onclick: move |_| active_tab.set(tab_clone),
                                        {
                                            match tab {
                                                CitationEditorTab::Internal => rsx! {
                                                    Link2Icon { class: "w-4 h-4".to_string() }
                                                },
                                                CitationEditorTab::External => rsx! {
                                                    GlobeIcon { class: "w-4 h-4".to_string() }
                                                },
                                                CitationEditorTab::Hardcopy => rsx! {
                                                    BookOpenIcon { class: "w-4 h-4".to_string() }
                                                },
                                                CitationEditorTab::Prompt => rsx! {
                                                    SettingsIcon { class: "w-4 h-4".to_string() }
                                                },
                                            }
                                        }
                                        "{tab.label()}"
                                    }
                                }
                            })
                    }
                }
                div { class: "flex-1 overflow-y-auto p-6 space-y-4",
                    p { class: "text-sm text-muted-foreground mb-4",
                        "{active_tab.read().description()}"
                    }
                    match *active_tab.read() {
                        CitationEditorTab::Internal => rsx! {
                            div { class: "space-y-1.5",
                                label { class: "text-sm font-medium", "Nostr Address *" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                    placeholder: "naddr1... or nevent1...",
                                    value: "{coordinate}",
                                    oninput: move |e| coordinate.set(e.value()),
                                }
                                p { class: "text-xs text-muted-foreground",
                                    "The Nostr event you're citing (naddr, nevent, or note)"
                                }
                            }
                        },
                        CitationEditorTab::External => rsx! {
                            div { class: "space-y-1.5",
                                label { class: "text-sm font-medium", "URL *" }
                                input {
                                    r#type: "url",
                                    class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                    placeholder: "https://...",
                                    value: "{url}",
                                    oninput: move |e| url.set(e.value()),
                                }
                            }
                        },
                        CitationEditorTab::Hardcopy => rsx! {
                            div { class: "space-y-1.5",
                                label { class: "text-sm font-medium", "Publisher" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                    placeholder: "Publisher name",
                                    value: "{publisher}",
                                    oninput: move |e| publisher.set(e.value()),
                                }
                            }
                            div { class: "grid grid-cols-2 gap-3",
                                div { class: "space-y-1.5",
                                    label { class: "text-sm font-medium", "Page Range" }
                                    input {
                                        r#type: "text",
                                        class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                        placeholder: "pp. 42-56",
                                        value: "{page_range}",
                                        oninput: move |e| page_range.set(e.value()),
                                    }
                                }
                                div { class: "space-y-1.5",
                                    label { class: "text-sm font-medium", "DOI" }
                                    input {
                                        r#type: "text",
                                        class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                        placeholder: "10.1234/...",
                                        value: "{doi}",
                                        oninput: move |e| doi.set(e.value()),
                                    }
                                }
                            }
                        },
                        CitationEditorTab::Prompt => rsx! {
                            div { class: "space-y-1.5",
                                label { class: "text-sm font-medium", "AI Model *" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                    placeholder: "e.g., Claude, GPT-4, Llama",
                                    value: "{llm}",
                                    oninput: move |e| llm.set(e.value()),
                                }
                            }
                            div { class: "space-y-1.5",
                                label { class: "text-sm font-medium", "Conversation URL" }
                                input {
                                    r#type: "url",
                                    class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                    placeholder: "Link to shared conversation (optional)",
                                    value: "{prompt_url}",
                                    oninput: move |e| prompt_url.set(e.value()),
                                }
                            }
                            div { class: "space-y-1.5",
                                label { class: "text-sm font-medium", "Conversation Summary" }
                                textarea {
                                    class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50 min-h-[80px] resize-y",
                                    placeholder: "Brief summary of the conversation context (optional)",
                                    value: "{conversation_summary}",
                                    oninput: move |e| conversation_summary.set(e.value()),
                                }
                            }
                        },
                    }
                    div { class: "grid grid-cols-2 gap-3",
                        div { class: "space-y-1.5",
                            label { class: "text-sm font-medium",
                                if *active_tab.read() == CitationEditorTab::Hardcopy {
                                    "Title *"
                                } else {
                                    "Title"
                                }
                            }
                            input {
                                r#type: "text",
                                class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                placeholder: "Source title",
                                value: "{title}",
                                oninput: move |e| title.set(e.value()),
                            }
                        }
                        div { class: "space-y-1.5",
                            label { class: "text-sm font-medium",
                                if *active_tab.read() == CitationEditorTab::Hardcopy {
                                    "Author *"
                                } else {
                                    "Author"
                                }
                            }
                            input {
                                r#type: "text",
                                class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                placeholder: "Author name",
                                value: "{author}",
                                oninput: move |e| author.set(e.value()),
                            }
                        }
                    }
                    div { class: "space-y-1.5",
                        label { class: "text-sm font-medium", "Cited Text *" }
                        textarea {
                            class: "w-full px-3 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50 min-h-[120px] resize-y",
                            placeholder: "The quoted or cited content...",
                            value: "{cited_text}",
                            oninput: move |e| cited_text.set(e.value()),
                        }
                        p { class: "text-xs text-muted-foreground",
                            "The text you're quoting or summarizing from the source"
                        }
                    }
                    if let Some(err) = error.read().as_ref() {
                        div { class: "p-3 bg-destructive/10 border border-destructive/30 rounded-lg",
                            p { class: "text-sm text-destructive", "{err}" }
                        }
                    }
                }
                div { class: "flex items-center justify-end gap-3 px-6 py-4 border-t border-border bg-muted/30",
                    button {
                        class: "px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground transition-colors",
                        onclick: close_modal,
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: !*is_valid.read() || *saving.read(),
                        onclick: handle_save,
                        if *saving.read() {
                            "Saving..."
                        } else if is_editing {
                            "Update Citation"
                        } else {
                            "Create Citation"
                        }
                    }
                }
            }
        }
    }
}
