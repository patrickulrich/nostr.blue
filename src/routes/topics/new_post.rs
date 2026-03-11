//! Topic New Post Page
//! Full-page composer with topic autocomplete
use crate::components::TopicPostComposer;
use crate::routes::Route;
use crate::stores::nostr_client::HAS_SIGNER;
use dioxus::prelude::*;

#[component]
pub fn TopicNewPost() -> Element {
    let has_signer = *HAS_SIGNER.read();
    let nav = navigator();

    rsx! {
        div {
            class: "w-full max-w-2xl px-4 py-4",
            h1 { class: "text-2xl font-bold text-foreground mb-4", "New Post" }
            if has_signer {
                TopicPostComposer {
                    on_success: move |_event_id: String| {
                        nav.push(Route::TopicsHome {});
                    },
                }
            } else {
                div {
                    class: "bg-muted border border-border rounded-lg p-8 text-center",
                    p { class: "text-muted-foreground", "Sign in to create a topic post." }
                }
            }
        }
    }
}
