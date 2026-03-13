//! GRASP Server Registry
//!
//! Global registry of discovered GRASP (Git Repositories Announced on Servers with Protocol)
//! servers. Used to determine which clone URLs can be accessed directly vs. require CORS proxy.
//!
//! Servers are discovered dynamically from NIP-34 repository announcements when a domain
//! appears in both `clone` and `relays` tags.
use dioxus::prelude::*;
use std::collections::HashSet;
/// Global GRASP server registry using Dioxus GlobalSignal pattern
/// Thread-safe and integrates with reactivity system
static GRASP_SERVERS: GlobalSignal<HashSet<String>> = Signal::global(|| {
    let mut set = HashSet::new();
    set.insert("relay.ngit.dev".to_string());
    set.insert("gitnostr.com".to_string());
    set.insert("ngit.danconwaydev.com".to_string());
    set.insert("git.shakespeare.diy".to_string());
    set.insert("git-01.uid.ovh".to_string());
    set.insert("git-02.uid.ovh".to_string());
    set.insert("git.jb55.com".to_string());
    set
});
/// Add a discovered GRASP server domain
pub fn add_grasp_server(domain: &str) {
    let mut servers = GRASP_SERVERS.write();
    if servers.insert(domain.to_string()) {
        log::debug!("Discovered new GRASP server: {}", domain);
    }
}
/// Get all known GRASP servers as a vector
#[allow(dead_code)]
pub fn get_grasp_servers() -> Vec<String> {
    GRASP_SERVERS.read().iter().cloned().collect()
}
/// Check if a domain is a known GRASP server (for CORS proxy decision)
pub fn is_grasp_server(domain: &str) -> bool {
    GRASP_SERVERS.read().contains(domain)
}
