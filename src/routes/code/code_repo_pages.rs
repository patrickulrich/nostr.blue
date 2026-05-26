use crate::components::code::repo_pages_panel::RepoPagesPanel;
use crate::hooks::{use_nostr_resource_public, NostrResourceState};
use crate::services::git_hosting::repository::fetch_repository;
use crate::stores::auth_store;
use dioxus::prelude::*;

#[component]
pub fn CodeRepoPages(naddr: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let user_pubkey = auth.pubkey.clone().unwrap_or_default();
    let mut naddr_signal = use_signal(|| naddr.clone());
    use_effect(use_reactive!(|naddr| {
        naddr_signal.set(naddr);
    }));
    let repo = use_nostr_resource_public(move || {
        let naddr = naddr_signal.read().clone();
        async move { fetch_repository(&naddr).await }
    });
    let repo_state = repo.state();
    rsx! {
        div { class: "max-w-4xl mx-auto px-4 py-6",
            match &*repo_state.read() {
                NostrResourceState::Loading | NostrResourceState::Initializing => rsx! {
                    div { class: "text-center py-12 text-muted-foreground",
                        "Loading repository..."
                    }
                },
                NostrResourceState::Error(err) => rsx! {
                    div { class: "text-center py-12 text-red-500",
                        "{err}"
                    }
                },
                NostrResourceState::Loaded(r) => {
                    let name = r.name.clone().unwrap_or_default();
                    rsx! {
                        div { class: "mb-6",
                            h1 { class: "text-2xl font-bold text-foreground",
                                "Static Pages: {name}"
                            }
                            p { class: "text-muted-foreground mt-1",
                                "Publish and manage static pages for this repository."
                            }
                        }
                        RepoPagesPanel {
                            repo: r.clone(),
                            naddr: naddr.clone(),
                            is_owner: user_pubkey == r.pubkey,
                        }
                    }
                }
                NostrResourceState::AuthRequired => rsx! {},
            }
        }
    }
}
