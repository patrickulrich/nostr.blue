use dioxus::prelude::ReadableExt;
use nostr_sdk::{EventBuilder, Tag};

pub const CLIENT_NAME: &str = "nostr.blue";

pub fn client_tag() -> Tag {
    Tag::client(CLIENT_NAME)
}

pub fn should_publish_client_tag() -> bool {
    crate::stores::settings_store::SETTINGS
        .read()
        .publish_client_tag
}

pub fn tag_event_builder(builder: EventBuilder) -> EventBuilder {
    if should_publish_client_tag() {
        builder.tag(client_tag())
    } else {
        builder
    }
}

#[cfg(test)]
mod tests {
    use super::{client_tag, CLIENT_NAME};

    #[test]
    fn client_tag_uses_standard_sdk_format() {
        assert_eq!(client_tag().to_vec(), vec!["client", CLIENT_NAME]);
    }
}
