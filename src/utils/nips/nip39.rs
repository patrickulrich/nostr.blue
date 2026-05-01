//! NIP-39: External Identities in Profiles
//!
//! Handles parsing and publishing of kind 10011 events with `i` tags
//! that link Nostr profiles to external identities (GitHub, Twitter,
//! Mastodon, Telegram).
//!
//! Tag format: `["i", "platform:identity", "proof"]`
use crate::stores::nostr_client;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub const KIND_EXTERNAL_IDENTITIES: u16 = 10011;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExternalIdentityInfo {
    pub platform: String,
    pub ident: String,
    pub proof: String,
}

impl ExternalIdentityInfo {
    pub fn proof_url(&self) -> String {
        match self.platform.as_str() {
            "github" => format!("https://gist.github.com/{}/{}", self.ident, self.proof),
            "twitter" => format!("https://x.com/{}/status/{}", self.ident, self.proof),
            "mastodon" => format!("https://{}/{}", self.ident, self.proof),
            "telegram" => format!("https://t.me/{}", self.proof),
            _ => String::new(),
        }
    }

    pub fn display_name(&self) -> String {
        if self.platform == "mastodon" {
            if let Some(pos) = self.ident.find("/@") {
                let domain = &self.ident[..pos];
                let username = &self.ident[pos + 2..];
                return format!("{}@{}", username, domain);
            }
        }
        self.ident.clone()
    }
}

impl From<&Identity> for ExternalIdentityInfo {
    fn from(identity: &Identity) -> Self {
        Self {
            platform: identity.platform.to_string(),
            ident: identity.ident.clone(),
            proof: identity.proof.clone(),
        }
    }
}

pub fn parse_identities(event: &Event) -> Vec<ExternalIdentityInfo> {
    event
        .tags
        .iter()
        .filter_map(|tag| {
            if let Some(TagStandard::ExternalIdentity(identity)) = tag.as_standardized() {
                Some(ExternalIdentityInfo::from(identity))
            } else {
                None
            }
        })
        .collect()
}

pub async fn fetch_external_identities(pubkey: &str) -> Result<Vec<ExternalIdentityInfo>, String> {
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_EXTERNAL_IDENTITIES))
        .author(pk)
        .limit(1);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch external identities: {}", e))?;

    if let Some(event) = events
        .into_iter()
        .max_by_key(|e| e.created_at.as_secs())
    {
        let identities = parse_identities(&event);
        if !identities.is_empty() {
            return Ok(identities);
        }
    }

    Ok(Vec::new())
}

pub fn parse_github_proof_url(url: &str) -> Option<Identity> {
    let url = url.trim();
    let path = url
        .strip_prefix("https://gist.github.com/")
        .or_else(|| url.strip_prefix("http://gist.github.com/"))?;
    let path = path.split('?').next()?;
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 2 {
        let username = parts[0].trim_end_matches('/');
        let gist_id = parts[1].trim_end_matches('/');
        if !username.is_empty() && !gist_id.is_empty() {
            return Identity::new(
                format!("github:{}", username.to_lowercase()),
                gist_id.to_string(),
            )
            .ok();
        }
    }
    None
}

pub fn parse_twitter_proof_url(url: &str) -> Option<Identity> {
    let url = url.trim();
    let path = url
        .strip_prefix("https://x.com/")
        .or_else(|| url.strip_prefix("https://twitter.com/"))
        .or_else(|| url.strip_prefix("http://x.com/"))
        .or_else(|| url.strip_prefix("http://twitter.com/"))?;
    let path = path.split('?').next()?;
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 3 && parts[1] == "status" {
        let username = parts[0].trim_end_matches('/');
        let status_id = parts[2].trim_end_matches('/');
        if !username.is_empty() && !status_id.is_empty() {
            return Identity::new(
                format!("twitter:{}", username.to_lowercase()),
                status_id.to_string(),
            )
            .ok();
        }
    }
    None
}

pub fn parse_mastodon_proof_url(url: &str) -> Option<Identity> {
    let url = url.trim();
    let path = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let path = path.split('?').next()?;
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 3 {
        let server = parts[0];
        let user_part = parts[1];
        let post_id = parts[2].trim_end_matches('/');
        if !server.is_empty() && !post_id.is_empty() {
            let username = user_part.strip_prefix('@').unwrap_or(user_part);
            let ident = format!("{}/@{}", server, username);
            return Identity::new(format!("mastodon:{}", ident), post_id.to_string()).ok();
        }
    }
    None
}

