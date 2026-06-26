use crate::stores::mostro::encrypted_attachment::AttachmentMeta;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[cfg(feature = "web")]
use web_sys::HtmlInputElement;

const ATTACHMENT_ACCEPT: &str = "image/*,video/*,audio/*,.pdf,.doc,.docx,.txt,.zip";
const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatMsg {
    pub content: String,
    pub sender_hex: String,
    pub is_me: bool,
    pub timestamp: i64,
    #[serde(default)]
    pub attachments: Vec<AttachmentMeta>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChatEnvelope {
    text: String,
    #[serde(default)]
    attachments: Vec<AttachmentMeta>,
}

/// Phase 5.2: encode chat content for sending.
///
/// - If no attachments: send plain text (both nostr.blue and reference
///   clients handle this).
/// - If exactly one attachment: send the `AttachmentMeta` JSON directly
///   as the message content (spec format, matches mostro-cli/mostrix/mobile).
/// - If text + attachments: use the legacy `ChatEnvelope` wrapper (nostr.blue
///   format — other clients will see the JSON as text, but it's valid JSON
///   and not garbage). This case is rare (most messages have either text OR
///   attachment, not both).
#[allow(dead_code)]
pub fn encode_chat_content(text: &str, attachments: Vec<AttachmentMeta>) -> String {
    if attachments.is_empty() {
        return text.to_string();
    }
    if text.is_empty() && attachments.len() == 1 {
        // Spec format: single attachment JSON as the content.
        return serde_json::to_string(&attachments[0])
            .unwrap_or_else(|_| text.to_string());
    }
    // Legacy nostr.blue envelope format.
    serde_json::to_string(&ChatEnvelope {
        text: text.to_string(),
        attachments,
    })
    .unwrap_or_else(|_| text.to_string())
}

/// Phase 5.2: decode incoming chat content.
///
/// Tries three formats in order:
/// 1. **Spec single-attachment**: content parses as a single `AttachmentMeta`
///    JSON (from mostro-cli/mostrix/mobile). Returns auto-generated text +
///    the attachment.
/// 2. **Legacy nostr.blue envelope**: content parses as `ChatEnvelope { text,
///    attachments }`. Returns the text + attachments.
/// 3. **Plain text**: no attachments.
pub fn decode_chat_content(raw: &str) -> (String, Vec<AttachmentMeta>) {
    // Try spec single-attachment format first.
    if let Ok(meta) = serde_json::from_str::<AttachmentMeta>(raw) {
        let label = match &meta.filename {
            Some(name) => format!("📎 {name}"),
            None => format!("📎 {}", meta.file_type.as_deref().unwrap_or("file")),
        };
        return (label, vec![meta]);
    }

    // Try legacy nostr.blue envelope format.
    if let Ok(envelope) = serde_json::from_str::<ChatEnvelope>(raw) {
        return (envelope.text, envelope.attachments);
    }

    // Plain text.
    (raw.to_string(), Vec::new())
}

pub fn chat_storage_key(order_id: &str) -> String {
    format!("mostro/chat/{order_id}")
}

pub fn load_chat_messages(order_id: &str) -> Vec<ChatMsg> {
    let key = chat_storage_key(order_id);
    crate::platform::storage::get::<String>(&key)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub fn save_chat_messages(order_id: &str, messages: &[ChatMsg]) {
    let key = chat_storage_key(order_id);
    if let Ok(json) = serde_json::to_string(messages) {
        let _ = crate::platform::storage::set(&key, &json);
    }
}

pub fn is_dup_chat_msg(messages: &[ChatMsg], msg: &ChatMsg) -> bool {
    messages.iter().any(|m| {
        m.sender_hex == msg.sender_hex && m.timestamp == msg.timestamp && m.content == msg.content
    })
}

const SENDER_COLORS: &[&str] = &[
    "text-red-500", "text-orange-500", "text-amber-500",
    "text-lime-500", "text-green-500", "text-teal-500",
    "text-cyan-500", "text-blue-500", "text-indigo-500",
    "text-violet-500", "text-purple-500", "text-pink-500",
];

fn sender_color(pubkey: &str) -> &'static str {
    let hash = pubkey.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    SENDER_COLORS[(hash as usize) % SENDER_COLORS.len()]
}

#[derive(Props, Clone, PartialEq)]
pub struct TradeChatProps {
    pub messages: Vec<ChatMsg>,
    pub locked: bool,
    pub my_pubkey_hex: String,
    pub on_send: EventHandler<String>,
    pub on_upload_file: EventHandler<(String, Vec<u8>, String)>,
    /// Phase 5.4 (M16): hex-encoded 32-byte ECDH shared key used for
    /// decrypt-on-download of attachments. When None, the "Download"
    /// link falls back to opening the raw (still-encrypted) blob.
    #[props(default)]
    pub shared_key_hex: Option<String>,
}

#[allow(dead_code)]
#[allow(unused_mut)]
fn pick_file_non_web(
    on_upload: EventHandler<(String, Vec<u8>, String)>,
    mut uploading: Signal<bool>,
    mut upload_error: Signal<Option<String>>,
) {
    #[cfg(feature = "mobile_platform")]
    {
        spawn(async move {
            upload_error.set(None);
            match crate::platform::mobile::pick_file().await {
                Ok((bytes, mime)) => {
                    let ext = mime.split('/').nth(1).unwrap_or("bin");
                    let name = format!("attachment.{}", ext);
                    if bytes.len() > MAX_ATTACHMENT_BYTES {
                        upload_error.set(Some("File too large (max 25 MB)".to_string()));
                        return;
                    }
                    uploading.set(true);
                    on_upload.call((name, bytes, mime));
                    uploading.set(false);
                }
                Err(e) => {
                    upload_error.set(Some(e));
                }
            }
        });
    }
    #[cfg(all(feature = "native", not(feature = "mobile_platform")))]
    {
        spawn(async move {
            upload_error.set(None);
            let handle = match rfd::FileDialog::new()
                .set_title("Select file to attach")
                .pick_file()
            {
                Some(h) => h,
                None => return,
            };
            let name = handle
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("attachment")
                .to_string();
            let path = handle.as_path().to_path_buf();
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    upload_error.set(Some(format!("Read failed: {e}")));
                    return;
                }
            };
            if bytes.len() > MAX_ATTACHMENT_BYTES {
                upload_error.set(Some("File too large (max 25 MB)".to_string()));
                return;
            }
            let mime = chat_mime_type_from_filename(&name);
            uploading.set(true);
            on_upload.call((name, bytes, mime));
            uploading.set(false);
        });
    }
    #[cfg(not(any(feature = "mobile_platform", all(feature = "native", not(feature = "mobile_platform")))))]
    {
        let _ = (on_upload, uploading, upload_error);
    }
}

