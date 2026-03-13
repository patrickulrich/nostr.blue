//! Image Upload Dialog Component
//!
//! A dialog for uploading images with alt text and title fields
//! for better accessibility and SEO.
use super::media_uploader::MediaUploader;
use super::modal::{Modal, ModalBody, ModalFooter, ModalHeader};
use crate::stores::blossom_store;
use dioxus::prelude::*;
/// Data returned when inserting an image
#[derive(Clone, Debug, Default)]
pub struct ImageInsertData {
    /// The URL of the uploaded image
    pub url: String,
    /// Alt text for accessibility (shown if image fails to load)
    pub alt: String,
    /// Optional title (shown as tooltip on hover)
    pub title: String,
}
impl ImageInsertData {
    /// Format as markdown image syntax
    pub fn to_markdown(&self) -> String {
        let alt = &self.alt;
        let url = &self.url;
        let title = &self.title;
        if !title.is_empty() {
            format!("![{alt}]({url} \"{title}\")")
        } else {
            format!("![{alt}]({url})")
        }
    }
}
#[derive(Props, Clone, PartialEq)]
pub struct ImageUploadDialogProps {
    /// Whether the dialog is open
    pub open: Signal<bool>,
    /// Callback when user inserts the image
    pub on_insert: EventHandler<ImageInsertData>,
}
#[component]
pub fn ImageUploadDialog(props: ImageUploadDialogProps) -> Element {
    let mut open = props.open;
    let mut uploaded_url = use_signal(String::new);
    let mut alt_text = use_signal(String::new);
    let mut title_text = use_signal(String::new);
    let mut image_error = use_signal(|| false);
    let upload_progress = blossom_store::UPLOAD_PROGRESS.read();
    let is_uploading = upload_progress.is_some();
    use_effect(move || {
        if *open.read() {
            uploaded_url.set(String::new());
            alt_text.set(String::new());
            title_text.set(String::new());
            image_error.set(false);
        }
    });
    let handle_upload = move |url: String| {
        uploaded_url.set(url);
        image_error.set(false);
    };
    let handle_insert = {
        let on_insert = props.on_insert;
        move |_| {
            let data = ImageInsertData {
                url: uploaded_url.read().clone(),
                alt: alt_text.read().clone(),
                title: title_text.read().clone(),
            };
            on_insert.call(data);
            open.set(false);
        }
    };
    let can_insert = !uploaded_url.read().is_empty();
    let has_image = !uploaded_url.read().is_empty();
    rsx! {
        Modal { open,
            div { class: "w-full max-w-lg",
                ModalHeader {
                    title: "Insert Image".to_string(),
                    on_close: move |_| open.set(false),
                }
                ModalBody {
                    div { class: "space-y-4",
                        div { class: "border-2 border-dashed border-border rounded-lg overflow-hidden bg-muted/30",
                            if has_image && !*image_error.read() {
                                div { class: "relative group",
                                    img {
                                        src: "{uploaded_url}",
                                        class: "w-full h-48 object-contain bg-background",
                                        alt: "Preview",
                                        onerror: move |_| image_error.set(true),
                                    }
                                    div { class: "absolute inset-0 bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center gap-2",
                                        button {
                                            class: "px-3 py-1.5 text-sm font-medium rounded bg-white text-black hover:bg-gray-200 transition",
                                            onclick: move |_| {
                                                uploaded_url.set(String::new());
                                                image_error.set(false);
                                            },
                                            "Change Image"
                                        }
                                    }
                                }
                            } else if *image_error.read() {
                                div { class: "p-8 text-center",
                                    div { class: "text-4xl mb-2", "⚠️" }
                                    p { class: "text-sm text-muted-foreground mb-2",
                                        "Failed to load image preview"
                                    }
                                    p { class: "text-xs text-muted-foreground font-mono break-all",
                                        "{uploaded_url}"
                                    }
                                    button {
                                        class: "mt-3 px-3 py-1.5 text-sm font-medium rounded bg-accent hover:bg-accent/80 transition",
                                        onclick: move |_| {
                                            uploaded_url.set(String::new());
                                            image_error.set(false);
                                        },
                                        "Try Different Image"
                                    }
                                }
                            } else {
                                div { class: "p-6",
                                    MediaUploader {
                                        on_upload: handle_upload,
                                        button_label: "Select Image".to_string(),
                                        input_id: "image-dialog-upload".to_string(),
                                        show_server_selector: true,
                                    }
                                }
                            }
                        }
                        div { class: "space-y-1.5",
                            label {
                                class: "block text-sm font-medium",
                                r#for: "image-alt-text",
                                "Alt Text"
                                span { class: "text-muted-foreground font-normal ml-1",
                                    "(accessibility)"
                                }
                            }
                            input {
                                id: "image-alt-text",
                                class: "w-full px-3 py-2 rounded-lg border border-border bg-background text-foreground placeholder:text-muted-foreground focus:outline-hidden focus:ring-2 focus:ring-primary/50 focus:border-primary transition",
                                r#type: "text",
                                placeholder: "Describe what's in the image...",
                                value: "{alt_text}",
                                disabled: !has_image,
                                oninput: move |e| alt_text.set(e.value()),
                            }
                            p { class: "text-xs text-muted-foreground",
                                "Shown if the image fails to load. Helps screen readers."
                            }
                        }
                        div { class: "space-y-1.5",
                            label {
                                class: "block text-sm font-medium",
                                r#for: "image-title",
                                "Title"
                                span { class: "text-muted-foreground font-normal ml-1",
                                    "(optional)"
                                }
                            }
                            input {
                                id: "image-title",
                                class: "w-full px-3 py-2 rounded-lg border border-border bg-background text-foreground placeholder:text-muted-foreground focus:outline-hidden focus:ring-2 focus:ring-primary/50 focus:border-primary transition",
                                r#type: "text",
                                placeholder: "Additional context shown on hover...",
                                value: "{title_text}",
                                disabled: !has_image,
                                oninput: move |e| title_text.set(e.value()),
                            }
                            p { class: "text-xs text-muted-foreground",
                                "Appears as a tooltip when hovering over the image."
                            }
                        }
                        if has_image {
                            div { class: "p-3 rounded-lg bg-muted/50 border border-border",
                                p { class: "text-xs font-medium text-muted-foreground mb-1",
                                    "Markdown Preview"
                                }
                                code { class: "text-xs font-mono break-all text-foreground",
                                    {
                                        ImageInsertData {
                                            url: uploaded_url.read().clone(),
                                            alt: alt_text.read().clone(),
                                            title: title_text.read().clone(),
                                        }
                                            .to_markdown()
                                    }
                                }
                            }
                        }
                    }
                }
                ModalFooter {
                    button {
                        class: "px-4 py-2 text-sm font-medium rounded-lg hover:bg-accent text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| open.set(false),
                        disabled: is_uploading,
                        "Cancel"
                    }
                    button {
                        class: "px-6 py-2 text-sm font-medium rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                        onclick: handle_insert,
                        disabled: !can_insert || is_uploading,
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z",
                            }
                        }
                        "Insert Image"
                    }
                }
            }
        }
    }
}
