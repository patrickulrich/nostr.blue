use crate::components::ArticleCoverUploader;
use crate::routes::Route;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::social::channel_store::create_channel;
use dioxus::prelude::*;

#[component]
pub fn ChatNew() -> Element {
    let mut name = use_signal(String::new);
    let mut about = use_signal(String::new);
    let picture = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let has_signer = use_memo(move || *HAS_SIGNER.read());
    let nav = navigator();

    let handle_submit = move |_| {
        if *creating.peek() {
            return;
        }
        let n = name.read().trim().to_string();
        let a = about.read().trim().to_string();
        let p = picture.read().trim().to_string();

        if n.is_empty() {
            error.set(Some("Channel name is required".to_string()));
            return;
        }

        creating.set(true);
        error.set(None);

        spawn(async move {
            match create_channel(&n, &a, &p).await {
                Ok(event) => {
                    let channel_id = event.id.to_hex();
                    nav.push(Route::ChatDetail { channel_id });
                }
                Err(e) => {
                    error.set(Some(format!("Failed to create channel: {}", e)));
                    creating.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "min-h-screen",
            // Header
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border px-4 py-3",
                div { class: "flex items-center gap-3",
                    Link {
                        to: Route::Chats {},
                        class: "p-2 hover:bg-accent rounded-lg transition",
                        "← Back"
                    }
                    h1 { class: "text-xl font-bold", "New Channel" }
                }
            }

            if !*has_signer.read() {
                div { class: "p-4",
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 text-destructive rounded-lg",
                        "You need to sign in to create a channel."
                    }
                }
            } else {
                div { class: "p-4 max-w-lg mx-auto",
                    div { class: "space-y-4",
                        // Name
                        div {
                            label { class: "block text-sm font-medium mb-1", r#for: "channel-name",
                                "Channel Name "
                                span { class: "text-red-500", "*" }
                            }
                            input {
                                id: "channel-name",
                                class: "w-full px-3 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "e.g. Nostr Dev Chat",
                                value: "{name}",
                                disabled: *creating.read(),
                                oninput: move |e| name.set(e.value()),
                            }
                        }

                        // About
                        div {
                            label { class: "block text-sm font-medium mb-1", r#for: "channel-about",
                                "Description"
                            }
                            textarea {
                                id: "channel-about",
                                class: "w-full px-3 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                                rows: "3",
                                placeholder: "What is this channel about?",
                                value: "{about}",
                                disabled: *creating.read(),
                                oninput: move |e| about.set(e.value()),
                            }
                        }

                        // Picture
                        ArticleCoverUploader {
                            cover_url: picture,
                            label: "Upload Channel Picture".to_string(),
                            show_url_input: true,
                        }

                        // Error
                        if let Some(ref err) = *error.read() {
                            div { class: "p-3 bg-destructive/10 border border-destructive/20 text-destructive rounded-lg text-sm",
                                "{err}"
                            }
                        }

                        // Submit
                        button {
                            class: "w-full py-3 bg-primary text-primary-foreground hover:bg-primary/90 font-medium rounded-lg transition disabled:opacity-50 disabled:cursor-not-allowed",
                            disabled: *creating.read() || name.read().trim().is_empty(),
                            onclick: handle_submit,
                            if *creating.read() {
                                span { class: "inline-flex items-center gap-2",
                                    span { class: "inline-block w-4 h-4 border-2 border-primary-foreground border-t-transparent rounded-full animate-spin" }
                                    "Creating Channel..."
                                }
                            } else {
                                "Create Channel"
                            }
                        }
                    }
                }
            }
        }
    }
}
