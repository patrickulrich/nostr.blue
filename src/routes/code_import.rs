//! Code Import Page
//!
//! Import repositories from GitHub, GitLab, etc. into NIP-34.

use dioxus::prelude::*;
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{
    github_import::{fetch_repo_from_url, GitHubRepo},
    publish_repository,
};
use crate::stores::auth_store;

/// Import wizard steps
#[derive(Clone, Copy, PartialEq, Eq)]
enum ImportStep {
    EnterUrl,
    Preview,
    Configure,
    Publishing,
    Complete,
}

/// Code import page component
#[component]
pub fn CodeImport() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let nav = use_navigator();

    // Wizard state
    let mut current_step = use_signal(|| ImportStep::EnterUrl);
    let url_input = use_signal(String::new);
    let mut is_fetching = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);

    // GitHub repo data
    let mut github_repo = use_signal(|| None::<GitHubRepo>);

    // Configuration options
    let mut repo_id = use_signal(String::new);
    let mut repo_name = use_signal(String::new);
    let mut repo_description = use_signal(String::new);
    let include_web_url = use_signal(|| true);
    let include_clone_url = use_signal(|| true);

    // Published result
    let mut published_naddr = use_signal(|| None::<String>);

    // Check authentication
    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState {}
        };
    }

    let handle_fetch_repo = move |_: ()| {
        let url = url_input.read().clone();
        if url.is_empty() {
            error_message.set(Some("Please enter a repository URL".to_string()));
            return;
        }

        spawn(async move {
            is_fetching.set(true);
            error_message.set(None);

            match fetch_repo_from_url(&url).await {
                Ok(repo) => {
                    // Pre-fill configuration
                    repo_id.set(repo.name.clone());
                    repo_name.set(repo.name.clone());
                    repo_description.set(repo.description.clone().unwrap_or_default());
                    github_repo.set(Some(repo));
                    current_step.set(ImportStep::Preview);
                }
                Err(e) => {
                    error_message.set(Some(e));
                }
            }
            is_fetching.set(false);
        });
    };

    let handle_configure = move |_| {
        current_step.set(ImportStep::Configure);
    };

    let handle_publish = {
        let _nav = nav;
        move |_| {
            let repo = match github_repo.read().clone() {
                Some(r) => r,
                None => return,
            };

            let id = repo_id.read().clone();
            let name = repo_name.read().clone();
            let description = repo_description.read().clone();
            let include_web = *include_web_url.read();
            let include_clone = *include_clone_url.read();

            spawn(async move {
                current_step.set(ImportStep::Publishing);
                error_message.set(None);

                // Build URL arrays
                let mut clone_urls = vec![];
                let mut web_urls = vec![];

                if include_clone {
                    clone_urls.push(repo.clone_url.as_str());
                }
                if include_web {
                    web_urls.push(repo.html_url.as_str());
                }

                // Default relays
                let relays = [
                    "wss://relay.damus.io",
                    "wss://nos.lol",
                    "wss://relay.nostr.band",
                ];

                let name_opt = if name.is_empty() { None } else { Some(name.as_str()) };
                let desc_opt = if description.is_empty() { None } else { Some(description.as_str()) };

                match publish_repository(
                    &id,
                    name_opt,
                    desc_opt,
                    &clone_urls,
                    &web_urls,
                    &relays,
                    &[],
                ).await {
                    Ok(event_id) => {
                        // Generate naddr for navigation
                        // For now, store the event ID hex
                        published_naddr.set(Some(event_id.to_hex()));
                        current_step.set(ImportStep::Complete);
                    }
                    Err(e) => {
                        error_message.set(Some(e));
                        current_step.set(ImportStep::Configure);
                    }
                }
            });
        }
    };

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeHome {},
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
                            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                            polyline { points: "7 10 12 15 17 10" }
                            line { x1: "12", y1: "15", x2: "12", y2: "3" }
                        }
                        "Import Repository"
                    }
                }

                // Progress steps
                div {
                    class: "px-4 pb-4",
                    StepIndicator {
                        current: *current_step.read(),
                    }
                }
            }

            // Content
            div {
                class: "p-4 max-w-2xl mx-auto",

                // Error message
                if let Some(error) = error_message.read().as_ref() {
                    div {
                        class: "mb-6 p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }

                match *current_step.read() {
                    ImportStep::EnterUrl => rsx! {
                        EnterUrlStep {
                            url_input: url_input,
                            is_fetching: *is_fetching.read(),
                            on_submit: handle_fetch_repo,
                        }
                    },
                    ImportStep::Preview => rsx! {
                        PreviewStep {
                            repo: github_repo.read().clone(),
                            on_continue: handle_configure,
                            on_back: move |_| current_step.set(ImportStep::EnterUrl),
                        }
                    },
                    ImportStep::Configure => rsx! {
                        ConfigureStep {
                            repo_id: repo_id,
                            repo_name: repo_name,
                            repo_description: repo_description,
                            include_web_url: include_web_url,
                            include_clone_url: include_clone_url,
                            on_publish: handle_publish,
                            on_back: move |_| current_step.set(ImportStep::Preview),
                        }
                    },
                    ImportStep::Publishing => rsx! {
                        PublishingStep {}
                    },
                    ImportStep::Complete => rsx! {
                        CompleteStep {
                            repo_name: repo_name.read().clone(),
                            event_id: published_naddr.read().clone(),
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn StepIndicator(current: ImportStep) -> Element {
    let steps = [
        ("URL", ImportStep::EnterUrl),
        ("Preview", ImportStep::Preview),
        ("Configure", ImportStep::Configure),
        ("Publish", ImportStep::Publishing),
    ];

    let current_idx = match current {
        ImportStep::EnterUrl => 0,
        ImportStep::Preview => 1,
        ImportStep::Configure => 2,
        ImportStep::Publishing | ImportStep::Complete => 3,
    };

    rsx! {
        div {
            class: "flex items-center justify-center gap-2",
            for (idx, (label, _step)) in steps.iter().enumerate() {
                if idx > 0 {
                    div {
                        class: if idx <= current_idx { "w-8 h-0.5 bg-primary" } else { "w-8 h-0.5 bg-muted" }
                    }
                }
                div {
                    class: "flex items-center gap-1.5",
                    div {
                        class: if idx < current_idx {
                            "w-6 h-6 rounded-full bg-primary text-primary-foreground flex items-center justify-center text-xs"
                        } else if idx == current_idx {
                            "w-6 h-6 rounded-full bg-primary text-primary-foreground flex items-center justify-center text-xs ring-2 ring-primary ring-offset-2 ring-offset-background"
                        } else {
                            "w-6 h-6 rounded-full bg-muted text-muted-foreground flex items-center justify-center text-xs"
                        },
                        if idx < current_idx {
                            "✓"
                        } else {
                            "{idx + 1}"
                        }
                    }
                    span {
                        class: if idx <= current_idx { "text-xs font-medium" } else { "text-xs text-muted-foreground" },
                        "{label}"
                    }
                }
            }
        }
    }
}

#[component]
fn EnterUrlStep(
    url_input: Signal<String>,
    is_fetching: bool,
    on_submit: EventHandler<()>,
) -> Element {
    let handle_key_press = move |e: KeyboardEvent| {
        if e.key() == Key::Enter && !is_fetching {
            on_submit.call(());
        }
    };

    rsx! {
        div {
            class: "space-y-6",

            // Info card
            div {
                class: "p-4 bg-muted rounded-lg",
                div {
                    class: "flex items-start gap-3",
                    div {
                        class: "w-10 h-10 rounded-full bg-primary/10 flex items-center justify-center shrink-0",
                        svg {
                            class: "w-5 h-5 text-primary",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4" }
                            path { d: "M9 18c-4.51 2-5-2-7-2" }
                        }
                    }
                    div {
                        h3 { class: "font-semibold mb-1", "Import from GitHub" }
                        p {
                            class: "text-sm text-muted-foreground",
                            "Import your existing repository to Nostr. This will create a NIP-34 repository announcement that links to your code."
                        }
                    }
                }
            }

            // URL input
            div {
                label {
                    class: "block text-sm font-medium mb-2",
                    "Repository URL"
                }
                div {
                    class: "flex gap-2",
                    input {
                        class: "flex-1 px-4 py-3 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "https://github.com/username/repository",
                        value: "{url_input}",
                        disabled: is_fetching,
                        oninput: move |e| url_input.set(e.value()),
                        onkeypress: handle_key_press,
                    }
                }
                p {
                    class: "text-xs text-muted-foreground mt-2",
                    "Supports GitHub URLs in various formats (HTTPS, SSH, etc.)"
                }
            }

            // Fetch button
            button {
                class: "w-full py-3 bg-primary text-primary-foreground rounded-lg font-medium hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2",
                disabled: is_fetching,
                onclick: move |_| on_submit.call(()),
                if is_fetching {
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
                    "Fetching..."
                } else {
                    "Fetch Repository"
                }
            }

            // Supported platforms
            div {
                class: "text-center",
                p {
                    class: "text-xs text-muted-foreground mb-3",
                    "Currently supported:"
                }
                div {
                    class: "flex justify-center gap-4",
                    PlatformBadge { name: "GitHub", supported: true }
                    PlatformBadge { name: "GitLab", supported: false }
                    PlatformBadge { name: "Codeberg", supported: false }
                }
            }
        }
    }
}

#[component]
fn PlatformBadge(name: &'static str, supported: bool) -> Element {
    rsx! {
        div {
            class: if supported {
                "px-3 py-1.5 bg-primary/10 text-primary rounded-full text-xs font-medium"
            } else {
                "px-3 py-1.5 bg-muted text-muted-foreground rounded-full text-xs"
            },
            "{name}"
            if !supported {
                " (soon)"
            }
        }
    }
}

#[component]
fn PreviewStep(
    repo: Option<GitHubRepo>,
    on_continue: EventHandler<MouseEvent>,
    on_back: EventHandler<MouseEvent>,
) -> Element {
    let repo = match repo {
        Some(r) => r,
        None => return rsx! { div { "No repository data" } },
    };

    rsx! {
        div {
            class: "space-y-6",

            // Repository card
            div {
                class: "p-6 border border-border rounded-lg",
                div {
                    class: "flex items-start gap-4",
                    img {
                        class: "w-12 h-12 rounded-lg",
                        src: "{repo.owner.avatar_url}",
                        alt: "{repo.owner.login}"
                    }
                    div {
                        class: "flex-1 min-w-0",
                        h2 {
                            class: "font-semibold text-lg truncate",
                            "{repo.full_name}"
                        }
                        if let Some(desc) = &repo.description {
                            p {
                                class: "text-sm text-muted-foreground mt-1",
                                "{desc}"
                            }
                        }
                    }
                }

                // Stats
                div {
                    class: "mt-4 flex flex-wrap gap-4 text-sm",
                    if let Some(lang) = &repo.language {
                        div {
                            class: "flex items-center gap-1.5",
                            div { class: "w-3 h-3 rounded-full bg-primary" }
                            "{lang}"
                        }
                    }
                    div {
                        class: "flex items-center gap-1 text-muted-foreground",
                        "⭐ {repo.stargazers_count}"
                    }
                    div {
                        class: "flex items-center gap-1 text-muted-foreground",
                        "🍴 {repo.forks_count}"
                    }
                    div {
                        class: "flex items-center gap-1 text-muted-foreground",
                        "🔓 {repo.open_issues_count} issues"
                    }
                }

                // URLs
                div {
                    class: "mt-4 space-y-2 text-sm",
                    div {
                        class: "flex items-center gap-2 p-2 bg-muted rounded",
                        span { class: "text-muted-foreground", "Clone:" }
                        code { class: "truncate", "{repo.clone_url}" }
                    }
                    div {
                        class: "flex items-center gap-2 p-2 bg-muted rounded",
                        span { class: "text-muted-foreground", "Web:" }
                        code { class: "truncate", "{repo.html_url}" }
                    }
                }

                // Topics
                if !repo.topics.is_empty() {
                    div {
                        class: "mt-4 flex flex-wrap gap-2",
                        for topic in repo.topics.iter() {
                            span {
                                class: "px-2 py-0.5 bg-primary/10 text-primary rounded text-xs",
                                "{topic}"
                            }
                        }
                    }
                }
            }

            // Actions
            div {
                class: "flex gap-3",
                button {
                    class: "flex-1 py-3 border border-border rounded-lg font-medium hover:bg-muted transition",
                    onclick: move |e| on_back.call(e),
                    "Back"
                }
                button {
                    class: "flex-1 py-3 bg-primary text-primary-foreground rounded-lg font-medium hover:opacity-90 transition",
                    onclick: move |e| on_continue.call(e),
                    "Continue"
                }
            }
        }
    }
}

#[component]
fn ConfigureStep(
    repo_id: Signal<String>,
    repo_name: Signal<String>,
    repo_description: Signal<String>,
    include_web_url: Signal<bool>,
    include_clone_url: Signal<bool>,
    on_publish: EventHandler<MouseEvent>,
    on_back: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: "space-y-6",

            // NIP-34 info
            div {
                class: "p-4 bg-purple-500/10 rounded-lg border border-purple-500/20",
                div {
                    class: "flex items-start gap-3",
                    div {
                        class: "w-8 h-8 rounded-lg bg-purple-500/20 flex items-center justify-center shrink-0",
                        svg {
                            class: "w-4 h-4 text-purple-500",
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
                            span { class: "font-medium", "NIP-34 Repository Announcement" }
                            span { class: "text-muted-foreground", " - This creates a Kind 30617 event linking to your repository. The code stays on GitHub; this event helps others discover it on Nostr." }
                        }
                    }
                }
            }

            // ID field
            div {
                label {
                    class: "block text-sm font-medium mb-2",
                    "Repository ID "
                    span { class: "text-destructive", "*" }
                }
                input {
                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                    r#type: "text",
                    placeholder: "my-project",
                    value: "{repo_id}",
                    oninput: move |e| repo_id.set(e.value())
                }
                p {
                    class: "text-xs text-muted-foreground mt-1",
                    "Unique identifier (d-tag). Cannot be changed after publishing."
                }
            }

            // Name field
            div {
                label {
                    class: "block text-sm font-medium mb-2",
                    "Display Name"
                }
                input {
                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                    r#type: "text",
                    placeholder: "My Project",
                    value: "{repo_name}",
                    oninput: move |e| repo_name.set(e.value())
                }
            }

            // Description field
            div {
                label {
                    class: "block text-sm font-medium mb-2",
                    "Description"
                }
                textarea {
                    class: "w-full h-24 px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary resize-y",
                    placeholder: "A brief description of your project...",
                    value: "{repo_description}",
                    oninput: move |e| repo_description.set(e.value())
                }
            }

            // URL options
            div {
                class: "space-y-3",
                label {
                    class: "block text-sm font-medium",
                    "Include URLs"
                }
                label {
                    class: "flex items-center gap-3 p-3 bg-muted rounded-lg cursor-pointer",
                    input {
                        r#type: "checkbox",
                        class: "w-4 h-4 rounded border-border",
                        checked: *include_clone_url.read(),
                        onchange: move |e| include_clone_url.set(e.checked())
                    }
                    div {
                        div { class: "font-medium text-sm", "Clone URL" }
                        div { class: "text-xs text-muted-foreground", "Allow cloning via HTTPS" }
                    }
                }
                label {
                    class: "flex items-center gap-3 p-3 bg-muted rounded-lg cursor-pointer",
                    input {
                        r#type: "checkbox",
                        class: "w-4 h-4 rounded border-border",
                        checked: *include_web_url.read(),
                        onchange: move |e| include_web_url.set(e.checked())
                    }
                    div {
                        div { class: "font-medium text-sm", "Web URL" }
                        div { class: "text-xs text-muted-foreground", "Link to GitHub web interface" }
                    }
                }
            }

            // Actions
            div {
                class: "flex gap-3 pt-4",
                button {
                    class: "flex-1 py-3 border border-border rounded-lg font-medium hover:bg-muted transition",
                    onclick: move |e| on_back.call(e),
                    "Back"
                }
                button {
                    class: "flex-1 py-3 bg-primary text-primary-foreground rounded-lg font-medium hover:opacity-90 transition",
                    onclick: move |e| on_publish.call(e),
                    "Publish to Nostr"
                }
            }
        }
    }
}

#[component]
fn PublishingStep() -> Element {
    rsx! {
        div {
            class: "text-center py-12",
            div {
                class: "w-16 h-16 mx-auto mb-6 rounded-full bg-primary/10 flex items-center justify-center",
                svg {
                    class: "w-8 h-8 text-primary animate-spin",
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
            }
            h3 { class: "font-semibold text-lg mb-2", "Publishing..." }
            p { class: "text-muted-foreground", "Creating your repository announcement on Nostr" }
        }
    }
}

#[component]
fn CompleteStep(repo_name: String, event_id: Option<String>) -> Element {
    let nav = use_navigator();

    rsx! {
        div {
            class: "text-center py-12",
            div {
                class: "w-16 h-16 mx-auto mb-6 rounded-full bg-green-500/10 flex items-center justify-center",
                svg {
                    class: "w-8 h-8 text-green-500",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M22 11.08V12a10 10 0 1 1-5.93-9.14" }
                    polyline { points: "22 4 12 14.01 9 11.01" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "Repository Published!" }
            p {
                class: "text-muted-foreground mb-6",
                "\"{repo_name}\" has been published to Nostr."
            }

            if let Some(id) = event_id {
                div {
                    class: "mb-6 p-3 bg-muted rounded-lg",
                    p { class: "text-xs text-muted-foreground mb-1", "Event ID" }
                    code { class: "text-sm break-all", "{id}" }
                }
            }

            div {
                class: "flex flex-col gap-3",
                Link {
                    to: Route::CodeRepositories {},
                    class: "py-3 bg-primary text-primary-foreground rounded-lg font-medium hover:opacity-90 transition text-center",
                    "View My Repositories"
                }
                button {
                    class: "py-3 border border-border rounded-lg font-medium hover:bg-muted transition",
                    onclick: move |_| { let _ = nav.push(Route::CodeImport {}); },
                    "Import Another"
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
                    "Connect with your Nostr identity to import repositories."
                }
                Link {
                    to: Route::CodeHome {},
                    class: "text-primary hover:underline",
                    "Back to Code"
                }
            }
        }
    }
}
