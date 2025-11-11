use dioxus::prelude::*;
use crate::stores::nostr_client;

#[derive(Props, Clone, PartialEq)]
pub struct ReportModalProps {
    pub event_id: String,
    pub author_pubkey: String,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn ReportModal(props: ReportModalProps) -> Element {
    let mut selected_type = use_signal(|| "spam".to_string());
    let mut details = use_signal(|| String::new());
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);

    // Extract props fields before closures to avoid moving entire props struct
    let event_id = props.event_id.clone();
    let author_pubkey = props.author_pubkey.clone();
    let on_close = props.on_close.clone();

    // Report types from NIP-56
    let report_types = vec![
        ("spam", "Spam"),
        ("nudity", "Nudity / NSFW"),
        ("profanity", "Hateful Speech"),
        ("illegal", "Illegal Content"),
        ("malware", "Malware / Virus"),
        ("impersonation", "Impersonation"),
        ("other", "Other"),
    ];

    let handle_report = move |_| {
        let event_id = event_id.clone();
        let author_pubkey = author_pubkey.clone();
        let on_close = on_close.clone();
        let report_type = selected_type.read().clone();
        let report_details = details.read().clone();

        loading.set(true);
        error_msg.set(None);

        spawn(async move {
            let details_opt = if report_details.is_empty() {
                None
            } else {
                Some(report_details)
            };

            match nostr_client::report_post(event_id, author_pubkey, report_type, details_opt).await {
                Ok(_) => {
                    log::info!("Post reported successfully");
                    success.set(true);
                    loading.set(false);

                    // Auto-close after success
                    spawn(async move {
                        gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
                        on_close.call(());
                    });
                }
                Err(e) => {
                    log::error!("Failed to report post: {}", e);
                    error_msg.set(Some(format!("Failed to report: {}", e)));
                    loading.set(false);
                }
            }
        });
    };

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-background border border-border rounded-lg p-6 max-w-md mx-4 w-full",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex justify-between items-center mb-4",
                    h2 {
                        class: "text-xl font-bold",
                        "Report Post"
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                // Success message
                if *success.read() {
                    div {
                        class: "mb-4 p-3 bg-green-500/10 border border-green-500/20 rounded-lg text-green-600",
                        "✓ Report submitted successfully. The post has been hidden."
                    }
                }

                // Form
                if !*success.read() {
                    div {
                        class: "space-y-4",

                        // Report type selector
                        div {
                            label {
                                class: "block text-sm font-medium mb-2",
                                "Reason for reporting"
                            }
                            select {
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                value: "{selected_type}",
                                onchange: move |e| selected_type.set(e.value().clone()),

                                for (value, label) in report_types.iter() {
                                    option {
                                        value: "{value}",
                                        "{label}"
                                    }
                                }
                            }
                        }

                        // Additional details
                        div {
                            label {
                                class: "block text-sm font-medium mb-2",
                                "Additional details (optional)"
                            }
                            textarea {
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary resize-none",
                                rows: 3,
                                placeholder: "Provide additional context about why you're reporting this post...",
                                value: "{details}",
                                oninput: move |e| details.set(e.value().clone()),
                            }
                        }

                        // Error message
                        if let Some(err) = error_msg.read().as_ref() {
                            div {
                                class: "text-red-500 text-sm",
                                "{err}"
                            }
                        }

                        // Action buttons
                        div {
                            class: "flex gap-2 justify-end pt-2",
                            button {
                                class: "px-4 py-2 text-sm text-muted-foreground hover:text-foreground",
                                disabled: *loading.read(),
                                onclick: move |_| on_close.call(()),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 text-sm bg-red-500 hover:bg-red-600 text-white rounded-lg disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *loading.read(),
                                onclick: handle_report,
                                if *loading.read() {
                                    "Reporting..."
                                } else {
                                    "Submit Report"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
