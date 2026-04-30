use crate::components::icons::{GitHubIcon, MastodonIcon, TelegramIcon, TwitterIcon};
use crate::stores::nostr_client;
use crate::utils::nips::nip39::{self, ExternalIdentityInfo};
use dioxus::prelude::*;

#[component]
pub fn ExternalIdentitiesSection(pubkey: String) -> Element {
    let mut identities = use_signal(Vec::<ExternalIdentityInfo>::new);
    let mut loading = use_signal(|| true);

    use_effect(use_reactive(&pubkey, move |pk| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            match nip39::fetch_external_identities(&pk).await {
                Ok(found) => identities.set(found),
                Err(e) => {
                    crate::utils::log_fetch_error("external identities", e);
                }
            }
            loading.set(false);
        });
    }));

    let identity_list = identities.read();
    if identity_list.is_empty() && !*loading.read() {
        return rsx! {};
    }

    rsx! {
        div { class: "py-2 px-4",
            if *loading.read() {
                div { class: "flex gap-2",
                    for _ in 0..3 {
                        div { class: "w-8 h-8 rounded-full bg-muted animate-pulse" }
                    }
                }
            } else {
                div { class: "flex flex-wrap gap-2",
                    for identity in identity_list.iter() {
                        IdentityBadge { identity: identity.clone() }
                    }
                }
            }
        }
    }
}

fn platform_icon(platform: &str) -> Element {
    match platform {
        "github" => rsx! { GitHubIcon { class: "w-3.5 h-3.5" } },
        "twitter" => rsx! { TwitterIcon { class: "w-3.5 h-3.5" } },
        "mastodon" => rsx! { MastodonIcon { class: "w-3.5 h-3.5" } },
        "telegram" => rsx! { TelegramIcon { class: "w-3.5 h-3.5" } },
        _ => rsx! {
            span { class: "w-3.5 h-3.5 inline-flex items-center justify-center text-xs font-bold", "?" }
        },
    }
}

#[component]
fn IdentityBadge(identity: ExternalIdentityInfo) -> Element {
    let proof_url = identity.proof_url();
    let display = identity.display_name().to_string();
    let platform = identity.platform.clone();
    let platform_label = match platform.as_str() {
        "github" => "GitHub",
        "twitter" => "X (Twitter)",
        "mastodon" => "Mastodon",
        "telegram" => "Telegram",
        _ => "External",
    };

    if proof_url.is_empty() {
        return rsx! {
            span {
                class: "inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-full bg-muted text-muted-foreground",
                title: "Verified on {platform_label}",
                {platform_icon(&platform)}
                span { "{display}" }
            }
        };
    }

    rsx! {
        a {
            href: "{proof_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-full bg-muted hover:bg-accent text-muted-foreground hover:text-foreground transition-colors",
            title: "Verified on {platform_label}",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            {platform_icon(&platform)}
            span { "{display}" }
        }
    }
}
