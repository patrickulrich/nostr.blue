//! Wiki Download Menu Component
//!
//! A dropdown menu for downloading wiki pages in different formats:
//! - Print to PDF: Opens browser print dialog for high-quality PDF export
//! - Download as Markdown: Downloads the wiki content as a .md file

use crate::components::icons::{ChevronDownIcon, DownloadIcon, FileTextIcon, PrinterIcon};
use crate::stores::wiki_store::CachedWikiPage;
#[cfg(feature = "web")]
use crate::utils::download::{download_markdown, trigger_print};
use dioxus::prelude::*;
use dioxus_primitives::toast::consume_toast;
#[allow(unused_imports)]
use dioxus_primitives::toast::ToastOptions;
#[allow(unused_imports)]
use std::time::Duration;

#[derive(Props, Clone, PartialEq)]
pub struct WikiDownloadMenuProps {
    /// The wiki page to download
    pub page: CachedWikiPage,
}

#[component]
pub fn WikiDownloadMenu(props: WikiDownloadMenuProps) -> Element {
    let mut is_open = use_signal(|| false);
    let toast = consume_toast();

    let title = props.page.article.title.clone();
    let identifier = props.page.article.identifier.clone();
    let content = props.page.article.content.clone();

    let expanded = *is_open.read();
    let chevron_class = if expanded {
        "w-3 h-3 transition-transform rotate-180"
    } else {
        "w-3 h-3 transition-transform"
    };

    rsx! {
        div { class: "relative",
            // Toggle button with download icon and chevron
            button {
                class: "flex items-center gap-1 p-2 rounded-lg hover:bg-accent transition-colors text-muted-foreground hover:text-foreground",
                title: "Download",
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    is_open.set(!expanded);
                },
                DownloadIcon { class: "w-5 h-5".to_string() }
                ChevronDownIcon { class: chevron_class.to_string() }
            }

            // Dropdown menu
            if expanded {
                // Backdrop to close on click outside
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        is_open.set(false);
                    },
                }

                // Menu dropdown
                div { class: "absolute right-0 mt-2 w-56 bg-background border border-border rounded-lg shadow-lg z-50 py-1 overflow-hidden",
                    // Print to PDF option
                    {
                        rsx! {
                            button {
                                class: "w-full text-left px-4 py-2.5 hover:bg-accent transition-colors flex items-center gap-3",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    is_open.set(false);
                                    #[cfg(feature = "web")]
                                    {
                                        trigger_print();
                                        toast.info(
                                            "Print dialog opened".to_string(),
                                            ToastOptions::new()
                                                .description("Select 'Save as PDF' to download".to_string())
                                                .duration(Duration::from_secs(3))
                                                .permanent(false),
                                        );
                                    }
                                    #[cfg(not(feature = "web"))]
                                    {
                                        toast.error(
                                            "Print not supported on this platform".to_string(),
                                            ToastOptions::new()
                                                .duration(Duration::from_secs(3))
                                                .permanent(false),
                                        );
                                    }
                                },
                                PrinterIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                                div { class: "flex-1",
                                    p { class: "text-sm font-medium text-foreground", "Print to PDF" }
                                    p { class: "text-xs text-muted-foreground", "Formatted document" }
                                }
                            }
                        }
                    }

                    // Divider
                    div { class: "h-px bg-border mx-2 my-1" }

                    // Download as Markdown option
                    {
                        #[allow(unused_variables)]
                        let title_md = title.clone();
                        #[allow(unused_variables)]
                        let identifier_md = identifier.clone();
                        #[allow(unused_variables)]
                        let content_md = content.clone();
                        #[allow(unused_variables)]
                        let toast_api = toast;
                        rsx! {
                            button {
                                class: "w-full text-left px-4 py-2.5 hover:bg-accent transition-colors flex items-center gap-3",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    is_open.set(false);

                                    // Generate filename from identifier
                                    #[allow(unused_variables)]
                                    let filename = format!("{}.md", identifier_md);
                                    #[cfg(feature = "web")]
                                    {
                                        download_markdown(&filename, &title_md, &content_md);
                                        toast_api.success(
                                            "Downloaded!".to_string(),
                                            ToastOptions::new()
                                                .description(format!("Saved as {}", filename))
                                                .duration(Duration::from_secs(2))
                                                .permanent(false),
                                        );
                                    }
                                    #[cfg(not(feature = "web"))]
                                    {
                                        toast_api.error(
                                            "Download not supported on this platform".to_string(),
                                            ToastOptions::new()
                                                .duration(Duration::from_secs(3))
                                                .permanent(false),
                                        );
                                    }
                                },
                                FileTextIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                                div { class: "flex-1",
                                    p { class: "text-sm font-medium text-foreground", "Download as Markdown" }
                                    p { class: "text-xs text-muted-foreground", "Plain text format" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
