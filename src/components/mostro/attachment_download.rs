use crate::stores::mostro::encrypted_attachment::AttachmentMeta;
use dioxus::prelude::*;

/// Download a Blossom blob, decrypt it with the shared key, and save the
/// plaintext locally. Returns the filename on success.
///
/// Uses the spec-compatible `decrypt_attachment` which extracts the nonce
/// from the first 12 bytes of the blob. If the blob is too short for the
/// spec format, falls back to the legacy decrypt path using the nonce
/// from `AttachmentMeta`.
pub async fn download_and_decrypt_attachment(
    att: &AttachmentMeta,
    shared_key_hex: &str,
) -> Result<String, String> {
    let key_bytes = hex::decode(shared_key_hex)
        .map_err(|e| format!("invalid shared key hex: {e}"))?;
    if key_bytes.len() != 32 {
        return Err(format!(
            "shared key must be 32 bytes, got {}",
            key_bytes.len()
        ));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);

    let blob = crate::stores::media::blossom_store::download_blob(&att.blossom_url).await?;

    let plaintext = match crate::stores::mostro::encrypted_attachment::decrypt_attachment(
        &blob, &key,
    ) {
        Ok(p) => p,
        Err(spec_err) => {
            log::debug!(
                "spec decrypt failed ({spec_err}), trying legacy nonce from meta"
            );
            let nonce = att.parse_nonce().map_err(|e| {
                format!("both spec and legacy decrypt failed: nonce parse: {e}")
            })?;
            crate::stores::mostro::encrypted_attachment::decrypt_attachment_legacy(
                &blob, &nonce, &key,
            )?
        }
    };

    let filename = att
        .filename
        .clone()
        .unwrap_or_else(|| format!("attachment.{}", mime_to_extension(&att.mime_type)));
    crate::platform::download::save_bytes(&filename, &plaintext, &att.mime_type)?;

    Ok(filename)
}

fn mime_to_extension(mime: &str) -> &str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/wav" => "wav",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        "application/zip" => "zip",
        _ => "bin",
    }
}

/// Standalone download button component for chat attachments. Owns its
/// own `downloading` signal to avoid rsx capture issues with mutability
/// in the parent component.
///
/// Shared between `TradeChat` and `DisputeChat` so both get
/// decrypt-on-download functionality.
#[derive(Props, Clone, PartialEq)]
pub struct AttachmentDownloadButtonProps {
    pub att: AttachmentMeta,
    pub shared_key_hex: String,
}

#[component]
pub fn AttachmentDownloadButton(props: AttachmentDownloadButtonProps) -> Element {
    let mut downloading = use_signal(|| false);
    rsx! {
        button {
            class: "text-primary hover:underline",
            disabled: *downloading.read(),
            onclick: move |_| {
                let att = props.att.clone();
                let key = props.shared_key_hex.clone();
                spawn(async move {
                    downloading.set(true);
                    match download_and_decrypt_attachment(&att, &key).await {
                        Ok(filename) => {
                            let toast = dioxus_primitives::toast::consume_toast();
                            toast.info(
                                "Downloaded".to_string(),
                                dioxus_primitives::toast::ToastOptions::new()
                                    .description(filename)
                                    .duration(std::time::Duration::from_secs(3)),
                            );
                        }
                        Err(e) => {
                            log::warn!("attachment download failed: {e}");
                            let toast = dioxus_primitives::toast::consume_toast();
                            toast.error(
                                "Download failed".to_string(),
                                dioxus_primitives::toast::ToastOptions::new()
                                    .description(e)
                                    .duration(std::time::Duration::from_secs(5)),
                            );
                        }
                    }
                    downloading.set(false);
                });
            },
            if *downloading.read() { "Downloading…" } else { "Download" }
        }
    }
}
