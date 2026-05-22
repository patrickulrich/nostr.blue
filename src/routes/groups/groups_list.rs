use crate::components::groups::card::{GroupCard, GroupCardSkeleton};
use crate::components::groups::explore::GroupExplore;
use crate::stores::nostr_client::CLIENT_INITIALIZED;
use crate::stores::social::group_store::{fetch_user_groups, GROUPS_LOADING};
use dioxus::prelude::*;

#[component]
pub fn Groups() -> Element {
    let mut groups = use_signal(Vec::<crate::stores::social::group_store::Group>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let mut active_tab = use_signal(|| 0u8);

    use_effect(move || {
        let client_initialized = *CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            error_msg.set(None);
            match fetch_user_groups().await {
                Ok(g) => {
                    groups.set(g);
                }
                Err(e) => {
                    log::error!("Failed to fetch groups: {}", e);
                    error_msg.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    let loading_global = *GROUPS_LOADING.read();

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "flex items-center justify-between px-4 py-3",
                    h1 { class: "text-xl font-bold text-foreground", "Groups" }
                }
                div { class: "flex border-b border-border",
                    button {
                        class: if active_tab() == 0 {
                            "flex-1 px-4 py-2 text-sm font-medium text-foreground border-b-2 border-primary"
                        } else {
                            "flex-1 px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                        },
                        onclick: move |_| active_tab.set(0),
                        "My Groups"
                    }
                    button {
                        class: if active_tab() == 1 {
                            "flex-1 px-4 py-2 text-sm font-medium text-foreground border-b-2 border-primary"
                        } else {
                            "flex-1 px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                        },
                        onclick: move |_| active_tab.set(1),
                        "Explore"
                    }
                }
            }

            div { class: "p-4",
                if active_tab() == 0 {
                    if !*CLIENT_INITIALIZED.read() || *loading.read() || loading_global {
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                            for i in 0..4 {
                                GroupCardSkeleton { key: "{i}" }
                            }
                        }
                    } else if let Some(err) = error_msg.read().as_ref() {
                        div { class: "text-center py-8 text-red-500",
                            "Error: {err}"
                        }
                    } else if groups().is_empty() {
                        div { class: "text-center py-12",
                            p { class: "text-muted-foreground mb-4",
                                "You haven't joined any groups yet"
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                onclick: move |_| active_tab.set(1),
                                "Explore Groups"
                            }
                        }
                    } else {
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                            for group in groups() {
                                GroupCard { key: "{group.relay_url}-{group.id}", group }
                            }
                        }
                    }
                } else {
                    GroupExplore {}
                }
            }
        }
    }
}
