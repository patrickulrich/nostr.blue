use crate::components::RttBadge;
use crate::routes::Route;
use crate::stores::{nostr_client, relay};
use crate::utils::nip66::RelayDiscoveryData;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct RelayDiscoveryCardProps {
    pub data: RelayDiscoveryData,
}

fn network_badge_class(nt: &str) -> &'static str {
    match nt {
        "clearnet" => "bg-blue-500/20 text-blue-400 border-blue-500/30",
        "tor" => "bg-purple-500/20 text-purple-400 border-purple-500/30",
        "i2p" => "bg-indigo-500/20 text-indigo-400 border-indigo-500/30",
        "loki" => "bg-orange-500/20 text-orange-400 border-orange-500/30",
        _ => "bg-muted text-muted-foreground border-border",
    }
}

fn requirement_display(reqs: &[(String, bool)]) -> Vec<String> {
    reqs.iter()
        .map(|(val, negated)| {
            if *negated {
                format!("!{val}")
            } else {
                val.clone()
            }
        })
        .collect()
}

#[component]
pub fn RelayDiscoveryCard(props: RelayDiscoveryCardProps) -> Element {
    let mut adding = use_signal(|| false);
    let mut added = use_signal(|| false);
    let d = &props.data;
    let relay_url = d.relay_url.clone();
    let display_url = {
        if let Ok(parsed) = nostr::Url::parse(&relay_url) {
            let host = parsed.host_str().unwrap_or(&relay_url);
            match parsed.port() {
                Some(port) => format!("{}:{}", host, port),
                None => host.to_string(),
            }
        } else {
            relay_url.clone()
        }
    };
    let route = Route::RelayDetail {
        relay_id: crate::utils::relay::encode_relay_route_id(&relay_url),
    };
    let nip_count = d.supported_nips.len();
    let reqs = requirement_display(&d.requirements);

    let handle_add = move |_| {
        if *adding.read() || *added.read() {
            return;
        }
        let url = relay_url.clone();
        adding.set(true);
        spawn(async move {
            if let Some(client) = nostr_client::get_client() {
                let _ = relay::pool::add_relay(&client, &url).await;
                added.set(true);
            }
            adding.set(false);
        });
    };

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 hover:border-foreground/20 transition",
            div { class: "flex items-start justify-between gap-2 mb-3",
                Link {
                    to: route,
                    class: "min-w-0",
                    p { class: "font-mono text-sm text-foreground hover:underline truncate", "{display_url}" }
                }
                if *added.read() {
                    span { class: "text-xs text-green-400 shrink-0", "Added" }
                } else if *adding.read() {
                    span { class: "text-xs text-muted-foreground shrink-0", "Adding..." }
                } else {
                    button {
                        class: "text-xs text-blue-600 dark:text-blue-400 hover:underline shrink-0",
                        onclick: handle_add,
                        "Add"
                    }
                }
            }

            if d.network_type.is_some() || !d.relay_types.is_empty() {
                div { class: "flex flex-wrap gap-1.5 mb-2",
                    if let Some(ref nt) = d.network_type {
                        span { class: "px-1.5 py-0.5 rounded text-xs border {network_badge_class(nt)}", "{nt}" }
                    }
                    for rt in &d.relay_types {
                        span { key: "{rt}", class: "px-1.5 py-0.5 rounded text-xs border bg-muted text-muted-foreground border-border", "{rt}" }
                    }
                }
            }

            div { class: "flex flex-wrap gap-1.5 mb-2",
                RttBadge { label: "Open".to_string(), ms: d.rtt_open }
                RttBadge { label: "Read".to_string(), ms: d.rtt_read }
                RttBadge { label: "Write".to_string(), ms: d.rtt_write }
            }

            if !reqs.is_empty() {
                div { class: "flex flex-wrap gap-1.5 mb-2",
                    for req in &reqs {
                        span {
                            key: "{req}",
                            class: if req.starts_with('!') {
                                "px-1.5 py-0.5 rounded text-xs border bg-muted/50 text-muted-foreground border-border"
                            } else {
                                "px-1.5 py-0.5 rounded text-xs border bg-amber-500/20 text-amber-400 border-amber-500/30"
                            },
                            "{req}"
                        }
                    }
                }
            }

            if nip_count > 0 {
                div { class: "mb-2",
                    span { class: "px-1.5 py-0.5 rounded text-xs bg-primary/20 text-primary border border-primary/30",
                        "{nip_count} NIPs"
                    }
                }
            }

            if !d.topics.is_empty() {
                div { class: "flex flex-wrap gap-1",
                    for topic in &d.topics {
                        span { key: "{topic}", class: "px-1.5 py-0.5 rounded text-xs bg-muted text-muted-foreground", "#{topic}" }
                    }
                }
            }
        }
    }
}
