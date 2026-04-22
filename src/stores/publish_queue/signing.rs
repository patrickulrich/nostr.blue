use crate::stores::signer::SignerType;

pub async fn sign_event_builder(
    builder: nostr_sdk::EventBuilder,
) -> Result<nostr_sdk::Event, String> {
    let signer =
        crate::stores::signer::get_signer().ok_or_else(|| "No signer available".to_string())?;
    sign_event_builder_with_signer(builder, signer).await
}

pub async fn sign_event_builder_with_signer(
    builder: nostr_sdk::EventBuilder,
    signer: SignerType,
) -> Result<nostr_sdk::Event, String> {
    let builder = crate::utils::nips::nip89::tag_event_builder(builder);
    match signer {
        SignerType::Keys(keys) => builder
            .sign_with_keys(&keys)
            .map_err(|e| format!("Failed to sign event: {}", e)),
        #[cfg(target_family = "wasm")]
        SignerType::BrowserExtension(browser_signer) => builder
            .sign(&*browser_signer)
            .await
            .map_err(|e| format!("Failed to sign event: {}", e)),
        SignerType::NostrConnect(remote_signer) => builder
            .sign(&*remote_signer)
            .await
            .map_err(|e| format!("Failed to sign event: {}", e)),
        #[cfg(feature = "mobile_platform")]
        SignerType::AndroidSigner(android_signer) => builder
            .sign(&*android_signer)
            .await
            .map_err(|e| format!("Failed to sign event: {}", e)),
    }
}
