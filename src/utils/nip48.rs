//! NIP-48: Proxy Tags
//!
//! Extracts proxy information from bridged Nostr events.
//! Events bridged from other protocols (ActivityPub, ATProto, RSS, Web)
//! can include a "proxy" tag linking back to the original source.
//!
//! Tag format: ["proxy", <id>, <protocol>]
//!
//! Supported protocols:
//! - activitypub: Mastodon/Fediverse (URL format)
//! - atproto: Bluesky (AT URI format)
//! - rss: RSS feeds (URL with guid fragment)
//! - web: Generic web content (URL format)
use nostr::nips::nip48::Protocol;
use nostr_sdk::prelude::*;
/// Extracted proxy tag information
#[derive(Clone, Debug, PartialEq)]
pub struct ProxyInfo {
    /// The ID/URL of the source content
    pub id: String,
    /// The protocol the content originated from
    pub protocol: Protocol,
}
impl ProxyInfo {
    /// Get a human-readable display name for the protocol
    pub fn display_name(&self) -> &'static str {
        get_protocol_display_name(&self.protocol)
    }
}
/// Extract all proxy tags from an event
///
/// Returns a vector of ProxyInfo structs, one for each proxy tag found.
/// Most bridged events will have exactly one proxy tag.
pub fn extract_proxy_tags(event: &Event) -> Vec<ProxyInfo> {
    event
        .tags
        .iter()
        .filter_map(|tag| {
            if let Some(TagStandard::Proxy { id, protocol }) = tag.as_standardized() {
                Some(ProxyInfo {
                    id: id.clone(),
                    protocol: protocol.clone(),
                })
            } else {
                None
            }
        })
        .collect()
}
/// Get the first proxy tag from an event, if any
///
/// Convenience function for the common case where only one proxy tag exists.
pub fn get_proxy_info(event: &Event) -> Option<ProxyInfo> {
    extract_proxy_tags(event).into_iter().next()
}
/// Get a human-readable display name for a protocol
pub fn get_protocol_display_name(protocol: &Protocol) -> &'static str {
    match protocol {
        Protocol::ActivityPub => "Fediverse",
        Protocol::ATProto => "Bluesky",
        Protocol::Rss => "RSS",
        Protocol::Web => "Web",
        Protocol::Custom(_) => "External",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_protocol_display_names() {
        assert_eq!(get_protocol_display_name(&Protocol::ActivityPub), "Fediverse");
        assert_eq!(get_protocol_display_name(&Protocol::ATProto), "Bluesky");
        assert_eq!(get_protocol_display_name(&Protocol::Rss), "RSS");
        assert_eq!(get_protocol_display_name(&Protocol::Web), "Web");
        assert_eq!(
            get_protocol_display_name(&Protocol::Custom("custom".to_string())),
            "External",
        );
    }
}
