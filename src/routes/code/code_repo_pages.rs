use crate::components::code::repo_pages_panel::RepoPagesPanel;
use crate::services::git_hosting::repository::fetch_repository;
use crate::stores::auth_store;
use crate::utils::nip34::Repository;
use dioxus::prelude::*;

#[component]
pub fn CodeRepoPages(naddr: String) -> Element {
    let mut repo = use_signal(|| None::<Repository>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let auth = auth_store::AUTH_STATE.read();
    let user_pubkey = auth.pubkey.clone().unwrap_or_default();

    use_effect(use_reactive(&naddr, move |naddr| {
        spawn(async move {
            match fetch_repository(&naddr).await {
                Ok(r) => {
                    repo.set(Some(r));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    }));

    rsx! {
        div { class: "max-w-4xl mx-auto px-4 py-6",
            if *loading.read() {
                div { class: "text-center py-12 text-muted-foreground",
                    "Loading repository..."
                }
            } else if let Some(err) = error.read().as_ref() {
                div { class: "text-center py-12 text-red-500",
                    "{err}"
                }
            } else if let Some(r) = repo.read().as_ref() {
                {
                    let name = r.name.clone().unwrap_or_default();
                    let _d_tag = crate::utils::nips::nip5a::slug_to_nsite_dtag(&name);
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
            }
        }
    }
}
