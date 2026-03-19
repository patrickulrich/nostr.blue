use dioxus::prelude::ReadableExt;
use nostr_sdk::{EventBuilder, Tag, TagKind};

pub const CLIENT_NAME: &str = "nostr.blue";

pub fn client_tag() -> Tag {
    Tag::client(CLIENT_NAME)
}

pub fn should_publish_client_tag() -> bool {
    crate::stores::settings_store::SETTINGS
        .read()
        .publish_client_tag
}

pub fn tag_event_builder_with_enabled(builder: EventBuilder, publish_client_tag: bool) -> EventBuilder {
    if publish_client_tag {
        builder.tag(client_tag())
    } else {
        builder
    }
}

pub fn tag_event_builder(builder: EventBuilder) -> EventBuilder {
    tag_event_builder_with_enabled(builder, should_publish_client_tag())
}

pub fn strip_client_tags(tags: Vec<Tag>) -> Vec<Tag> {
    tags.into_iter()
        .filter(|tag| tag.kind() != TagKind::Client)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{client_tag, strip_client_tags, tag_event_builder_with_enabled, CLIENT_NAME};
    use crate::stores::settings_store::AppSettings;
    use nostr_sdk::{EventBuilder, Keys, Kind, TagKind};

    #[test]
    fn client_tag_uses_standard_sdk_format() {
        assert_eq!(client_tag().to_vec(), vec!["client", CLIENT_NAME]);
    }

    #[test]
    fn strip_client_tags_removes_existing_client_tags() {
        let tags = vec![
            client_tag(),
            nostr_sdk::Tag::identifier("demo"),
            client_tag(),
        ];

        let stripped = strip_client_tags(tags);
        assert_eq!(stripped.len(), 1);
        assert_eq!(stripped[0].clone().to_vec(), vec!["d", "demo"]);
    }

    #[test]
    fn settings_default_keeps_client_tag_enabled() {
        assert!(AppSettings::default().publish_client_tag);
    }

    #[test]
    fn explicit_tagging_flag_controls_client_tag() {
        let keys = Keys::generate();
        let tagged = tag_event_builder_with_enabled(EventBuilder::new(Kind::TextNote, "tagged"), true)
            .sign_with_keys(&keys)
            .unwrap();
        assert!(tagged.tags.iter().any(|tag| tag.kind() == TagKind::Client));

        let untagged = tag_event_builder_with_enabled(EventBuilder::new(Kind::TextNote, "plain"), false)
            .sign_with_keys(&keys)
            .unwrap();
        assert!(!untagged.tags.iter().any(|tag| tag.kind() == TagKind::Client));
    }
}
