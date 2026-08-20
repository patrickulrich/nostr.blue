//! Relay row name with NIP-11 enrichment (issue #359).
//!
//! Renders a relay's NIP-11 name (falling back to the hostname), an icon
//! when available, and a "Paid" badge when the relay's NIP-11 document says
//! `limitations.payment_required`. Reads the session-level
//! [`crate::stores::relay::nip11_info`] cache, so it re-renders as documents
//! trickle in and never blocks first paint.

use crate::stores::relay::nip11_info;
use dioxus::prelude::*;

/// NIP-11-enriched relay name for use inside (or next to) a relay row.
#[component]
pub fn RelayDisplayName(url: String) -> Element {
    let info = nip11_info::lookup(&url);
    let host = crate::utils::relay::display_relay_url(&url);
    rsx! {
        span { class: "flex items-center gap-1.5 min-w-0",
            if let Some(icon) = info.as_ref().and_then(|i| i.icon.clone()) {
                img {
                    class: "w-4 h-4 rounded object-cover shrink-0 bg-muted",
                    src: "{icon}",
                    alt: "",
                }
            }
            if let Some(name) = info.as_ref().and_then(|i| i.name.clone()) {
                span {
                    class: "text-sm text-gray-900 dark:text-white truncate",
                    title: "{host}",
                    "{name}"
                }
            } else {
                span {
                    class: "font-mono text-sm text-gray-900 dark:text-white break-all",
                    "{host}"
                }
            }
            if info.as_ref().is_some_and(|i| i.paid) {
                span {
                    class: "px-1.5 py-0.5 shrink-0 bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200 rounded text-[10px] font-semibold uppercase",
                    title: "This relay requires payment (NIP-11 payment_required)",
                    "Paid"
                }
            }
        }
    }
}
