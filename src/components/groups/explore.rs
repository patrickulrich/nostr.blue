use crate::stores::social::group_store::{
    create_group, fetch_groups_from_relay, RECOMMENDED_GROUP_RELAYS,
};
use crate::components::groups::card::GroupCard;
use dioxus::prelude::*;

#[component]
pub fn GroupExplore() -> Element {
    let mut relay_input = use_signal(String::new);
    let mut groups = use_signal(Vec::<crate::stores::social::group_store::Group>::new);
    let mut loading = use_signal(|| false);
    let mut selected_relay = use_signal(|| None::<String>);
    let mut search_query = use_signal(String::new);
    let mut show_create = use_signal(|| false);
    let mut create_relay = use_signal(String::new);
    let mut creating = use_signal(|| false);

    let load_groups = move |relay_url: String| {
        spawn(async move {
            loading.set(true);
            selected_relay.set(Some(relay_url.clone()));
            match fetch_groups_from_relay(&relay_url).await {
                Ok(g) => {
                    groups.set(g);
                }
                Err(e) => {
                    log::error!("Failed to fetch groups from {}: {}", relay_url, e);
                    groups.set(Vec::new());
                }
            }
            loading.set(false);
        });
    };

    let filtered_groups: Vec<_> = {
        let q = search_query().to_lowercase();
        if q.is_empty() {
            groups()
        } else {
            groups()
                .into_iter()
                .filter(|g| {
                    g.name
                        .as_ref()
                        .map(|n| n.to_lowercase().contains(&q))
                        .unwrap_or(false)
                        || g.about
                            .as_ref()
                            .map(|a| a.to_lowercase().contains(&q))
                            .unwrap_or(false)
                        || g.id.to_lowercase().contains(&q)
                })
                .collect()
        }
    };

    rsx! {
        div { class: "space-y-4",
            div { class: "space-y-2",
                label { class: "text-sm font-medium text-foreground", "Group Relay" }
                div { class: "flex gap-2",
                    input {
                        class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "wss://groups.example.com",
                        value: "{relay_input}",
                        oninput: move |e| relay_input.set(e.value()),
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                        disabled: relay_input().is_empty() || *loading.read(),
                        onclick: move |_| {
                            let url = relay_input();
                            if !url.is_empty() {
                                load_groups(url);
                            }
                        },
                        if *loading.read() { "Loading..." } else { "Browse" }
                    }
                }
            }

            div { class: "space-y-2",
                label { class: "text-sm font-medium text-muted-foreground", "Popular Relays" }
                div { class: "flex flex-wrap gap-2",
                    for relay in RECOMMENDED_GROUP_RELAYS {
                        {
                            let relay_str = relay.to_string();
                            let relay_label = relay_str.clone();
                            let relay_key = relay_str.clone();
                            rsx! {
                                button {
                                    key: "{relay_key}",
                                    class: if selected_relay() == Some(relay.to_string()) {
                                        "px-3 py-1.5 rounded-lg text-sm bg-primary text-primary-foreground"
                                    } else {
                                        "px-3 py-1.5 rounded-lg text-sm bg-accent text-foreground hover:bg-accent/80 transition"
                                    },
                                    onclick: move |_| {
                                        load_groups(relay_str.clone());
                                    },
                                    "{relay_label}"
                                }
                            }
                        }
                    }
                }
            }

            if !groups().is_empty() {
                div { class: "flex gap-2",
                    input {
                        class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary text-sm",
                        r#type: "text",
                        placeholder: "Filter groups...",
                        value: "{search_query}",
                        oninput: move |e| search_query.set(e.value()),
                    }
                    button {
                        class: "px-3 py-2 bg-accent text-foreground rounded-lg hover:bg-accent/80 transition text-sm",
                        onclick: move |_| {
                            if let Some(r) = selected_relay.as_ref() {
                                create_relay.set(r.clone());
                            }
                            show_create.set(true);
                        },
                        "+ Create"
                    }
                }
            }

            if *loading.read() {
                div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                    for i in 0..4 {
                        crate::components::groups::GroupCardSkeleton { key: "{i}" }
                    }
                }
            } else if !filtered_groups.is_empty() {
                div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                    for group in filtered_groups {
                        GroupCard { key: "{group.relay_url}-{group.id}", group }
                    }
                }
            } else if selected_relay().is_some() {
                div { class: "text-center py-8 text-muted-foreground",
                    "No groups found on this relay"
                }
            }

            if show_create() {
                div {
                    class: "fixed inset-0 z-40 bg-black/50 backdrop-blur-sm flex items-center justify-center",
                    onclick: move |_| show_create.set(false),
                    div {
                        class: "bg-card border border-border rounded-lg p-6 w-full max-w-md mx-4 space-y-4",
                        onclick: move |e| e.stop_propagation(),
                        h3 { class: "text-lg font-semibold text-foreground", "Create Group" }
                        p { class: "text-sm text-muted-foreground",
                            "This will create a new group on the selected relay. The relay may assign you as the group owner (king)."
                        }
                        div { class: "space-y-2",
                            label { class: "text-sm text-foreground", "Relay" }
                            input {
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary",
                                value: "{create_relay}",
                                oninput: move |e| create_relay.set(e.value()),
                            }
                        }
                        div { class: "flex gap-2 justify-end",
                            button {
                                class: "px-4 py-2 rounded-lg text-sm text-muted-foreground hover:bg-accent transition",
                                onclick: move |_| show_create.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50 text-sm",
                                disabled: create_relay().is_empty() || *creating.read(),
                                onclick: {
                                    move |_| {
                                        let relay = create_relay();
                                        if relay.is_empty() { return; }
                                        creating.set(true);
                                        spawn(async move {
                                            match create_group(&relay).await {
                                                Ok(_id) => {
                                                    log::info!("Group creation requested on {}", relay);
                                                }
                                                Err(e) => {
                                                    log::error!("Failed to create group: {}", e);
                                                }
                                            }
                                            creating.set(false);
                                            show_create.set(false);
                                            load_groups(relay);
                                        });
                                    }
                                },
                                if *creating.read() { "Creating..." } else { "Create" }
                            }
                        }
                    }
                }
            }
        }
    }
}