#[component]
pub fn TradeChat(props: TradeChatProps) -> Element {
    let input = use_signal(String::new);
    let uploading = use_signal(|| false);
    let upload_error = use_signal(|| Option::<String>::None);
    // Phase 5.4 (M16): tracks in-flight attachment downloads for UI.
    // Owned by the DownloadButton subcomponent, not this component.
    let file_input_id = format!("chat-file-{}", props.my_pubkey_hex.get(..8).unwrap_or("0"));

    rsx! {
        div { class: "p-4 bg-card border border-border rounded-lg",
            h3 { class: "text-sm font-semibold mb-3", "Chat" }

            if props.locked {
                p { class: "text-xs text-muted-foreground mb-3",
                    "Chat will be available once the trade is active and counterparty is revealed."
                }
            }

            div { class: "max-h-64 overflow-y-auto space-y-2 mb-3",
                for msg in &props.messages {
                    {
                        let color = if msg.is_me {
                            "text-primary"
                        } else {
                            sender_color(&msg.sender_hex)
                        };
                        let short = if msg.sender_hex.len() >= 8 {
                            &msg.sender_hex[..8]
                        } else {
                            &msg.sender_hex
                        };
                        rsx! {
                            div { class: "text-sm",
                                span { class: "text-xs font-medium {color}", "{short}" }
                                span { class: "text-xs text-muted-foreground ml-2",
                                    {crate::utils::time::format_time_ago(msg.timestamp as u64)}
                                }
                                if !msg.content.is_empty() {
                                    p { class: "text-foreground mt-0.5", "{msg.content}" }
                                }
                                for att in &msg.attachments {
                                    div { class: "mt-1 p-2 bg-accent/50 rounded text-xs",
                                        p { class: "text-muted-foreground",
                                            "{att.mime_type} ({crate::utils::format::format_bytes(att.original_size as usize)})"
                                        }
                                        // Phase 5.4 (M16): decrypt-on-download.
                                        // When the shared key is available,
                                        // fetch + decrypt + save locally.
                                        // When not available, fall back to
                                        // opening the raw blob.
                                        if let Some(ref key_hex) = props.shared_key_hex {
                                            super::attachment_download::AttachmentDownloadButton {
                                                att: att.clone(),
                                                shared_key_hex: key_hex.clone(),
                                            }
                                        } else {
                                            a {
                                                class: "text-primary hover:underline",
                                                href: "{att.blossom_url}",
                                                target: "_blank",
                                                "Download (encrypted)"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if props.messages.is_empty() {
                    p { class: "text-xs text-muted-foreground text-center py-4", "No messages yet" }
                }
            }

            if let Some(ref err) = *upload_error.read() {
                p { class: "text-xs text-red-500 mb-2", "{err}" }
            }

            ChatFileInput {
                file_input_id: file_input_id.clone(),
                uploading: uploading,
                upload_error: upload_error,
                locked: props.locked,
                on_upload_file: props.on_upload_file,
                input: input,
                on_send: props.on_send,
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ChatFileInputProps {
    file_input_id: String,
    uploading: Signal<bool>,
    upload_error: Signal<Option<String>>,
    locked: bool,
    on_upload_file: EventHandler<(String, Vec<u8>, String)>,
    input: Signal<String>,
    on_send: EventHandler<String>,
}

#[component]
fn ChatFileInput(props: ChatFileInputProps) -> Element {
    let file_input_id = props.file_input_id.clone();
    let accept = ATTACHMENT_ACCEPT.to_string();

    rsx! {
        div { class: "flex gap-2",
            // Web: hidden file input that triggers onchange
            input {
                id: "{file_input_id}",
                class: "hidden",
                r#type: "file",
                accept: "{accept}",
                onchange: {
                    let fid = file_input_id.clone();
                    let on_upload = props.on_upload_file;
                    let mut uploading = props.uploading;
                    let mut upload_error = props.upload_error;
                    move |_: Event<FormData>| {
                        let fid = fid.clone();
                        let on_upload = on_upload;
                        spawn(async move {
                            upload_error.set(None);
                            match read_attachment_bytes_web(&fid).await {
                                Ok((name, bytes, mime)) => {
                                    uploading.set(true);
                                    on_upload.call((name, bytes, mime));
                                    uploading.set(false);
                                }
                                Err(e) => {
                                    upload_error.set(Some(e));
                                }
                            }
                        });
                    }
                },
            }

            input {
                class: "flex-1 p-2 border border-border rounded-lg bg-background text-sm disabled:opacity-50",
                r#type: "text",
                placeholder: if *props.uploading.read() { "Uploading file..." } else if props.locked { "Locked" } else { "Type a message..." },
                disabled: props.locked || *props.uploading.read(),
                value: "{props.input}",
                oninput: {
                    let mut input = props.input;
                    move |e| input.set(e.value())
                },
                onkeydown: {
                    let input_val = props.input.read().clone();
                    let on_send = props.on_send;
                    let locked = props.locked;
                    let uploading = props.uploading;
                    let mut input_sig = props.input;
                    move |e: KeyboardEvent| {
                        if e.key() == Key::Enter && !locked && !*uploading.read() {
                            let text = input_val.trim().to_string();
                            if !text.is_empty() {
                                input_sig.set(String::new());
                                on_send.call(text);
                            }
                        }
                    }
                },
            }

            {
                let attach_disabled = props.locked || *props.uploading.read();
                let _fid = file_input_id.clone();
                let _on_upload = props.on_upload_file;
                let _uploading = props.uploading;
                let _upload_error = props.upload_error;
                rsx! {
                    button {
                        class: "p-2 hover:bg-accent rounded-lg text-sm disabled:opacity-50",
                        disabled: attach_disabled,
                        title: "Attach file",
                        onclick: move |_| {
                            #[cfg(feature = "web")]
                            {
                                let fid = _fid.clone();
                                let window = match web_sys::window() {
                                    Some(w) => w,
                                    None => return,
                                };
                                let document = match window.document() {
                                    Some(d) => d,
                                    None => return,
                                };
                                if let Some(el) = document.get_element_by_id(&fid) {
                                    if let Ok(inp) = el.dyn_into::<HtmlInputElement>() {
                                        inp.set_value("");
                                        inp.click();
                                    }
                                }
                            }
                            #[cfg(not(feature = "web"))]
                            {
                                pick_file_non_web(_on_upload, _uploading, _upload_error);
                            }
                        },
                        if *props.uploading.read() {
                            span { class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin text-muted-foreground" }
                        } else {
                            crate::components::icons::FileIcon { class: "w-4 h-4".to_string() }
                        }
                    }
                }
            }

            button {
                class: "px-3 py-2 bg-primary text-primary-foreground rounded-lg text-sm disabled:opacity-50",
                disabled: props.locked || *props.uploading.read() || props.input.read().trim().is_empty(),
                onclick: {
                    let input_val = props.input.read().clone();
                    let mut input_sig = props.input;
                    let on_send = props.on_send;
                    move |_| {
                        let text = input_val.trim().to_string();
                        if !text.is_empty() {
                            input_sig.set(String::new());
                            on_send.call(text);
                        }
                    }
                },
                "Send"
            }
        }
    }
}

#[cfg(feature = "web")]
async fn read_attachment_bytes_web(input_id: &str) -> Result<(String, Vec<u8>, String), String> {
    use js_sys::{ArrayBuffer, Uint8Array};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::window;
    let window = window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;
    let input_el = document
        .get_element_by_id(input_id)
        .ok_or("File input not found")?;
    let input: HtmlInputElement = input_el
        .dyn_into()
        .map_err(|_| "Not an input element")?;
    let file_list = input.files().ok_or("No files selected")?;
    let file = file_list.get(0).ok_or("No file selected")?;
    let file_name = file.name();
    let mime_type = file.type_();
    if file.size() as usize > MAX_ATTACHMENT_BYTES {
        return Err(format!(
            "File too large: {} bytes (max {} MB)",
            file.size(),
            MAX_ATTACHMENT_BYTES / (1024 * 1024)
        ));
    }
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| "Failed to read file")?;
    let array_buffer: ArrayBuffer = buffer.dyn_into().map_err(|_| "Not an ArrayBuffer")?;
    let bytes = Uint8Array::new(&array_buffer).to_vec();
    Ok((file_name, bytes, mime_type))
}

#[cfg(not(feature = "web"))]
async fn read_attachment_bytes_web(
    _input_id: &str,
) -> Result<(String, Vec<u8>, String), String> {
    Err("File upload not supported on this platform".to_string())
}

#[cfg(all(feature = "native", not(feature = "mobile_platform")))]
fn chat_mime_type_from_filename(filename: &str) -> String {
    match filename
        .rsplit('.')
        .next()
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png".to_string(),
        Some("jpg") | Some("jpeg") => "image/jpeg".to_string(),
        Some("gif") => "image/gif".to_string(),
        Some("webp") => "image/webp".to_string(),
        Some("mp4") => "video/mp4".to_string(),
        Some("webm") => "video/webm".to_string(),
        Some("mp3") => "audio/mpeg".to_string(),
        Some("ogg") => "audio/ogg".to_string(),
        Some("wav") => "audio/wav".to_string(),
        Some("pdf") => "application/pdf".to_string(),
        Some("txt") => "text/plain".to_string(),
        Some("zip") => "application/zip".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}
