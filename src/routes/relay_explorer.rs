use crate::components::RelayDiscoveryCard;
use crate::routes::Route;
use crate::stores::nostr_client;
use crate::utils::nip66;
use dioxus::prelude::*;
use std::time::Duration;

const NETWORK_TYPES: &[&str] = &["all", "clearnet", "tor", "i2p", "loki"];
const SORT_OPTIONS: &[&str] = &["rtt", "nips", "name"];

#[component]
pub fn RelayExplorer() -> Element {
    let mut discoveries = use_signal(Vec::<nip66::RelayDiscoveryData>::new);
    let mut is_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut fetch_gen = use_signal(|| 0u32);
    let mut search_query = use_signal(String::new);
    let mut search_input = use_signal(String::new);
    let mut filter_network = use_signal(|| "all".to_string());
    let mut sort_by = use_signal(|| "rtt".to_string());
    let mut refetch_trigger = use_signal(|| 0u32);

    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let _ = refetch_trigger.read();
        if !client_initialized {
            return;
        }
        let gen = *fetch_gen.peek() + 1;
        fetch_gen.set(gen);
        is_loading.set(true);
        error.set(None);

        spawn(async move {
            let filter = nip66::discovery_filter(1000);
            let result = nostr_client::fetch_events_from_connected_relays(
                filter,
                Duration::from_secs(15),
            )
            .await;

            if *fetch_gen.peek() != gen {
                return;
            }

            match result {
                Ok(events) => {
                    let parsed: Vec<nip66::RelayDiscoveryData> = events
                        .iter()
                        .filter_map(nip66::parse_relay_discovery)
                        .collect();
                    let aggregated = nip66::aggregate_discoveries(&parsed);
                    discoveries.set(aggregated);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            is_loading.set(false);
        });
    });

    let filtered = use_memo(move || {
        let query = search_query.read().to_lowercase();
        let network = filter_network.read().clone();
        let sort = sort_by.read().clone();
        let mut items: Vec<nip66::RelayDiscoveryData> = discoveries
            .read()
            .iter()
            .filter(|d| {
                if !query.is_empty() {
                    let matches_url = d.relay_url.to_lowercase().contains(&query);
                    let matches_topic = d.topics.iter().any(|t| t.to_lowercase().contains(&query));
                    if !matches_url && !matches_topic {
                        return false;
                    }
                }
                if network != "all" {
                    match &d.network_type {
                        Some(nt) if nt == network.as_str() => {}
                        _ => return false,
                    }
                }
                true
            })
            .cloned()
            .collect();

        match sort.as_str() {
            "rtt" => {
                items.sort_by(|a, b| {
                    let a_rtt = a.rtt_open.unwrap_or(u64::MAX);
                    let b_rtt = b.rtt_open.unwrap_or(u64::MAX);
                    a_rtt.cmp(&b_rtt)
                });
            }
            "nips" => {
                items.sort_by_key(|b| std::cmp::Reverse(b.supported_nips.len()));
            }
            "name" => {
                items.sort_by(|a, b| a.relay_url.cmp(&b.relay_url));
            }
            _ => {}
        }

        items
    });

    let total_count = discoveries.read().len();

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-30 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "max-w-6xl mx-auto px-4 py-3",
                    div { class: "flex items-center justify-between mb-3",
                        div {
                            Link {
                                to: Route::SettingsRelays {},
                                class: "text-blue-600 dark:text-blue-400 hover:underline flex items-center gap-2 text-sm mb-1",
                                "← Back to Relay Settings"
                            }
                            h1 { class: "text-xl font-bold text-foreground", "Relay Explorer" }
                            p { class: "text-xs text-muted-foreground mt-0.5",
                                "Discover relays from NIP-66 monitor data. {total_count} relays found."
                            }
                        }
                    }

                    div { class: "flex flex-wrap items-center gap-2",
                        div { class: "flex-1 min-w-[200px] max-w-md",
                            div { class: "relative",
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-1.5 pl-8 bg-muted border border-border rounded-lg text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50",
                                    placeholder: "Search relays or topics...",
                                    value: "{search_input}",
                                    oninput: move |e| {
                                        search_input.set(e.value());
                                    },
                                    onkeydown: move |e: KeyboardEvent| {
                                        if e.key() == Key::Enter {
                                            search_query.set(search_input.read().clone());
                                        }
                                    },
                                }
                                span { class: "absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground",
                                    "🔍"
                                }
                            }
                        }
                        div { class: "flex items-center gap-2",
                            select {
                                class: "px-2 py-1.5 bg-muted border border-border rounded-lg text-sm text-foreground focus:outline-none",
                                value: "{filter_network}",
                                onchange: move |e| filter_network.set(e.value()),
                                for nt in NETWORK_TYPES {
                                    option { value: "{nt}",
                                        selected: *filter_network.read() == *nt,
                                        {nt.to_uppercase()}
                                    }
                                }
                            }
                            select {
                                class: "px-2 py-1.5 bg-muted border border-border rounded-lg text-sm text-foreground focus:outline-none",
                                value: "{sort_by}",
                                onchange: move |e| sort_by.set(e.value()),
                                for opt in SORT_OPTIONS {
                                    option { value: "{opt}",
                                        selected: *sort_by.read() == *opt,
                                        {match *opt {
                                            "rtt" => "Sort: RTT",
                                            "nips" => "Sort: NIPs",
                                            "name" => "Sort: Name",
                                            _ => opt,
                                        }}
                                    }
                                }
                            }
                            button {
                                class: "px-2 py-1.5 bg-muted border border-border rounded-lg text-sm text-muted-foreground hover:text-foreground transition",
                                title: "Refresh",
                                onclick: move |_| {
                                    refetch_trigger.set(refetch_trigger() + 1);
                                },
                                "↻"
                            }
                        }
                    }
                }
            }

            div { class: "max-w-6xl mx-auto px-4 py-4",
                if *is_loading.read() {
                    div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                        for _ in 0..9 {
                            div { class: "bg-card border border-border rounded-lg p-4 animate-pulse",
                                div { class: "h-4 bg-muted rounded w-3/4 mb-3" }
                                div { class: "h-3 bg-muted rounded w-1/2 mb-2" }
                                div { class: "h-3 bg-muted rounded w-2/3 mb-2" }
                                div { class: "flex gap-1.5",
                                    div { class: "h-5 bg-muted rounded w-16" }
                                    div { class: "h-5 bg-muted rounded w-16" }
                                    div { class: "h-5 bg-muted rounded w-16" }
                                }
                            }
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "bg-card border border-border rounded-lg p-6 text-center",
                        p { class: "text-red-500 mb-3", "{err}" }
                        button {
                            class: "px-4 py-2 bg-primary text-white rounded-lg hover:opacity-80 transition text-sm",
                            onclick: move |_| { refetch_trigger.set(refetch_trigger() + 1); },
                            "Try Again"
                        }
                    }
                } else if filtered.read().is_empty() && !search_query.read().is_empty() {
                    div { class: "bg-card border border-border rounded-lg p-6 text-center",
                        p { class: "text-muted-foreground", "No relays match your search." }
                    }
                } else if filtered.read().is_empty() {
                    div { class: "bg-card border border-border rounded-lg p-6 text-center",
                        p { class: "text-muted-foreground mb-3", "No relay discovery data found." }
                        p { class: "text-xs text-muted-foreground", "Make sure you're connected to relays that carry NIP-66 monitor events." }
                    }
                } else {
                    div { class: "text-xs text-muted-foreground mb-3",
                        "{filtered.read().len()} relays"
                    }
                    div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                        for data in filtered.read().iter().cloned() {
                            RelayDiscoveryCard { key: "{data.relay_url}", data }
                        }
                    }
                }
            }
        }
    }
}
