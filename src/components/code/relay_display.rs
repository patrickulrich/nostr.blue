//! GRASP Relay Display Component
//!
//! Sidebar widget that categorizes repository relays into GRASP servers
//! (Git Repositories Announced on Servers with Protocol) vs plain relays.
use dioxus::prelude::*;
use crate::stores::grasp_servers::is_grasp_server;

/// Extract domain from a relay URL (e.g., "wss://relay.example.com" -> "relay.example.com")
fn extract_domain(url: &str) -> &str {
    url.trim_start_matches("wss://")
        .trim_start_matches("ws://")
        .trim_end_matches('/')
        .split('/')
        .next()
        .unwrap_or(url)
}

/// Sidebar widget displaying repository relays categorized by type
#[component]
pub fn RelayDisplay(relays: Vec<String>) -> Element {
    if relays.is_empty() {
        return rsx! {};
    }

    let grasp_relays: Vec<&String> = relays.iter().filter(|r| is_grasp_server(extract_domain(r))).collect();
    let plain_relays: Vec<&String> = relays.iter().filter(|r| !is_grasp_server(extract_domain(r))).collect();

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4",
            h3 { class: "text-sm font-semibold text-foreground mb-3 flex items-center gap-2",
                svg {
                    class: "w-4 h-4",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M12 2a10 10 0 1 0 10 10 4 4 0 0 1-5-5 4 4 0 0 1-5-5" }
                    path { d: "M8.5 8.5v.01" }
                    path { d: "M16 15.5v.01" }
                    path { d: "M12 12v.01" }
                    path { d: "M11 17v.01" }
                    path { d: "M7 14v.01" }
                }
                "Relays"
            }
            div { class: "space-y-3",
                if !grasp_relays.is_empty() {
                    div {
                        div { class: "flex items-center gap-1.5 mb-2",
                            span { class: "inline-block w-2 h-2 rounded-full bg-green-500" }
                            span { class: "text-xs font-medium text-muted-foreground", "GRASP Servers" }
                        }
                        div { class: "space-y-1",
                            for relay in grasp_relays.iter() {
                                RelayItem { url: relay.to_string(), is_grasp: true }
                            }
                        }
                    }
                }
                if !plain_relays.is_empty() {
                    div {
                        div { class: "flex items-center gap-1.5 mb-2",
                            span { class: "inline-block w-2 h-2 rounded-full bg-muted-foreground/50" }
                            span { class: "text-xs font-medium text-muted-foreground", "Relays" }
                        }
                        div { class: "space-y-1",
                            for relay in plain_relays.iter() {
                                RelayItem { url: relay.to_string(), is_grasp: false }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Individual relay item display
#[component]
fn RelayItem(url: String, is_grasp: bool) -> Element {
    let domain = extract_domain(&url);
    let dot_class = if is_grasp {
        "inline-block w-1.5 h-1.5 rounded-full bg-green-500/50"
    } else {
        "inline-block w-1.5 h-1.5 rounded-full bg-muted-foreground/30"
    };
    rsx! {
        div { class: "flex items-center gap-2 text-xs text-muted-foreground",
            span { class: dot_class }
            span { class: "font-mono truncate", "{domain}" }
        }
    }
}
