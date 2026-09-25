use crate::stores::signer::SignerType;

pub async fn sign_event_builder(
    builder: nostr::event::EventBuilder,
) -> Result<nostr::event::Event, String> {
    let signer =
        crate::stores::signer::get_signer().ok_or_else(|| "No signer available".to_string())?;
    sign_event_builder_with_signer(builder, signer).await
}

pub async fn sign_event_builder_with_signer(
    builder: nostr::event::EventBuilder,
    signer: SignerType,
) -> Result<nostr::event::Event, String> {
    let builder = crate::utils::nips::nip89::tag_event_builder(builder);
    use nostr::event::{FinalizeEvent, FinalizeEventAsync};
    match signer {
        SignerType::Keys(keys) => builder
            .finalize(&keys)
            .map_err(|e| format!("Failed to sign event: {}", e)),
        #[cfg(target_family = "wasm")]
        SignerType::BrowserExtension(browser_signer) => builder
            .finalize_async(&*browser_signer)
            .await
            .map_err(|e| format!("Failed to sign event: {}", e)),
        SignerType::NostrConnect(remote_signer) => builder
            .finalize_async(&*remote_signer)
            .await
            .map_err(|e| format!("Failed to sign event: {}", e)),
        #[cfg(feature = "mobile_platform")]
        SignerType::AndroidSigner(android_signer) => builder
            .finalize_async(&*android_signer)
            .await
            .map_err(|e| format!("Failed to sign event: {}", e)),
    }
}
