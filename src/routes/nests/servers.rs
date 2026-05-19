use crate::components::icons;
use crate::routes::Route;
use crate::stores::auth_store;
use crate::stores::nostr_client::{self, CLIENT_INITIALIZED};
use crate::utils::nips::nip53::{parse_nests_servers, NestsServer};
use dioxus::prelude::Event as DioxusEvent;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

#[component]
pub fn NestServers() -> Element {
    let navigator = use_navigator();
    let is_logged_in = use_memo(move || auth_store::get_pubkey().is_some());
    let mut servers = use_signal(Vec::<NestsServer>::new);
    let mut loading = use_signal(|| true);
    let mut relay_url = use_signal(String::new);
    let mut auth_url = use_signal(String::new);
    let mut is_saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    use_effect(use_reactive(&*CLIENT_INITIALIZED.read(), move |client_ready| {
        if !client_ready || !is_logged_in() {
            return;
        }
        spawn(async move {
            loading.set(true);
            if let Some(pk) = auth_store::get_pubkey() {
                let author = PublicKey::from_hex(&pk).unwrap_or_else(|_| {
                    PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                });
                let filter = nostr_sdk::Filter::new()
                    .kind(Kind::Custom(10112))
                    .author(author)
                    .limit(1);
                match nostr_client::fetch_events_aggregated(
                    filter,
                    std::time::Duration::from_secs(10),
                )
                .await
                {
                    Ok(events) => {
                        if let Some(event) = events.first() {
                            let parsed = parse_nests_servers(event);
                            if !parsed.is_empty() {
                                servers.set(parsed);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to fetch nest servers: {}", e);
                    }
                }
            }
            loading.set(false);
        });
    }));

    if !is_logged_in() {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center",
                div { class: "text-center space-y-4",
                    h1 { class: "text-xl font-bold", "Authentication Required" }
                    p { class: "text-muted-foreground", "Please log in to manage servers" }
                    Link {
                        to: Route::NestsHome {},
                        class: "inline-block mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                        "Back to Nests"
                    }
                }
            }
        };
    }

    let handle_add = move |_: DioxusEvent<MouseData>| {
        let relay_val = relay_url.read().clone();
        let auth_val = auth_url.read().clone();
        if relay_val.trim().is_empty() || auth_val.trim().is_empty() {
            error.set(Some("Both relay URL and auth URL are required".to_string()));
            return;
        }
        if !relay_val.trim().starts_with("wss://") && !relay_val.trim().starts_with("ws://") {
            error.set(Some("Relay URL must start with wss:// or ws://".to_string()));
            return;
        }
        if !auth_val.trim().starts_with("https://") && !auth_val.trim().starts_with("http://") {
            error.set(Some("Auth URL must start with https:// or http://".to_string()));
            return;
        }
        let mut current = servers.write();
        current.push(NestsServer {
            relay_url: relay_val.trim().to_string(),
            auth_url: auth_val.trim().to_string(),
        });
        drop(current);
        relay_url.set(String::new());
        auth_url.set(String::new());
        error.set(None);
    };

    let handle_save = move |_: DioxusEvent<MouseData>| {
        if *is_saving.read() {
            return;
        }
        is_saving.set(true);
        error.set(None);
        let servers_val = servers.read().clone();
        let mut is_saving_cb = is_saving;
        let mut error_cb = error;
        spawn(async move {
            let tags = crate::utils::nips::nip53::build_nests_servers_tags(&servers_val);
            let builder = EventBuilder::new(Kind::Custom(10112), "").tags(tags);
            match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
                Ok(event) => {
                    crate::stores::publish_queue::enqueue(
                        event,
                        crate::stores::publish_queue::types::QueueEventType::Other(
                            "nest-servers".to_string(),
                        ),
                        None,
                        std::collections::HashMap::new(),
                    )
                    .await;
                    is_saving_cb.set(false);
                    navigator.push(Route::NestsHome {});
                }
                Err(e) => {
                    error_cb.set(Some(format!("Failed to save: {}", e)));
                    is_saving_cb.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-30 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-3",
                    Link {
                        to: Route::NestsHome {},
                        class: "p-2 hover:bg-muted rounded-lg transition",
                        span {
                            dangerous_inner_html: icons::ARROW_LEFT,
                        }
                    }
                    h1 { class: "text-lg font-bold", "Nest Servers" }
                }
            }
            div { class: "p-4 max-w-xl mx-auto space-y-6",
                if *loading.read() {
                    div { class: "flex items-center justify-center py-8",
                        div { class: "animate-pulse text-muted-foreground",
                            crate::components::icons::RadioIcon { class: "w-8 h-8".to_string() }
                        }
                    }
                } else {
                    if let Some(err) = error.read().as_ref() {
                        div { class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                            "{err}"
                        }
                    }

                    div { class: "space-y-3",
                        h2 { class: "text-sm font-semibold text-muted-foreground", "Current Servers" }
                        if servers.read().is_empty() {
                            p { class: "text-sm text-muted-foreground", "No servers configured. Add one below." }
                        } else {
                            div { class: "divide-y divide-border border border-border rounded-lg",
                                for (i, server) in servers.read().iter().enumerate() {
                                    div {
                                        key: "{i}",
                                        class: "flex items-center justify-between px-4 py-3",
                                        div { class: "min-w-0",
                                            p { class: "text-sm font-medium truncate", "{server.relay_url}" }
                                            p { class: "text-xs text-muted-foreground truncate", "{server.auth_url}" }
                                        }
                                        button {
                                            class: "p-1.5 rounded-lg hover:bg-accent text-muted-foreground transition shrink-0 ml-2",
                                            onclick: {
                                                let mut servers = servers;
                                                move |_: DioxusEvent<MouseData>| {
                                                    let mut current = servers.write();
                                                    if i < current.len() {
                                                        current.remove(i);
                                                    }
                                                }
                                            },
                                            crate::components::icons::TrashIcon { class: "w-4 h-4".to_string() }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "space-y-3",
                        h2 { class: "text-sm font-semibold text-muted-foreground", "Add Server" }
                        div { class: "space-y-2",
                            input {
                                r#type: "url",
                                class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary text-sm",
                                placeholder: "Relay URL (e.g., wss://moq.nostrnests.com:4443)",
                                value: "{relay_url}",
                                oninput: move |e| relay_url.set(e.value()),
                            }
                            input {
                                r#type: "url",
                                class: "w-full px-3 py-2 bg-muted border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary text-sm",
                                placeholder: "Auth URL (e.g., https://moq-auth.nostrnests.com)",
                                value: "{auth_url}",
                                oninput: move |e| auth_url.set(e.value()),
                            }
                        }
                        button {
                            class: "px-4 py-2 bg-muted hover:bg-accent text-sm font-medium rounded-lg transition",
                            onclick: handle_add,
                            "Add Server"
                        }
                    }

                    div { class: "border-t border-border pt-4",
                        button {
                            class: "w-full py-3 bg-blue-500 hover:bg-blue-600 text-white font-bold rounded-xl transition disabled:opacity-50",
                            disabled: *is_saving.read() || servers.read().is_empty(),
                            onclick: handle_save,
                            if *is_saving.read() { "Saving..." } else { "Save Server List" }
                        }
                    }
                }
            }
        }
    }
}
