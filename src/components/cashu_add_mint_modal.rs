use dioxus::prelude::*;
use crate::stores::cashu;
use crate::stores::cashu::MintInfoDisplay;

#[component]
pub fn CashuAddMintModal(
    on_close: EventHandler<()>,
    on_mint_added: EventHandler<String>,
) -> Element {
    let mut mint_url = use_signal(|| String::new());
    let mut is_checking = use_signal(|| false);
    let mut is_adding = use_signal(|| false);
    let mut mint_info = use_signal(|| Option::<MintInfoDisplay>::None);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut is_confirmed = use_signal(|| false);

    let on_check_mint = move |_| {
        let url = mint_url.read().clone().trim().to_string();

        if url.is_empty() {
            error_message.set(Some("Please enter a mint URL".to_string()));
            return;
        }

        // Basic URL validation
        if !url.starts_with("https://") && !url.starts_with("http://") {
            error_message.set(Some("URL must start with http:// or https://".to_string()));
            return;
        }

        is_checking.set(true);
        error_message.set(None);
        mint_info.set(None);

        spawn(async move {
            match cashu::get_mint_info(&url).await {
                Ok(info) => {
                    mint_info.set(Some(info));
                    is_checking.set(false);
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to connect: {}", e)));
                    is_checking.set(false);
                }
            }
        });
    };

    let on_add_mint = move |_| {
        let url = mint_url.read().clone().trim().to_string();

        is_adding.set(true);
        error_message.set(None);

        spawn(async move {
            match cashu::add_mint(&url).await {
                Ok(_) => {
                    is_confirmed.set(true);
                    is_adding.set(false);
                    on_mint_added.call(url);
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to add mint: {}", e)));
                    is_adding.set(false);
                }
            }
        });
    };

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| {
                if !*is_checking.read() && !*is_adding.read() {
                    on_close.call(());
                }
            },

            // Modal content
            div {
                class: "bg-card border border-border rounded-lg max-w-md w-full shadow-xl",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    h3 {
                        class: "text-xl font-bold flex items-center gap-2",
                        span { "+" }
                        "Add Mint"
                    }
                    if !*is_checking.read() && !*is_adding.read() {
                        button {
                            class: "text-2xl text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| on_close.call(()),
                            "×"
                        }
                    }
                }

                // Body
                div {
                    class: "p-6 space-y-4",

                    // Success message
                    if *is_confirmed.read() {
                        div {
                            class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                svg {
                                    class: "w-6 h-6 text-green-600 flex-shrink-0",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M5 13l4 4L19 7"
                                    }
                                }
                                div {
                                    p {
                                        class: "text-sm font-semibold text-green-800 dark:text-green-200",
                                        "Mint added successfully!"
                                    }
                                    p {
                                        class: "text-xs text-green-700 dark:text-green-300 mt-1",
                                        "You can now use this mint to receive and send ecash."
                                    }
                                }
                            }
                        }
                    }

                    // Error message
                    if let Some(msg) = error_message.read().as_ref() {
                        div {
                            class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                svg {
                                    class: "w-6 h-6 text-red-600 flex-shrink-0",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M6 18L18 6M6 6l12 12"
                                    }
                                }
                                p {
                                    class: "text-sm text-red-800 dark:text-red-200",
                                    "{msg}"
                                }
                            }
                        }
                    }

                    // URL input (only show if not confirmed)
                    if !*is_confirmed.read() {
                        div {
                            label {
                                class: "block text-sm font-semibold mb-2",
                                "Mint URL"
                            }
                            input {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg font-mono text-sm",
                                r#type: "url",
                                placeholder: "https://mint.example.com",
                                value: mint_url.read().clone(),
                                disabled: *is_checking.read() || *is_adding.read() || mint_info.read().is_some(),
                                oninput: move |evt| mint_url.set(evt.value())
                            }
                        }
                    }

                    // Mint info display
                    if let Some(info) = mint_info.read().as_ref() {
                        div {
                            class: "bg-accent/50 rounded-lg p-4 space-y-3",
                            h4 {
                                class: "text-sm font-semibold mb-2",
                                "Mint Information"
                            }

                            // Name
                            if let Some(name) = &info.name {
                                div {
                                    class: "flex justify-between items-center",
                                    span { class: "text-sm text-muted-foreground", "Name" }
                                    span { class: "text-sm font-medium", "{name}" }
                                }
                            }

                            // Description
                            if let Some(desc) = &info.description {
                                div {
                                    class: "text-sm text-muted-foreground",
                                    "{desc}"
                                }
                            }

                            // Supported NUTs
                            if !info.supported_nuts.is_empty() {
                                div {
                                    span { class: "text-sm text-muted-foreground", "Supported NUTs: " }
                                    span { class: "text-sm font-mono",
                                        {info.supported_nuts.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")}
                                    }
                                }
                            }

                            // Check marks for required features
                            div {
                                class: "flex gap-4 mt-2",
                                div {
                                    class: if info.supported_nuts.contains(&4) { "text-green-500 text-sm" } else { "text-red-500 text-sm" },
                                    if info.supported_nuts.contains(&4) { "✓ NUT-4 (Mint)" } else { "✗ NUT-4 (Mint)" }
                                }
                                div {
                                    class: if info.supported_nuts.contains(&5) { "text-green-500 text-sm" } else { "text-red-500 text-sm" },
                                    if info.supported_nuts.contains(&5) { "✓ NUT-5 (Melt)" } else { "✗ NUT-5 (Melt)" }
                                }
                            }

                            // MOTD
                            if let Some(motd) = &info.motd {
                                div {
                                    class: "mt-2 p-2 bg-background/50 rounded text-sm italic",
                                    "{motd}"
                                }
                            }

                            // Contact
                            if !info.contact.is_empty() {
                                div {
                                    class: "mt-2 pt-2 border-t border-border",
                                    span { class: "text-xs text-muted-foreground", "Contact: " }
                                    for (method, contact_info) in info.contact.iter() {
                                        span { class: "text-xs", "{method}: {contact_info} " }
                                    }
                                }
                            }
                        }
                    }

                    // Loading indicator
                    if *is_checking.read() {
                        div {
                            class: "flex items-center justify-center py-4",
                            span { class: "text-sm text-muted-foreground", "Checking mint..." }
                        }
                    }
                }

                // Footer
                div {
                    class: "px-6 py-4 border-t border-border flex gap-3",

                    if *is_confirmed.read() {
                        // Done button after success
                        button {
                            class: "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition",
                            onclick: move |_| on_close.call(()),
                            "Done"
                        }
                    } else {
                        // Cancel button
                        button {
                            class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                            disabled: *is_checking.read() || *is_adding.read(),
                            onclick: move |_| {
                                if mint_info.read().is_some() {
                                    // Go back to URL input
                                    mint_info.set(None);
                                } else {
                                    on_close.call(());
                                }
                            },
                            if mint_info.read().is_some() { "Back" } else { "Cancel" }
                        }

                        // Action button
                        if mint_info.read().is_some() {
                            // Add mint button
                            button {
                                class: if *is_adding.read() {
                                    "flex-1 px-4 py-3 bg-green-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                                } else {
                                    "flex-1 px-4 py-3 bg-green-500 hover:bg-green-600 text-white font-semibold rounded-lg transition"
                                },
                                disabled: *is_adding.read(),
                                onclick: on_add_mint,
                                if *is_adding.read() { "Adding..." } else { "Add Mint" }
                            }
                        } else {
                            // Check mint button
                            button {
                                class: if *is_checking.read() || mint_url.read().is_empty() {
                                    "flex-1 px-4 py-3 bg-blue-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                                } else {
                                    "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition"
                                },
                                disabled: *is_checking.read() || mint_url.read().is_empty(),
                                onclick: on_check_mint,
                                if *is_checking.read() { "Checking..." } else { "Check Mint" }
                            }
                        }
                    }
                }
            }
        }
    }
}
