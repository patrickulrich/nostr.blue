//! Badge Creation Page
//!
//! Form for creating new badge definitions (kind 30009).
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip58;
use crate::utils::slugify;
use dioxus::prelude::*;
/// Badge creation page component
#[component]
pub fn BadgeNew() -> Element {
    let navigator = use_navigator();
    let mut badge_id = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut image_url = use_signal(String::new);
    let mut thumb_url = use_signal(String::new);
    let mut auto_id = use_signal(|| true);
    let mut publishing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let is_authenticated = auth_store::is_authenticated();
    use_effect(move || {
        if *auto_id.read() && !name.read().is_empty() {
            badge_id.set(slugify(&name.read()));
        }
    });
    if !is_authenticated {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center p-4",
                div { class: "text-center",
                    div { class: "text-6xl mb-4", "🔒" }
                    h2 { class: "text-xl font-bold mb-2", "Authentication Required" }
                    p { class: "text-muted-foreground mb-4", "You need to sign in to create badges" }
                    Link {
                        to: Route::BadgesHome {},
                        class: "text-primary hover:underline",
                        "← Back to Badges"
                    }
                }
            }
        };
    }
    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! {
            ClientInitializing {}
        };
    }
    let can_publish = !badge_id.read().is_empty() && !*publishing.read();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center justify-between px-4 py-3",
                    div { class: "flex items-center gap-4",
                        button {
                            class: "p-2 rounded-full hover:bg-accent transition",
                            onclick: move |_| {
                                navigator.go_back();
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
                                    d: "M15 19l-7-7 7-7",
                                }
                            }
                        }
                        h1 { class: "text-xl font-bold", "Create Badge" }
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                        disabled: !can_publish,
                        onclick: move |_| {
                            publishing.set(true);
                            error.set(None);
                            let id = badge_id.read().clone();
                            let n = if name.read().is_empty() { None } else { Some(name.read().clone()) };
                            let d = if description.read().is_empty() {
                                None
                            } else {
                                Some(description.read().clone())
                            };
                            let img = if image_url.read().is_empty() {
                                None
                            } else {
                                Some(image_url.read().clone())
                            };
                            let thumb = if thumb_url.read().is_empty() {
                                None
                            } else {
                                Some(thumb_url.read().clone())
                            };
                            spawn(async move {
                                match nip58::create_badge_definition(
                                        &id,
                                        n.as_deref(),
                                        d.as_deref(),
                                        img.as_deref(),
                                        None,
                                        thumb.as_deref(),
                                        None,
                                    )
                                    .await
                                {
                                    Ok(event_id) => {
                                        log::info!("Badge created: {}", event_id);
                                        navigator.push(Route::BadgesHome {});
                                    }
                                    Err(e) => {
                                        log::error!("Failed to create badge: {}", e);
                                        error.set(Some(e));
                                    }
                                }
                                publishing.set(false);
                            });
                        },
                        if *publishing.read() {
                            "Publishing..."
                        } else {
                            "Publish"
                        }
                    }
                }
            }
            div { class: "p-4 max-w-2xl mx-auto space-y-6",
                if let Some(e) = error.read().as_ref() {
                    div { class: "bg-destructive/10 border border-destructive/20 rounded-xl p-4 text-destructive",
                        "{e}"
                    }
                }
                div { class: "bg-card border border-border rounded-xl overflow-hidden",
                    div { class: "aspect-video bg-muted flex items-center justify-center",
                        if !image_url.read().is_empty() {
                            img {
                                src: "{image_url}",
                                alt: "Badge preview",
                                class: "max-h-full max-w-full object-contain",
                            }
                        } else if !thumb_url.read().is_empty() {
                            img {
                                src: "{thumb_url}",
                                alt: "Badge preview",
                                class: "max-h-full max-w-full object-contain",
                            }
                        } else {
                            div { class: "text-center text-muted-foreground",
                                div { class: "text-4xl mb-2", "🏅" }
                                "Badge Preview"
                            }
                        }
                    }
                    div { class: "p-4",
                        h3 { class: "font-bold text-lg",
                            if name.read().is_empty() {
                                "Badge Name"
                            } else {
                                "{name}"
                            }
                        }
                        if !description.read().is_empty() {
                            p { class: "text-muted-foreground text-sm mt-1", "{description}" }
                        }
                        p { class: "text-xs text-muted-foreground mt-2 font-mono",
                            "ID: {badge_id}"
                        }
                    }
                }
                div { class: "space-y-4",
                    div {
                        label { class: "block text-sm font-medium mb-2", "Name" }
                        input {
                            class: "w-full px-4 py-2 bg-muted rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Badge of Honor",
                            value: "{name}",
                            oninput: move |e| name.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2",
                            "Badge ID"
                            span { class: "text-destructive ml-1", "*" }
                        }
                        div { class: "flex gap-2",
                            input {
                                class: "flex-1 px-4 py-2 bg-muted rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary font-mono",
                                r#type: "text",
                                placeholder: "badge-of-honor",
                                value: "{badge_id}",
                                oninput: move |e| {
                                    badge_id.set(e.value());
                                    auto_id.set(false);
                                },
                            }
                            button {
                                class: if *auto_id.read() { "px-3 py-2 rounded-lg bg-primary/20 text-primary text-sm" } else { "px-3 py-2 rounded-lg bg-muted hover:bg-accent text-sm" },
                                onclick: move |_| {
                                    let current = *auto_id.read();
                                    auto_id.set(!current);
                                    if !current {
                                        badge_id.set(slugify(&name.read()));
                                    }
                                },
                                "Auto"
                            }
                        }
                        p { class: "text-xs text-muted-foreground mt-1",
                            "Unique identifier for this badge (URL-safe)"
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Description" }
                        textarea {
                            class: "w-full px-4 py-2 bg-muted rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                            rows: "3",
                            placeholder: "Describe what this badge represents...",
                            value: "{description}",
                            oninput: move |e| description.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Image URL" }
                        input {
                            class: "w-full px-4 py-2 bg-muted rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "url",
                            placeholder: "https://example.com/badge.png",
                            value: "{image_url}",
                            oninput: move |e| image_url.set(e.value()),
                        }
                        p { class: "text-xs text-muted-foreground mt-1",
                            "High-resolution image (recommended: 512x512 or larger)"
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Thumbnail URL" }
                        input {
                            class: "w-full px-4 py-2 bg-muted rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "url",
                            placeholder: "https://example.com/badge-thumb.png",
                            value: "{thumb_url}",
                            oninput: move |e| thumb_url.set(e.value()),
                        }
                        p { class: "text-xs text-muted-foreground mt-1",
                            "Optional smaller thumbnail (recommended: 64x64)"
                        }
                    }
                }
                div { class: "bg-muted/50 rounded-xl p-4 text-sm text-muted-foreground space-y-2",
                    h4 { class: "font-medium text-foreground", "About Badges" }
                    p {
                        "Badges are digital achievements you can award to other users. Once created, you can award this badge to anyone on Nostr."
                    }
                    ul { class: "list-disc list-inside space-y-1",
                        li { "Badge ID cannot be changed after creation" }
                        li { "Images should be PNG or SVG format" }
                        li { "Users must accept badges before they appear on their profile" }
                    }
                }
            }
        }
    }
}
