//! Article Cover Image Uploader Component
//!
//! A specialized component for uploading and managing article cover images.
//! Shows a URL field with an upload button that fills in the URL when uploaded.
use dioxus::prelude::*;
use super::media_uploader::MediaUploader;
use super::modal::{Modal, ModalBody, ModalFooter, ModalHeader};
#[derive(Props, Clone, PartialEq)]
pub struct ArticleCoverUploaderProps {
    /// Signal holding the current cover image URL
    pub cover_url: Signal<String>,
    /// Optional label for the upload button
    #[props(default = "Upload Cover Image".to_string())]
    pub label: String,
    /// Whether to show the manual URL input option
    #[props(default = true)]
    pub show_url_input: bool,
}
#[component]
pub fn ArticleCoverUploader(props: ArticleCoverUploaderProps) -> Element {
    let mut show_upload_modal = use_signal(|| false);
    let mut failed_url: Signal<Option<String>> = use_signal(|| None);
    let input_id = use_signal(|| format!("cover-upload-{}", uuid::Uuid::new_v4()));
    let cover_input_id = use_signal(|| format!("cover-url-{}", uuid::Uuid::new_v4()));
    let has_cover = !props.cover_url.read().is_empty();
    let handle_upload = {
        let mut cover_url = props.cover_url;
        move |url: String| {
            cover_url.set(url);
            show_upload_modal.set(false);
            failed_url.set(None);
        }
    };
    let handle_url_change = {
        let mut cover_url = props.cover_url;
        move |e: Event<FormData>| {
            cover_url.set(e.value());
            failed_url.set(None);
        }
    };
    let handle_remove = {
        let mut cover_url = props.cover_url;
        move |_| {
            cover_url.set(String::new());
            failed_url.set(None);
        }
    };
    rsx! {
        div { class: "space-y-3",
            if props.show_url_input {
                label {
                    class: "block text-sm font-medium mb-2",
                    r#for: "{cover_input_id}",
                    "Cover Image"
                    span { class: "text-muted-foreground font-normal ml-1", "(optional)" }
                }
            } else {
                span { class: "block text-sm font-medium mb-2",
                    "Cover Image"
                    span { class: "text-muted-foreground font-normal ml-1", "(optional)" }
                }
            }
            div { class: "flex gap-2",
                if props.show_url_input {
                    input {
                        id: "{cover_input_id}",
                        class: "flex-1 px-3 py-2 border border-border rounded-lg bg-background text-foreground placeholder:text-muted-foreground focus:outline-hidden focus:ring-2 focus:ring-primary text-sm",
                        r#type: "url",
                        placeholder: "https://example.com/image.jpg",
                        value: "{props.cover_url}",
                        oninput: handle_url_change,
                    }
                }
                button {
                    r#type: "button",
                    class: "px-4 py-2 bg-accent hover:bg-accent/80 text-foreground rounded-lg font-medium transition flex items-center gap-2 text-sm",
                    onclick: move |_| show_upload_modal.set(true),
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12",
                        }
                    }
                    "Upload"
                }
            }
            if has_cover {
                div { class: "relative rounded-lg overflow-hidden border border-border",
                    {
                        let current_url = props.cover_url.read().clone();
                        let failed = failed_url.read().clone();
                        let is_current_url_failed = failed.as_ref() == Some(&current_url);
                        if !is_current_url_failed {
                            rsx! {
                                img {
                                    src: "{current_url}",
                                    class: "w-full h-48 object-cover",
                                    alt: "Cover image preview",
                                    onerror: {
                                        let url = current_url.clone();
                                        move |_| failed_url.set(Some(url.clone()))
                                    },
                                }
                            }
                        } else {
                            rsx! {
                                div { class: "w-full h-48 flex flex-col items-center justify-center bg-muted text-muted-foreground",
                                    span { class: "text-4xl mb-2", "🖼️" }
                                    p { class: "text-sm", "Image failed to load" }
                                    p { class: "text-xs text-muted-foreground truncate max-w-full px-4", "{current_url}" }
                                }
                            }
                        }
                    }
                    div { class: "absolute top-2 right-2",
                        button {
                            r#type: "button",
                            class: "p-2 bg-red-600/80 hover:bg-red-600 text-white rounded-full transition",
                            title: "Remove cover image",
                            aria_label: "Remove cover image",
                            onclick: handle_remove,
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M6 18L18 6M6 6l12 12",
                                }
                            }
                        }
                    }
                }
            }
            Modal { open: show_upload_modal,
                div { class: "w-full max-w-md",
                    ModalHeader {
                        title: "Upload Cover Image".to_string(),
                        on_close: move |_| show_upload_modal.set(false),
                    }
                    ModalBody {
                        div { class: "space-y-4",
                            p { class: "text-sm text-muted-foreground",
                                "Upload an image to use as your article cover. The URL will be filled in automatically."
                            }
                            MediaUploader {
                                on_upload: handle_upload,
                                button_label: props.label.clone(),
                                input_id: input_id.read().clone(),
                                show_server_selector: true,
                            }
                        }
                    }
                    ModalFooter {
                        button {
                            r#type: "button",
                            class: "px-4 py-2 text-sm font-medium rounded-lg hover:bg-accent text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| show_upload_modal.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}
