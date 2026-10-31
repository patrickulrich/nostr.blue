//! Web Bookmark Detail Page (NIP-B0, kind 39701)
//!
//! Full-page view of a referenced web bookmark. Reuses `WebBookmarkCard`
//! (read-only: `on_edit: None`) — the card already renders the URL, title,
//! description, image, and favorite/archived state.
use crate::components::webbookmark_card::{WebBookmarkCard, WebBookmarkCardSkeleton};
use crate::stores::nostr_client;
use dioxus::prelude::*;

/// Web bookmark detail page component.
#[component]
pub fn WebBookmarkViewer(naddr: String) -> Element {
    let mut event = use_signal(|| None::<nostr::event::Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    use_effect(use_reactive(&naddr, move |addr| {
        if !*nostr_client::CLIENT_INITIALIZED.read() {
            return;
        }
        spawn(async move {
            loading.set(true);
            error.set(None);
            match crate::utils::nip19::parse_naddr(&addr) {
                Ok(parsed) => {
                    match nostr_client::fetch_event_by_coordinate_with_relays(
                        parsed.kind,
                        parsed.pubkey.clone(),
                        parsed.identifier,
                        parsed.relay_hints,
                    )
                    .await
                    {
                        Ok(Some(e)) => {
                            event.set(Some(e));
                        }
                        Ok(None) => {
                            error.set(Some("Bookmark not found".to_string()));
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                }
                Err(e) => {
                    error.set(Some(format!("Invalid bookmark address: {e}")));
                }
            }
            loading.set(false);
        });
    }));

    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center",
                div { class: "animate-pulse text-muted-foreground", "Initializing..." }
            }
        };
    }

    rsx! {
        div { class: "min-h-screen pb-20 max-w-2xl mx-auto",
            if *loading.read() {
                div { class: "p-4",
                    WebBookmarkCardSkeleton {}
                }
            } else if let Some(err) = error.read().as_ref() {
                div { class: "p-4",
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{err}"
                    }
                }
            } else if let Some(ev) = event.read().clone() {
                div { class: "p-4",
                    // Read-only view — editing stays in the /webbookmarks
                    // management surface where the signer-gated actions live.
                    WebBookmarkCard { event: ev, on_edit: None }
                }
            }
        }
    }
}
