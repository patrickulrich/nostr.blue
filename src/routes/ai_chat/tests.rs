#[cfg(test)]
mod tests {
    use super::super::types::{
        history_save_snapshot_key, should_suggest_image_model, ChatImage, DisplayMessage,
        DisplayRole,
    };
    use super::super::logic::{
        build_api_messages, delete_conversation_state, derive_conversation_title,
        resolve_selected_model, seeded_conversation_with_timestamp,
    };
    use crate::services::ai_chat::{ChatMessageContent, ChatMessagePart, ChatModel, ChatModelKind};
    use crate::stores::ai_chat_store::{
        PersistedChatImage, PersistedChatMessage, PersistedChatRole, PersistedChatState,
        PersistedConversation, PersistedToolCall,
    };

    fn state_with_messages(messages: Vec<PersistedChatMessage>) -> PersistedChatState {
        PersistedChatState {
            active_conversation_id: Some("conversation-1".to_string()),
            conversations: vec![PersistedConversation {
                id: "conversation-1".to_string(),
                title: "New Chat".to_string(),
                messages,
                created_at_ms: 1,
                updated_at_ms: 1,
            }],
        }
    }

    #[test]
    fn history_snapshot_key_is_deterministic() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::User,
            content: "hello".to_string(),
            images: vec![],
            tool_calls: vec![],
        }];
        let state = state_with_messages(messages.clone());
        let initial_loaded_state = state_with_messages(messages);

        let first = history_save_snapshot_key("account", &state, &initial_loaded_state);
        let second =
            history_save_snapshot_key("account", &state.clone(), &initial_loaded_state.clone());

        assert_eq!(first, second);
    }

    #[test]
    fn history_snapshot_key_changes_when_message_content_changes() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::Assistant,
            content: "before".to_string(),
            images: vec![],
            tool_calls: vec![PersistedToolCall {
                id: "tool-1".to_string(),
                name: "set_theme".to_string(),
                result: "{\"success\":true}".to_string(),
            }],
        }];
        let mut edited_messages = messages.clone();
        edited_messages[0].content = "after".to_string();
        let messages_state = state_with_messages(messages);
        let edited_state = state_with_messages(edited_messages);

        assert_ne!(
            history_save_snapshot_key("account", &messages_state, &messages_state),
            history_save_snapshot_key("account", &edited_state, &messages_state),
        );
    }

    #[test]
    fn history_snapshot_key_changes_when_tool_calls_change() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::Assistant,
            content: "hello".to_string(),
            images: vec![],
            tool_calls: vec![],
        }];
        let mut with_tool_call = messages.clone();
        with_tool_call[0].tool_calls.push(PersistedToolCall {
            id: "tool-1".to_string(),
            name: "set_theme".to_string(),
            result: "{\"success\":true}".to_string(),
        });
        let messages_state = state_with_messages(messages);
        let with_tool_call_state = state_with_messages(with_tool_call);

        assert_ne!(
            history_save_snapshot_key("account", &messages_state, &messages_state),
            history_save_snapshot_key("account", &with_tool_call_state, &messages_state),
        );
    }

    #[test]
    fn history_snapshot_key_changes_when_account_key_changes() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::Assistant,
            content: "hello".to_string(),
            images: vec![],
            tool_calls: vec![PersistedToolCall {
                id: "tool-1".to_string(),
                name: "set_theme".to_string(),
                result: "{\"success\":true}".to_string(),
            }],
        }];
        let messages_state = state_with_messages(messages);

        assert_ne!(
            history_save_snapshot_key("account-a", &messages_state, &messages_state),
            history_save_snapshot_key("account-b", &messages_state, &messages_state),
        );
    }

    #[test]
    fn history_snapshot_key_changes_when_initial_loaded_messages_change() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::User,
            content: "hello".to_string(),
            images: vec![],
            tool_calls: vec![],
        }];
        let mut different_initial_loaded_messages = messages.clone();
        different_initial_loaded_messages.push(PersistedChatMessage {
            id: "2".to_string(),
            role: PersistedChatRole::Assistant,
            content: "welcome".to_string(),
            images: vec![],
            tool_calls: vec![],
        });
        let messages_state = state_with_messages(messages);
        let different_initial_state = state_with_messages(different_initial_loaded_messages);

        assert_ne!(
            history_save_snapshot_key("account", &messages_state, &messages_state),
            history_save_snapshot_key("account", &messages_state, &different_initial_state,),
        );
    }

    #[test]
    fn seeded_conversation_uses_title_hint_and_creates_user_message() {
        let conversation =
            seeded_conversation_with_timestamp(42, Some("Bible: John 3:16"), "Bible passage text");

        assert_eq!(conversation.title, "Bible: John 3:16");
        assert_eq!(conversation.id, "conversation-42");
        assert_eq!(conversation.messages.len(), 1);
        assert_eq!(conversation.messages[0].role, PersistedChatRole::User);
        assert_eq!(conversation.messages[0].content, "Bible passage text");
    }

    #[test]
    fn seeded_conversation_falls_back_to_default_title() {
        let conversation = seeded_conversation_with_timestamp(42, Some("   "), "Seeded message");

        assert_eq!(conversation.title, "New Chat");
    }

    #[test]
    fn derive_conversation_title_uses_image_fallback_for_image_only_first_message() {
        let title = derive_conversation_title(
            "New Chat",
            &[PersistedChatMessage {
                id: "1".to_string(),
                role: PersistedChatRole::User,
                content: String::new(),
                images: vec![PersistedChatImage {
                    url: "https://cdn.example.com/image.png".to_string(),
                    alt: String::new(),
                    title: String::new(),
                }],
                tool_calls: vec![],
            }],
        );

        assert_eq!(title, "Image");
    }

    #[test]
    fn delete_conversation_state_returns_empty_state_when_last_conversation_is_removed() {
        let conversation = PersistedConversation {
            id: "conversation-1".to_string(),
            title: "New Chat".to_string(),
            messages: vec![PersistedChatMessage {
                id: "message-1".to_string(),
                role: PersistedChatRole::User,
                content: "hello".to_string(),
                images: vec![],
                tool_calls: vec![],
            }],
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let (conversations, active_id, messages) =
            delete_conversation_state(&[conversation], "conversation-1");

        assert!(conversations.is_empty());
        assert!(active_id.is_none());
        assert!(messages.is_empty());
    }

    #[test]
    fn delete_conversation_state_selects_most_recent_remaining_conversation() {
        let older = PersistedConversation {
            id: "conversation-1".to_string(),
            title: "Older".to_string(),
            messages: vec![],
            created_at_ms: 1,
            updated_at_ms: 5,
        };
        let newer = PersistedConversation {
            id: "conversation-2".to_string(),
            title: "Newer".to_string(),
            messages: vec![PersistedChatMessage {
                id: "message-1".to_string(),
                role: PersistedChatRole::Assistant,
                content: "welcome".to_string(),
                images: vec![],
                tool_calls: vec![],
            }],
            created_at_ms: 2,
            updated_at_ms: 10,
        };

        let (conversations, active_id, messages) =
            delete_conversation_state(&[older, newer.clone()], "conversation-1");

        assert_eq!(conversations.len(), 1);
        assert_eq!(active_id, Some("conversation-2".to_string()));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, newer.messages[0].content);
    }

    #[test]
    fn resolve_selected_model_prefers_current_when_valid() {
        let models = vec![
            ChatModel {
                id: "model-a".to_string(),
                name: "Model A".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
            ChatModel {
                id: "model-b".to_string(),
                name: "Model B".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
        ];

        assert_eq!(
            resolve_selected_model("model-b", Some("model-a"), &models),
            "model-b"
        );
    }

    #[test]
    fn resolve_selected_model_falls_back_to_saved_model() {
        let models = vec![
            ChatModel {
                id: "model-a".to_string(),
                name: "Model A".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
            ChatModel {
                id: "model-b".to_string(),
                name: "Model B".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
        ];

        assert_eq!(
            resolve_selected_model("missing", Some("model-b"), &models),
            "model-b"
        );
    }

    #[test]
    fn resolve_selected_model_defaults_to_first_available_model() {
        let models = vec![
            ChatModel {
                id: "model-a".to_string(),
                name: "Model A".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
            ChatModel {
                id: "model-b".to_string(),
                name: "Model B".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
        ];

        assert_eq!(
            resolve_selected_model("missing", Some("also-missing"), &models),
            "model-a"
        );
    }

    #[test]
    fn resolve_selected_model_returns_empty_without_available_models() {
        assert!(resolve_selected_model("missing", Some("also-missing"), &[]).is_empty());
    }

    #[test]
    fn suggests_image_model_without_blocking_conditions() {
        let active_model = ChatModel {
            id: "text-model".to_string(),
            name: "Text Model".to_string(),
            description: String::new(),
            kind: ChatModelKind::Chat,
            supports_image_input: false,
            total_cost: None,
        };

        assert!(should_suggest_image_model(
            &active_model,
            &[],
            true,
            "Generate a photorealistic sunset over mountains"
        ));
        assert!(!should_suggest_image_model(
            &active_model,
            &[ChatImage::default()],
            true,
            "Generate a photorealistic sunset over mountains"
        ));
    }

    #[test]
    fn build_api_messages_keeps_assistant_images_as_parts() {
        let api_messages = build_api_messages(&[DisplayMessage {
            id: "1".to_string(),
            role: DisplayRole::Assistant,
            content: "See attached".to_string(),
            images: vec![ChatImage {
                url: "https://cdn.example.com/image.png".to_string(),
                alt: String::new(),
                title: String::new(),
            }],
            tool_calls: vec![],
        }]);

        assert_eq!(api_messages.len(), 2);
        match &api_messages[1].content {
            ChatMessageContent::Parts(parts) => {
                assert_eq!(
                    parts[0],
                    ChatMessagePart::Text {
                        text: "See attached".to_string()
                    }
                );
                assert_eq!(
                    parts[1],
                    ChatMessagePart::ImageUrl {
                        image_url: crate::services::ai_chat::ChatImageUrl {
                            url: "https://cdn.example.com/image.png".to_string()
                        }
                    }
                );
            }
            other => panic!("expected multipart assistant message, got {other:?}"),
        }
    }
}
