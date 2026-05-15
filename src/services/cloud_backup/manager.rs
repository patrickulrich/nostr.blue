use nostr::ToBech32;

use super::crypto;
use super::types::{BackupBundle, BackupEntry, GoogleAuthResult, ZeroizeString};

const BACKUP_FILE_PREFIX: &str = "nostrblue_backup_";
const BACKUP_FILE_SUFFIX: &str = ".bin";

#[allow(dead_code)]
fn backup_filename(npub: &str) -> String {
    format!("{}{}{}", BACKUP_FILE_PREFIX, npub, BACKUP_FILE_SUFFIX)
}

fn npub_from_filename(name: &str) -> Option<String> {
    if name.starts_with(BACKUP_FILE_PREFIX) && name.ends_with(BACKUP_FILE_SUFFIX) {
        let npub = &name[BACKUP_FILE_PREFIX.len()..name.len() - BACKUP_FILE_SUFFIX.len()];
        if npub.starts_with("npub1") {
            return Some(npub.to_string());
        }
    }
    None
}

pub async fn google_sign_in() -> Result<GoogleAuthResult, String> {
    #[cfg(target_family = "wasm")]
    {
        super::web::google_sign_in().await
    }
    #[cfg(all(target_os = "android", feature = "mobile_platform"))]
    {
        tokio::task::spawn_blocking(|| super::android::google_sign_in())
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(target_family = "wasm", all(target_os = "android", feature = "mobile_platform"))))]
    {
        Err("Google sign-in not available on this platform".to_string())
    }
}

pub async fn list_cloud_backups(
    auth: &GoogleAuthResult,
) -> Result<Vec<BackupEntry>, String> {
    let files = list_raw(&auth.access_token).await?;
    let mut entries = Vec::new();
    for (file_id, name) in files {
        if let Some(npub) = npub_from_filename(&name) {
            entries.push(BackupEntry {
                file_id,
                npub,
                display_name: None,
                picture: None,
            });
        }
    }
    Ok(entries)
}

pub async fn backup_to_cloud(
    nsec_hex: &str,
    nwc_uri: Option<&str>,
    account_label: Option<&str>,
    auth: &GoogleAuthResult,
) -> Result<(), String> {
    let keys =
        nostr::Keys::new(nostr::SecretKey::from_hex(nsec_hex).map_err(|e| e.to_string())?);
    let npub = keys
        .public_key()
        .to_bech32()
        .map_err(|e| e.to_string())?;

    let bundle = BackupBundle {
        nsec_hex: ZeroizeString(nsec_hex.to_string()),
        nwc_uri: nwc_uri.map(|s| s.to_string()),
        account_label: account_label.map(|s| s.to_string()),
        created_at: crate::platform::timestamp::now_secs(),
    };

    let key = crypto::derive_backup_key(&auth.sub);
    let payload = crypto::encrypt_bundle(&bundle, &key)?;

    upload_raw(&auth.access_token, &npub, &payload).await
}

pub async fn restore_from_cloud(
    file_id: &str,
    auth: &GoogleAuthResult,
) -> Result<BackupBundle, String> {
    let payload_b64 = download_raw(&auth.access_token, file_id).await?;
    let key = crypto::derive_backup_key(&auth.sub);
    crypto::decrypt_bundle(&payload_b64, &key)
}

#[allow(dead_code)]
pub async fn delete_cloud_backup(file_id: &str, auth: &GoogleAuthResult) -> Result<(), String> {
    delete_raw(&auth.access_token, file_id).await
}

#[allow(unused_variables)]
async fn list_raw(access_token: &str) -> Result<Vec<(String, String)>, String> {
    #[cfg(target_family = "wasm")]
    {
        super::web::list_backups(access_token).await
    }
    #[cfg(all(target_os = "android", feature = "mobile_platform"))]
    {
        let access_token = access_token.to_string();
        tokio::task::spawn_blocking(move || super::android::list_backups(&access_token))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(target_family = "wasm", all(target_os = "android", feature = "mobile_platform"))))]
    {
        Err("Not available on this platform".to_string())
    }
}

async fn upload_raw(
    access_token: &str,
    npub: &str,
    payload_b64: &str,
) -> Result<(), String> {
    #[cfg(target_family = "wasm")]
    {
        super::web::upload_backup(access_token, npub, payload_b64).await
    }
    #[cfg(all(target_os = "android", feature = "mobile_platform"))]
    {
        let access_token = access_token.to_string();
        let npub = npub.to_string();
        let payload_b64 = payload_b64.to_string();
        tokio::task::spawn_blocking(move || {
            super::android::upload_backup(&access_token, &npub, &payload_b64)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(target_family = "wasm", all(target_os = "android", feature = "mobile_platform"))))]
    {
        let _ = (access_token, npub, payload_b64);
        Err("Not available on this platform".to_string())
    }
}

async fn download_raw(access_token: &str, file_id: &str) -> Result<String, String> {
    #[cfg(target_family = "wasm")]
    {
        super::web::download_backup(access_token, file_id).await
    }
    #[cfg(all(target_os = "android", feature = "mobile_platform"))]
    {
        let access_token = access_token.to_string();
        let file_id = file_id.to_string();
        tokio::task::spawn_blocking(move || {
            super::android::download_backup(&access_token, &file_id)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(target_family = "wasm", all(target_os = "android", feature = "mobile_platform"))))]
    {
        let _ = (access_token, file_id);
        Err("Not available on this platform".to_string())
    }
}

#[allow(dead_code)]
async fn delete_raw(access_token: &str, file_id: &str) -> Result<(), String> {
    #[cfg(target_family = "wasm")]
    {
        super::web::delete_backup(access_token, file_id).await
    }
    #[cfg(all(target_os = "android", feature = "mobile_platform"))]
    {
        let access_token = access_token.to_string();
        let file_id = file_id.to_string();
        tokio::task::spawn_blocking(move || {
            super::android::delete_backup(&access_token, &file_id)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(target_family = "wasm", all(target_os = "android", feature = "mobile_platform"))))]
    {
        let _ = (access_token, file_id);
        Err("Not available on this platform".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_filename() {
        assert_eq!(
            backup_filename("npub1abc"),
            "nostrblue_backup_npub1abc.bin"
        );
    }

    #[test]
    fn test_npub_from_filename() {
        assert_eq!(
            npub_from_filename("nostrblue_backup_npub1abc.bin"),
            Some("npub1abc".to_string())
        );
        assert_eq!(npub_from_filename("other_file.bin"), None);
        assert_eq!(npub_from_filename("nostrblue_backup_hex123.bin"), None);
    }
}
