//! New Repository Page
//!
//! Create a new NIP-34 Git repository announcement (Kind 30617).
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::publish_repository;
use crate::stores::auth_store;
use crate::utils::nips::nip34::encode_repo_naddr;
use dioxus::prelude::*;
use nostr_sdk::prelude::{Coordinate, Kind, PublicKey};

/// New repository page component
#[component]
pub fn CodeNew() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut repo_id = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let clone_urls = use_signal(|| vec![String::new()]);
    let web_urls = use_signal(|| vec![String::new()]);
    let relays = use_signal(|| vec![String::new()]);
    let maintainer_inputs = use_signal(|| vec![String::new()]);
    let mut topics_input = use_signal(String::new);
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let nav = use_navigator();

    // Auto-generate repo_id from name
    use_effect(move || {
        let name_val = name.read().clone();
        if !name_val.is_empty() && repo_id.read().is_empty() {
            let slug = name_val
                .to_lowercase()
                .replace(' ', "-")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>();
            repo_id.set(slug);
        }
    });

    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState {}
        };
    }

    let handle_submit = move |_| {
        let id_val = repo_id.read().clone();
        let name_val = name.read().clone();
        let desc_val = description.read().clone();
        let clone_val = clone_urls.read().clone();
        let web_val = web_urls.read().clone();
        let relay_val = relays.read().clone();
        let maint_val = maintainer_inputs.read().clone();
        let topics_val = topics_input.read().clone();

        spawn(async move {
            if id_val.trim().is_empty() {
                error_message.set(Some("Repository ID is required".to_string()));
                return;
            }

            is_publishing.set(true);
            error_message.set(None);

            // Filter out empty URLs
            let clone_list: Vec<&str> = clone_val
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            let web_list: Vec<&str> = web_val
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            let relay_list: Vec<&str> = relay_val
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            // Parse maintainer pubkeys
            let mut maintainers = Vec::new();
            for input in maint_val.iter() {
                let trimmed = input.trim();
                if !trimmed.is_empty() {
                    match PublicKey::parse(trimmed) {
                        Ok(pk) => maintainers.push(pk),
                        Err(e) => {
                            error_message.set(Some(format!("Invalid pubkey '{}': {}", trimmed, e)));
                            is_publishing.set(false);
                            return;
                        }
                    }
                }
            }

            let name_opt = if name_val.is_empty() { None } else { Some(name_val.as_str()) };
            let desc_opt = if desc_val.is_empty() { None } else { Some(desc_val.as_str()) };

            let _topic_list: Vec<&str> = topics_val
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            match publish_repository(
                &id_val,
                name_opt,
                desc_opt,
                &clone_list,
                &web_list,
                &relay_list,
                &maintainers,
            )
            .await
            {
                Ok(_event_id) => {
                    // Navigate to the new repository
                    // Construct the naddr from the coordinate
                    if let Some(pk_hex) = auth_store::get_pubkey() {
                        let pk = match PublicKey::parse(&pk_hex) {
                            Ok(p) => p,
                            Err(_) => {
                                nav.push(Route::CodeHome {});
                                return;
                            }
                        };
                        let coordinate = Coordinate::new(Kind::GitRepoAnnouncement, pk)
                            .identifier(&id_val);
                        match encode_repo_naddr(&coordinate, &[]) {
                            Ok(naddr) => {
                                nav.push(Route::CodeRepo { naddr });
                            }
                            Err(_) => {
                                nav.push(Route::CodeHome {});
                            }
                        }
                    } else {
                        nav.push(Route::CodeHome {});
                    }
                }
                Err(e) => {
                    error_message.set(Some(e));
                    is_publishing.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeHome {},
                            class: "text-muted-foreground hover:text-foreground",
                            dangerous_inner_html: icons::ARROW_LEFT,
                        }
                        h1 { class: "text-xl font-bold flex items-center gap-2",
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
                                line { x1: "6", y1: "3", x2: "6", y2: "15" }
                                circle { cx: "18", cy: "6", r: "3" }
                                circle { cx: "6", cy: "18", r: "3" }
                                path { d: "M18 9a9 9 0 0 1-9 9" }
                            }
                            "New Repository"
                        }
                    }
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
                                    stroke_width: "4",
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z",
                                }
                            }
                            "Publishing..."
                        } else {
                            "Create Repository"
                        }
                    }
                }
            }

            div { class: "p-4 space-y-6",
                if let Some(error) = error_message.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }

                div { class: "p-4 bg-blue-500/10 rounded-lg border border-blue-500/20",
                    div { class: "flex items-start gap-3",
                        div { class: "w-8 h-8 rounded-lg bg-blue-500/20 flex items-center justify-center shrink-0",
                            svg {
                                class: "w-4 h-4 text-blue-500",
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
                            p { class: "text-sm",
                                span { class: "font-medium", "NIP-34 Git Repository" }
                                span { class: "text-muted-foreground",
                                    " - Your repository announcement will be published as a Kind 30617 event on Nostr. This creates a decentralized Git repository that can host issues, pull requests, and collaboration."
                                }
                            }
                        }
                    }
                }

                div {
                    label { class: "block text-sm font-medium mb-2",
                        "Repository ID "
                        span { class: "text-destructive", "*" }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "e.g., my-awesome-project",
                        value: "{repo_id}",
                        oninput: move |e| repo_id.set(e.value()),
                    }
                    p { class: "text-xs text-muted-foreground mt-1",
                        "Unique identifier (kebab-case). Auto-generated from name."
                    }
                }

                div {
                    label { class: "block text-sm font-medium mb-2",
                        "Display Name "
                        span { class: "text-muted-foreground font-normal", "(optional)" }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "e.g., My Awesome Project",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }

                div {
                    label { class: "block text-sm font-medium mb-2",
                        "Description "
                        span { class: "text-muted-foreground font-normal", "(optional)" }
                    }
                    textarea {
                        class: "w-full h-24 px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary resize-y",
                        placeholder: "Describe your repository...",
                        value: "{description}",
                        oninput: move |e| description.set(e.value()),
                    }
                }

                div { class: "space-y-2",
                    label { class: "block text-sm font-medium text-foreground", "Topics" }
                    input {
                        class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder-muted-foreground",
                        placeholder: "rust, nostr, git (comma-separated)",
                        value: "{topics_input}",
                        oninput: move |e| topics_input.set(e.value()),
                    }
                }

                DynamicListField {
                    label: "Clone URLs",
                    placeholder: "git://example.com/repo.git or https://github.com/user/repo.git",
                    values: clone_urls,
                }

                DynamicListField {
                    label: "Web URLs",
                    placeholder: "https://example.com/repo",
                    values: web_urls,
                }

                DynamicListField {
                    label: "Relays",
                    placeholder: "wss://relay.example.com",
                    values: relays,
                }

                DynamicListField {
                    label: "Maintainers",
                    placeholder: "npub... or hex pubkey",
                    values: maintainer_inputs,
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DynamicListFieldProps {
    label: &'static str,
    placeholder: &'static str,
    values: Signal<Vec<String>>,
}

#[component]
fn DynamicListField(props: DynamicListFieldProps) -> Element {
    let mut values = props.values;

    let add_item = move |_| {
        let mut current = values.read().clone();
        current.push(String::new());
        values.set(current);
    };

    let mut remove_item = move |index: usize| {
        let mut current = values.read().clone();
        if current.len() > 1 {
            current.remove(index);
        } else {
            current[0] = String::new();
        }
        values.set(current);
    };

    let mut update_item = move |index: usize, value: String| {
        let mut current = values.read().clone();
        if index < current.len() {
            current[index] = value;
            values.set(current);
        }
    };

    rsx! {
        div {
            label { class: "block text-sm font-medium mb-2",
                "{props.label} "
                span { class: "text-muted-foreground font-normal", "(optional)" }
            }
            div { class: "space-y-2",
                for (i , value) in values.read().iter().enumerate() {
                    div { key: "{i}", class: "flex items-center gap-2",
                        input {
                            class: "flex-1 px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "{props.placeholder}",
                            value: "{value}",
                            oninput: move |e| update_item(i, e.value()),
                        }
                        button {
                            class: "p-2 text-muted-foreground hover:text-destructive transition",
                            r#type: "button",
                            onclick: move |_| remove_item(i),
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                line { x1: "5", y1: "12", x2: "19", y2: "12" }
                            }
                        }
                    }
                }
                button {
                    class: "flex items-center gap-2 text-sm text-primary hover:underline",
                    r#type: "button",
                    onclick: add_item,
                    svg {
                        class: "w-4 h-4",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        line { x1: "12", y1: "5", x2: "12", y2: "19" }
                        line { x1: "5", y1: "12", x2: "19", y2: "12" }
                    }
                    "Add"
                }
            }
        }
    }
}

#[component]
fn NotAuthenticatedState() -> Element {
    rsx! {
        div { class: "min-h-screen flex items-center justify-center p-4",
            div { class: "text-center max-w-md",
                div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
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
                h2 { class: "font-semibold text-xl mb-2", "Sign In Required" }
                p { class: "text-muted-foreground mb-6",
                    "Connect with your Nostr identity to create repositories."
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
