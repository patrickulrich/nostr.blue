use crate::services::pages;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::utils::nips::nip5a::{
    build_nsite_url, is_reserved_slug, slug_to_nsite_dtag, SiteManifest, DEFAULT_GATEWAY,
    KNOWN_GATEWAYS, PAGES_README_BEGIN,
};
use crate::utils::nip34::Repository;
use dioxus::prelude::*;

#[component]
pub fn RepoPagesPanel(repo: Repository, naddr: String, is_owner: bool) -> Element {
    let mut manifest = use_signal(|| None::<SiteManifest>);
    let mut loading = use_signal(|| false);
    let mut publishing = use_signal(|| false);
    let mut slug_input = use_signal(String::new);
    let mut slug_error = use_signal(|| None::<String>);
    let mut message = use_signal(|| None::<String>);
    let mut gateway = use_signal(|| DEFAULT_GATEWAY.to_string());

    let repo_name = repo.name.clone().unwrap_or_default();
    let default_dtag = slug_to_nsite_dtag(&repo_name);
    let pubkey = repo.pubkey.clone();
    let effective_dtag = {
        let input = slug_input.read();
        if input.is_empty() {
            default_dtag.clone()
        } else {
            slug_to_nsite_dtag(&input)
        }
    };
    let site_url = build_nsite_url(&gateway.read(), &pubkey, Some(&effective_dtag));
    let effective_dtag_clone = effective_dtag.clone();

    let _naddr_clone = naddr.clone();
    let pubkey_clone = pubkey.clone();
    let effective_dtag_for_effect = effective_dtag.clone();
    use_effect(move || {
        let pk = pubkey_clone.clone();
        let dt = effective_dtag_for_effect.clone();
        spawn(async move {
            loading.set(true);
            match pages::fetch_pages_manifest(&pk, Some(&dt)).await {
                Ok(m) => manifest.set(m),
                Err(_) => manifest.set(None),
            }
            loading.set(false);
        });
    });

    let naddr_publish = naddr.clone();
    let dtag_publish = effective_dtag_clone.clone();
    let title = repo.name.clone().unwrap_or_default();
    let source_url = repo.web.first().cloned();

    rsx! {
        div { class: "bg-card border border-border rounded-lg overflow-hidden",
            details { class: "group",
                summary { class: "px-4 py-3 cursor-pointer select-none hover:bg-accent/50 transition flex items-center gap-2",
                    span { class: "text-foreground font-medium",
                        "🌐 Static Pages"
                    }
                    if manifest.read().is_some() {
                        span { class: "text-xs bg-green-500/20 text-green-400 px-2 py-0.5 rounded-full",
                            "Published"
                        }
                    }
                }

                div { class: "px-4 pb-4 space-y-4 border-t border-border pt-4",
                    // Status message
                    if let Some(msg) = message.read().as_ref() {
                        div { class: "text-sm px-3 py-2 rounded-lg bg-accent/50 text-foreground",
                            "{msg}"
                        }
                    }

                    if let Some(err) = slug_error.read().as_ref() {
                        div { class: "text-sm px-3 py-2 rounded-lg bg-red-500/20 text-red-400",
                            "{err}"
                        }
                    }

                    // Manifest info
                    if *loading.read() {
                        div { class: "text-sm text-muted-foreground",
                            "Checking manifest status..."
                        }
                    } else if let Some(m) = manifest.read().as_ref() {
                        div { class: "space-y-2",
                            div { class: "flex items-center gap-2",
                                span { class: "text-green-400 text-sm", "✓" }
                                span { class: "text-sm text-foreground",
                                    "Published: {m.path_count()} files"
                                }
                            }
                        }
                    } else if !*loading.read() {
                        div { class: "text-sm text-muted-foreground",
                            "No manifest published yet."
                        }
                    }

                    // Site name
                    if is_owner {
                        div { class: "space-y-1",
                            label { class: "text-xs text-muted-foreground uppercase tracking-wide",
                                "Site Name"
                            }
                            div { class: "flex gap-2",
                                input {
                                    class: "flex-1 bg-background border border-border rounded-lg px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary",
                                    placeholder: "{default_dtag}",
                                    value: "{slug_input}",
                                    oninput: move |e| {
                                        slug_input.set(e.value());
                                        let d = slug_to_nsite_dtag(&e.value());
                                        if is_reserved_slug(&d) {
                                            slug_error.set(Some(format!("'{}' is reserved", d)));
                                        } else {
                                            slug_error.set(None);
                                        }
                                    },
                                }
                            }
                            p { class: "text-xs text-muted-foreground",
                                "NIP-5A identifier: {effective_dtag}"
                            }
                        }
                    }

                    // Gateway selector
                    div { class: "space-y-1",
                        label { class: "text-xs text-muted-foreground uppercase tracking-wide",
                            "Gateway"
                        }
                        select {
                            class: "w-full bg-background border border-border rounded-lg px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary",
                            value: "{gateway}",
                            onchange: move |e| gateway.set(e.value()),
                            for gw in KNOWN_GATEWAYS {
                                option { value: "{gw}", "{gw}" }
                            }
                        }
                    }

                    // Live URL
                    div { class: "space-y-1",
                        label { class: "text-xs text-muted-foreground uppercase tracking-wide",
                            "Live URL"
                        }
                        div { class: "flex items-center gap-2",
                            a {
                                href: "{site_url}",
                                target: "_blank",
                                class: "text-sm text-primary hover:underline truncate flex-1",
                                "{site_url}"
                            }
                            button {
                                class: "p-1 hover:bg-accent rounded transition text-muted-foreground",
                                onclick: {
                                    let _url = site_url.clone();
                                    move |_| {
                                        message.set(Some("Copied URL!".to_string()));
                                    }
                                },
                                "📋"
                            }
                        }
                    }

                    // Push Manifest button (owner only)
                    if is_owner && *HAS_SIGNER.read() {
                        div { class: "pt-2",
                            button {
                                class: if *publishing.read() {
                                    "w-full px-4 py-2 rounded-lg text-sm font-medium bg-primary/50 text-primary-foreground cursor-not-allowed"
                                } else {
                                    "w-full px-4 py-2 rounded-lg text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition"
                                },
                                disabled: *publishing.read(),
                                onclick: {
                                    let naddr = naddr_publish.clone();
                                    let dtag = dtag_publish.clone();
                                    let title = title.clone();
                                    let source = source_url.clone();
                                    move |_| {
                                        let naddr = naddr.clone();
                                        let dtag = dtag.clone();
                                        let title = title.clone();
                                        let source = source.clone();
                                        spawn(async move {
                                            publishing.set(true);
                                            message.set(None);
                                            match pages::publish_pages_manifest(
                                                &naddr,
                                                &dtag,
                                                Some(title),
                                                None,
                                                source,
                                                None,
                                                None,
                                            ).await {
                                                Ok(result) => {
                                                    message.set(Some(format!(
                                                        "Published {} files!",
                                                        result.path_count
                                                    )));
                                                }
                                                Err(e) => {
                                                    message.set(Some(format!("Error: {}", e)));
                                                }
                                            }
                                            publishing.set(false);
                                        });
                                    }
                                },
                                if *publishing.read() {
                                    "Publishing..."
                                } else {
                                    "Push Manifest"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
