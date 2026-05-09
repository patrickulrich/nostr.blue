use crate::services::pages;
use crate::stores::nostr_client;
use crate::utils::nips::nip5a::{SiteManifest, DEFAULT_GATEWAY};
use dioxus::prelude::*;

#[component]
pub fn CodePages() -> Element {
    let mut manifests = use_signal(Vec::<SiteManifest>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut search = use_signal(String::new);
    let mut gen = use_signal(|| 0u32);

    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let current_gen = gen.peek().wrapping_add(1);
        gen.set(current_gen);
        spawn(async move {
            loading.set(true);
            error.set(None);
            match pages::fetch_recent_pages(100).await {
                Ok(m) => {
                    if *gen.peek() != current_gen {
                        return;
                    }
                    manifests.set(m);
                    loading.set(false);
                }
                Err(e) => {
                    if *gen.peek() != current_gen {
                        return;
                    }
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    rsx! {
        div { class: "max-w-6xl mx-auto px-4 py-6",
            div { class: "flex items-center justify-between mb-6",
                h1 { class: "text-2xl font-bold text-foreground", "Static Pages" }
                a {
                    href: "https://nsite.info/debug",
                    target: "_blank",
                    class: "text-sm text-muted-foreground hover:text-foreground transition",
                    "Debug Tool ↗"
                }
            }

            div { class: "mb-6",
                input {
                    class: "w-full bg-card border border-border rounded-lg px-4 py-2 text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary",
                    placeholder: "Search pages...",
                    value: "{search}",
                    oninput: move |e| search.set(e.value()),
                }
            }

            if *loading.read() {
                div { class: "text-center py-12 text-muted-foreground",
                    "Loading pages..."
                }
            } else if let Some(err) = error.read().as_ref() {
                div { class: "text-center py-12 text-red-500",
                    "{err}"
                }
            } else {
                {
                    let search_lower = search.read().to_lowercase();
                    let all = manifests.read();
                    let filtered: Vec<_> = all.iter()
                        .filter(|m| {
                            if search_lower.is_empty() { return true; }
                            m.title.as_deref().unwrap_or("").to_lowercase().contains(&search_lower)
                                || m.description.as_deref().unwrap_or("").to_lowercase().contains(&search_lower)
                                || m.d_tag.as_deref().unwrap_or("").to_lowercase().contains(&search_lower)
                        })
                        .cloned()
                        .collect();

                    if filtered.is_empty() {
                        rsx! {
                            div { class: "text-center py-12 text-muted-foreground",
                                "No pages found."
                            }
                        }
                    } else {
                        rsx! {
                            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                for manifest in filtered {
                                    { rsx! { PagesCard { manifest: manifest.clone() } } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PagesCard(manifest: SiteManifest) -> Element {
    let url = manifest.site_url(DEFAULT_GATEWAY);
    let title = manifest.title.clone().unwrap_or_else(|| manifest.d_tag.clone().unwrap_or_default());
    let description = manifest.description.clone().unwrap_or_default();
    let path_count = manifest.path_count();
    let d_tag = manifest.d_tag.clone().unwrap_or_default();

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 hover:border-primary/50 transition",
            div { class: "flex items-start justify-between mb-2",
                h3 { class: "font-semibold text-foreground truncate",
                    if !title.is_empty() {
                        "{title}"
                    } else {
                        "{d_tag}"
                    }
                }
                span { class: "text-xs text-muted-foreground ml-2 whitespace-nowrap",
                    "{path_count} files"
                }
            }
            if !description.is_empty() {
                p { class: "text-sm text-muted-foreground mb-3 line-clamp-2",
                    "{description}"
                }
            }
            div { class: "flex items-center gap-2 mt-2",
                a {
                    href: "{url}",
                    target: "_blank",
                    class: "text-xs text-primary hover:underline truncate",
                    "{url}"
                }
            }
        }
    }
}
