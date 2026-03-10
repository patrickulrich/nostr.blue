//! QR Share Modal Component
//!
//! Displays a QR code for sharing repository via Nostr or web URL.
use crate::utils::clipboard::copy_to_clipboard;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use qrcode::render::svg;
use qrcode::QrCode;

/// QR Share modal with Nostr and web URL tabs
#[component]
pub fn QrShareModal(naddr: String, repo_name: String, on_close: EventHandler<()>) -> Element {
    let toast = consume_toast();
    let mut active_tab = use_signal(|| "nostr");

    let nostr_url = format!("nostr:{}", naddr);
    let base = {
        #[cfg(feature = "web")]
        {
            web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_else(|| "https://nostr.blue".to_string())
        }
        #[cfg(not(feature = "web"))]
        {
            if cfg!(debug_assertions) {
                "http://localhost:8080".to_string()
            } else {
                "https://nostr.blue".to_string()
            }
        }
    };
    let web_url = format!("{}/code/repo/{}", base, naddr);

    let current_url = if *active_tab.read() == "nostr" {
        nostr_url.clone()
    } else {
        web_url.clone()
    };

    let qr_svg = QrCode::new(&current_url).ok().map(|code| {
        code.render::<svg::Color>()
            .min_dimensions(200, 200)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build()
    });

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),
        }
        // Modal
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-background border border-border rounded-lg p-6 w-full max-w-sm shadow-lg",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "qr-share-title",
                onclick: move |evt| evt.stop_propagation(),
                // Header
                div { class: "flex justify-between items-center mb-4",
                    h2 { id: "qr-share-title", class: "text-lg font-bold", "Share {repo_name}" }
                    button {
                        class: "p-1 hover:bg-accent rounded transition text-muted-foreground hover:text-foreground",
                        aria_label: "Close share modal",
                        r#type: "button",
                        onclick: move |_| on_close.call(()),
                        "\u{2715}"
                    }
                }
                // Tabs
                div { class: "flex gap-1 mb-4 border-b border-border",
                    {
                        let is_nostr = *active_tab.read() == "nostr";
                        let nostr_class = if is_nostr {
                            "px-3 py-2 text-sm font-medium text-primary border-b-2 border-primary -mb-px"
                        } else {
                            "px-3 py-2 text-sm text-muted-foreground hover:text-foreground transition -mb-px"
                        };
                        let web_class = if !is_nostr {
                            "px-3 py-2 text-sm font-medium text-primary border-b-2 border-primary -mb-px"
                        } else {
                            "px-3 py-2 text-sm text-muted-foreground hover:text-foreground transition -mb-px"
                        };
                        rsx! {
                            button {
                                class: nostr_class,
                                r#type: "button",
                                onclick: move |_| active_tab.set("nostr"),
                                "Nostr"
                            }
                            button {
                                class: web_class,
                                r#type: "button",
                                onclick: move |_| active_tab.set("web"),
                                "Web URL"
                            }
                        }
                    }
                }
                // QR Code
                div { class: "flex justify-center mb-4",
                    if let Some(ref svg_str) = qr_svg {
                        div {
                            class: "p-4 bg-black rounded-lg",
                            dangerous_inner_html: "{svg_str}",
                        }
                    } else {
                        div { class: "w-[200px] h-[200px] bg-muted rounded-lg flex items-center justify-center",
                            p { class: "text-sm text-muted-foreground", "QR generation failed" }
                        }
                    }
                }
                // URL display + copy
                div { class: "space-y-2",
                    div { class: "flex items-center gap-2",
                        code { class: "flex-1 text-xs font-mono bg-muted px-3 py-2 rounded-lg overflow-x-auto",
                            "{current_url}"
                        }
                        button {
                            class: "shrink-0 p-2 hover:bg-accent rounded-lg transition text-muted-foreground hover:text-foreground",
                            title: "Copy to clipboard",
                            aria_label: "Copy URL to clipboard",
                            r#type: "button",
                            onclick: {
                                let url_to_copy = current_url.clone();
                                move |_| {
                                    let url_clone = url_to_copy.clone();
                                    let toast = toast;
                                    spawn(async move {
                                        if copy_to_clipboard(&url_clone).await.is_ok() {
                                            toast.success("Copied to clipboard".to_string(), ToastOptions::new());
                                        } else {
                                            toast.error("Failed to copy".to_string(), ToastOptions::new());
                                        }
                                    });
                                }
                            },
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
                                rect { x: "9", y: "9", width: "13", height: "13", rx: "2" }
                                path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                            }
                        }
                    }
                }
            }
        }
    }
}
