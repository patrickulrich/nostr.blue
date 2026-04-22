use nostr_sdk::{TagKind, TagStandard};

pub fn get_content_warning(tags: &nostr_sdk::Tags) -> Option<Option<String>> {
    tags.find_standardized(TagKind::ContentWarning).map(|t| {
        if let TagStandard::ContentWarning { reason } = t {
            reason.clone()
        } else {
            None
        }
    })
}
