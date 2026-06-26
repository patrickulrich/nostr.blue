use crate::components::ReportModal;
use crate::stores::nostr_client;
use crate::stores::topic_store::{pin_post, unpin_post, MAX_PINS};
use crate::stores::topic_store::TopicPost;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};

#[component]
pub fn TopicPostMenu(
    post: TopicPost,
    topic: String,
    #[props(default = None)] creator_pubkey: Option<String>,
    #[props(default = false)] is_pinned: bool,
    #[props(default)] current_pins: Vec<String>,
    #[props(default)] on_pin_toggle: Option<EventHandler<()>>,
) -> Element {
    let toast = consume_toast();
    let mut is_open = use_signal(|| false);
    let mut show_report = use_signal(|| false);
    let mut pin_loading = use_signal(|| false);

    let my_pubkey = crate::stores::auth_store::get_pubkey();
    let can_pin = creator_pubkey
        .as_ref()
        .map(|cp| my_pubkey.as_ref() == Some(cp))
        .unwrap_or(false);

    let post_url = format!(
        "/topics/t/{}/post/{}",
        topic,
        crate::utils::nip19_urls::note_route_id(&post.id, Some(&post.pubkey))
    );

    rsx! {
        div {
            class: "relative",
            button {
                class: "p-1.5 rounded-full hover:bg-accent transition-colors text-muted-foreground hover:text-foreground",
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    is_open.set(!is_open());
                },
                svg {
                    class: "w-4 h-4",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "12", cy: "12", r: "1" }
                    circle { cx: "19", cy: "12", r: "1" }
                    circle { cx: "5", cy: "12", r: "1" }
                }
            }
            if *is_open.read() {
                {
                    let topic_for_pin = topic.clone();
                    let pins_for_pin = current_pins.clone();
                    let post_id_for_pin = post.id.clone();
                    let post_id_for_mute = post.id.clone();
                    let pubkey_for_block = post.pubkey.clone();
                    rsx! {
                        div {
                            class: "fixed inset-0 z-40",
                            onclick: move |_| is_open.set(false),
                        }
                        div {
                            class: "absolute right-0 mt-1 w-48 bg-background border border-border rounded-lg shadow-lg z-50 py-1",
                            onclick: move |e| e.stop_propagation(),
                            // Copy link
                            button {
                                class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-sm",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    let value = post_url.clone();
                                    spawn(async move {
                                        let _ = crate::platform::clipboard::copy_to_clipboard(&value).await;
                                    });
                                    toast.info("Link copied to clipboard".to_string(), ToastOptions::new());
                                    is_open.set(false);
                                },
                                "Copy link"
                            }
                            // Pin/Unpin
                            if can_pin {
                                {
                                    let is_at_limit = pins_for_pin.len() >= MAX_PINS;
                                    rsx! {
                                        button {
                                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-sm disabled:opacity-50",
                                            disabled: *pin_loading.read() || (!is_pinned && is_at_limit),
                                            onclick: move |e: MouseEvent| {
                                                e.stop_propagation();
                                                let topic = topic_for_pin.clone();
                                                let post_id = post_id_for_pin.clone();
                                                let pins = pins_for_pin.clone();
                                                let at_limit = is_at_limit;
                                                pin_loading.set(true);
                                                spawn(async move {
                                                    let result = if is_pinned {
                                                        unpin_post(&topic, &post_id, &pins).await
                                                    } else if !at_limit {
                                                        pin_post(&topic, &post_id, &pins).await
                                                    } else {
                                                        Err("Max pins reached".to_string())
                                                    };
                                                    match result {
                                                        Ok(_) => {
                                                            let msg = if is_pinned { "Unpinned" } else { "Pinned" };
                                                            toast.info(msg.to_string(), ToastOptions::new());
                                                            if let Some(cb) = on_pin_toggle {
                                                                cb.call(());
                                                            }
                                                        }
                                                        Err(e) => {
                                                            toast.error(e, ToastOptions::new());
                                                        }
                                                    }
                                                    pin_loading.set(false);
                                                    is_open.set(false);
                                                });
                                            },
                                            if is_pinned { "Unpin post" } else { "Pin post" }
                                        }
                                    }
                                }
                            }
                            div { class: "border-t border-border my-1" }
                            // Mute post
                            button {
                                class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-sm",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    let id = post_id_for_mute.clone();
                                    spawn(async move {
                                        match nostr_client::mute_post(id).await {
                                            Ok(_) => toast.info("Post muted".to_string(), ToastOptions::new()),
                                            Err(e) => toast.error(e, ToastOptions::new()),
                                        }
                                    });
                                    is_open.set(false);
                                },
                                "Mute post"
                            }
                            // Block user
                            button {
                                class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-sm",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    let pk = pubkey_for_block.clone();
                                    spawn(async move {
                                        match nostr_client::block_user(pk).await {
                                            Ok(_) => toast.info("User blocked".to_string(), ToastOptions::new()),
                                            Err(e) => toast.error(e, ToastOptions::new()),
                                        }
                                    });
                                    is_open.set(false);
                                },
                                "Block user"
                            }
                            div { class: "border-t border-border my-1" }
                            // Report
                            button {
                                class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-sm text-red-500 hover:text-red-600",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_report.set(true);
                                    is_open.set(false);
                                },
                                "Report post"
                            }
                        }
                    }
                }
            }
            if *show_report.read() {
                ReportModal {
                    event_id: post.id.clone(),
                    author_pubkey: post.pubkey.clone(),
                    on_close: move |_| show_report.set(false),
                }
            }
        }
    }
}