pub async fn publish_external_identities(
    identities: Vec<Identity>,
) -> Result<String, String> {
    let _client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !nostr_client::has_signer() {
        return Err("No signer available".to_string());
    }

    let tags: Vec<Tag> = identities
        .into_iter()
        .map(|id| {
            Tag::from_standardized_without_cell(TagStandard::ExternalIdentity(id))
        })
        .collect();

    let builder = EventBuilder::new(Kind::Custom(KIND_EXTERNAL_IDENTITIES), "").tags(tags);

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign external identities: {}", e))?;

    let event_id = event.id.to_hex();

    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other(
            "external_identity".to_string(),
        ),
        None,
        HashMap::new(),
    )
    .await;

    Ok(event_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_proof_url() {
        let id = parse_github_proof_url("https://gist.github.com/semisol/9721ce4ee4fceb91c9711ca2a6c9a5ab").unwrap();
        assert_eq!(id.platform, ExternalIdentity::GitHub);
        assert_eq!(id.ident, "semisol");
        assert_eq!(id.proof, "9721ce4ee4fceb91c9711ca2a6c9a5ab");
    }

    #[test]
    fn test_parse_twitter_proof_url() {
        let id = parse_twitter_proof_url("https://x.com/semisol_public/status/1619358434134196225").unwrap();
        assert_eq!(id.platform, ExternalIdentity::Twitter);
        assert_eq!(id.ident, "semisol_public");
        assert_eq!(id.proof, "1619358434134196225");
    }

    #[test]
    fn test_parse_twitter_proof_url_legacy() {
        let id = parse_twitter_proof_url("https://twitter.com/semisol_public/status/1619358434134196225").unwrap();
        assert_eq!(id.platform, ExternalIdentity::Twitter);
        assert_eq!(id.ident, "semisol_public");
    }

    #[test]
    fn test_parse_mastodon_proof_url() {
        let id = parse_mastodon_proof_url("https://bitcoinhackers.org/@semisol/109775066355589974").unwrap();
        assert_eq!(id.platform, ExternalIdentity::Mastodon);
        assert_eq!(id.ident, "bitcoinhackers.org/@semisol");
        assert_eq!(id.proof, "109775066355589974");
    }

    #[test]
    fn test_proof_url_github() {
        let info = ExternalIdentityInfo {
            platform: "github".to_string(),
            ident: "semisol".to_string(),
            proof: "9721ce4ee4fceb91c9711ca2a6c9a5ab".to_string(),
        };
        assert_eq!(info.proof_url(), "https://gist.github.com/semisol/9721ce4ee4fceb91c9711ca2a6c9a5ab");
    }

    #[test]
    fn test_proof_url_twitter() {
        let info = ExternalIdentityInfo {
            platform: "twitter".to_string(),
            ident: "semisol_public".to_string(),
            proof: "1619358434134196225".to_string(),
        };
        assert_eq!(info.proof_url(), "https://x.com/semisol_public/status/1619358434134196225");
    }

    #[test]
    fn test_proof_url_mastodon() {
        let info = ExternalIdentityInfo {
            platform: "mastodon".to_string(),
            ident: "bitcoinhackers.org/@semisol".to_string(),
            proof: "109775066355589974".to_string(),
        };
        assert_eq!(info.proof_url(), "https://bitcoinhackers.org/@semisol/109775066355589974");
    }

    #[test]
    fn test_proof_url_telegram() {
        let info = ExternalIdentityInfo {
            platform: "telegram".to_string(),
            ident: "1087295469".to_string(),
            proof: "nostrdirectory/770".to_string(),
        };
        assert_eq!(info.proof_url(), "https://t.me/nostrdirectory/770");
    }

    #[test]
    fn test_display_name_mastodon() {
        let info = ExternalIdentityInfo {
            platform: "mastodon".to_string(),
            ident: "bitcoinhackers.org/@semisol".to_string(),
            proof: "109775066355589974".to_string(),
        };
        assert_eq!(info.display_name(), "semisol@bitcoinhackers.org");
    }

    #[test]
    fn test_display_name_github() {
        let info = ExternalIdentityInfo {
            platform: "github".to_string(),
            ident: "semisol".to_string(),
            proof: "abc".to_string(),
        };
        assert_eq!(info.display_name(), "semisol");
    }

    #[test]
    fn test_parse_invalid_urls() {
        assert!(parse_github_proof_url("not a url").is_none());
        assert!(parse_twitter_proof_url("https://x.com/").is_none());
        assert!(parse_mastodon_proof_url("not a url").is_none());
    }
}
