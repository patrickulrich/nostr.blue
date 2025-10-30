/// Nostr List Event Kinds and Utilities
///
/// NIP-51: Lists
/// Reference: https://github.com/nostr-protocol/nips/blob/master/51.md

/// NIP-51 List kinds (parameterized replaceable events)
pub const NAMED_PEOPLE: u16 = 30000;     // People list
pub const NAMED_RELAYS: u16 = 30002;     // Relay list
pub const NAMED_BOOKMARKS: u16 = 30003;  // Bookmark list
pub const NAMED_CURATIONS: u16 = 30004;  // Curation list

/// All NIP-51 list kinds
pub const LIST_KINDS: &[u16] = &[
    NAMED_PEOPLE,
    NAMED_RELAYS,
    NAMED_BOOKMARKS,
    NAMED_CURATIONS,
];

/// Get human-readable list type name from kind
pub fn get_list_type_name(kind: u16) -> &'static str {
    match kind {
        NAMED_PEOPLE => "People",
        NAMED_RELAYS => "Relays",
        NAMED_BOOKMARKS => "Bookmarks",
        NAMED_CURATIONS => "Curations",
        _ => "Custom",
    }
}

/// Get emoji icon for list type
pub fn get_list_icon(kind: u16) -> &'static str {
    match kind {
        NAMED_PEOPLE => "👥",
        NAMED_RELAYS => "🔗",
        NAMED_BOOKMARKS => "🔖",
        NAMED_CURATIONS => "📚",
        _ => "📋",
    }
}

/// Get the count of items in a list
/// Counts tags with names: p, e, t, a
pub fn get_item_count(tags: &[nostr_sdk::Tag]) -> usize {
    tags.iter()
        .filter(|tag| {
            let kind = tag.kind();
            kind == nostr_sdk::TagKind::p()
                || kind == nostr_sdk::TagKind::e()
                || kind == nostr_sdk::TagKind::t()
                || kind == nostr_sdk::TagKind::a()
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_type_names() {
        assert_eq!(get_list_type_name(NAMED_PEOPLE), "People");
        assert_eq!(get_list_type_name(NAMED_RELAYS), "Relays");
        assert_eq!(get_list_type_name(NAMED_BOOKMARKS), "Bookmarks");
        assert_eq!(get_list_type_name(NAMED_CURATIONS), "Curations");
        assert_eq!(get_list_type_name(12345), "Custom");
    }

    #[test]
    fn test_list_icons() {
        assert_eq!(get_list_icon(NAMED_PEOPLE), "👥");
        assert_eq!(get_list_icon(NAMED_RELAYS), "🔗");
        assert_eq!(get_list_icon(NAMED_BOOKMARKS), "🔖");
        assert_eq!(get_list_icon(NAMED_CURATIONS), "📚");
        assert_eq!(get_list_icon(12345), "📋");
    }
}
