use crate::stores::social::group_store::Group;
use dioxus::prelude::*;
use nostr_sdk::nips::nip19::Nip19Coordinate;
use nostr_sdk::prelude::*;

#[cfg(feature = "web")]
#[allow(unused_imports)]
use wasm_bindgen::JsCast;

pub fn encode_group_share(group: &Group) -> Option<String> {
    let relay_pk = group.event.as_ref().map(|e| e.pubkey)?;
    let coord = Coordinate::new(Kind::Custom(39000), relay_pk).identifier(&group.id);
    let relay_url = RelayUrl::parse(&group.relay_url).ok()?;
    let nip19 = Nip19Coordinate::new(coord, vec![relay_url]);
    let naddr = nip19.to_bech32().ok()?;
    Some(format!("nostr:{}", naddr))
}

#[component]
pub fn GroupShareModal(
    group: Group,
    on_close: EventHandler<()>,
) -> Element {
    let share_uri = encode_group_share(&group).unwrap_or_default();
    let mut copied = use_signal(|| false);

    let group_name = group.name.clone().unwrap_or_else(|| group.id.clone());

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card border border-border rounded-lg p-6 max-w-sm w-full mx-4 space-y-4",
                onclick: move |e| e.stop_propagation(),

                h3 { class: "text-lg font-semibold text-foreground", "Share Group" }

                div { class: "flex items-center gap-3 p-3 bg-muted rounded-lg",
                    if let Some(url) = &group.picture {
                        img { class: "w-12 h-12 rounded-full object-cover", src: "{url}", loading: "lazy" }
                    } else {
                        div { class: "w-12 h-12 rounded-full bg-muted flex items-center justify-center text-sm", "{group_name}" }
                    }
                    div {
                        div { class: "font-medium text-foreground", "{group_name}" }
                        div { class: "text-xs text-muted-foreground", "{group.relay_url}" }
                    }
                }

                div { class: "space-y-2",
                    label { class: "text-sm font-medium text-foreground", "Share Link" }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-sm text-muted-foreground truncate",
                            readonly: true,
                            value: "{share_uri}",
                        }
                        button {
                            class: "px-3 py-2 bg-primary text-primary-foreground rounded-lg text-sm shrink-0",
                            onclick: {
                                let uri = share_uri.clone();
                                move |_| {
                                    let uri = uri.clone();
                                    spawn(async move {
                                        #[cfg(all(feature = "web", target_arch = "wasm32"))]
                                        {
                                            let window = web_sys::window().unwrap();
                                            let navigator = window.navigator();
                                            let clipboard = navigator.clipboard();
                                            let _ = clipboard.write_text(&uri);
                                        }
                                        #[cfg(not(all(feature = "web", target_arch = "wasm32")))]
                                        {
                                            let _ = uri;
                                        }
                                    });
                                    copied.set(true);
                                }
                            },
                            if *copied.read() { "Copied!" } else { "Copy" }
                        }
                    }
                }

                button {
                    class: "w-full px-4 py-2 bg-accent text-foreground rounded-lg text-sm hover:bg-accent/80 transition",
                    onclick: move |_| on_close.call(()),
                    "Close"
                }
            }
        }
    }
}
