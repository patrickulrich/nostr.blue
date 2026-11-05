
pub fn get_content_warning(tags: &nostr::event::Tags) -> Option<Option<String>> {
    tags.iter()
        .find(|t| t.kind() == "content-warning")
        .map(|t| t.content().map(|s| s.to_string()))
}
