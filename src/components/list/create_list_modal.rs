//! Create List Modal
//!
//! Modal for creating new NIP-51 lists with support for:
//! - People lists (kind 30000)
//! - Relay lists (kind 30002)
//! - Bookmark lists (kind 30003)
//! - Curation lists (kind 30004)
//!
//! People lists support NIP-44 encrypted private members.
use crate::stores::nostr_client;
use crate::utils::list_kinds::{NAMED_BOOKMARKS, NAMED_CURATIONS, NAMED_PEOPLE, NAMED_RELAYS};
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, Kind, Tag};
/// List type for creation
#[derive(Clone, Copy, PartialEq, Debug)]
enum ListType {
    People,
    Relays,
    Bookmarks,
    Curations,
}
impl ListType {
    fn kind(&self) -> u16 {
        match self {
            ListType::People => NAMED_PEOPLE,
            ListType::Relays => NAMED_RELAYS,
            ListType::Bookmarks => NAMED_BOOKMARKS,
            ListType::Curations => NAMED_CURATIONS,
        }
    }
    fn label(&self) -> &'static str {
        match self {
            ListType::People => "People List",
            ListType::Relays => "Relay List",
            ListType::Bookmarks => "Bookmark List",
            ListType::Curations => "Curation List",
        }
    }
    fn icon(&self) -> &'static str {
        match self {
            ListType::People => "👥",
            ListType::Relays => "🔗",
            ListType::Bookmarks => "🔖",
            ListType::Curations => "📚",
        }
    }
    fn description(&self) -> &'static str {
        match self {
            ListType::People => "Organize contacts into groups",
            ListType::Relays => "Save relay configurations",
            ListType::Bookmarks => "Save and organize posts",
            ListType::Curations => "Curate collections of content",
        }
    }
    fn alt_text(&self) -> &'static str {
        match self {
            ListType::People => "List of people",
            ListType::Relays => "List of relays",
            ListType::Bookmarks => "List of bookmarks",
            ListType::Curations => "List of curated content",
        }
    }
    /// Only people lists support private members per NIP-51
    fn supports_privacy(&self) -> bool {
        matches!(self, ListType::People)
    }
}
#[derive(Props, Clone, PartialEq)]
pub struct CreateListModalProps {
    pub on_close: EventHandler<()>,
    pub on_created: EventHandler<()>,
}
#[component]
pub fn CreateListModal(props: CreateListModalProps) -> Element {
    let mut list_type = use_signal(|| ListType::People);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut is_private_list = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let on_close = props.on_close;
    let on_created = props.on_created;
    let handle_create = move |_| {
        if *loading.peek() {
            return;
        }
        let list_type_val = *list_type.read();
        let name_val = name.read().clone();
        let desc_val = description.read().clone();
        let is_private = *is_private_list.read();
        if name_val.trim().is_empty() {
            error_msg.set(Some("Please enter a list name".to_string()));
            return;
        }
        loading.set(true);
        error_msg.set(None);
        spawn(async move {
            match create_list(list_type_val, name_val, desc_val, is_private).await {
                Ok(_) => {
                    loading.set(false);
                    on_created.call(());
                }
                Err(e) => {
                    error_msg.set(Some(e));
                    loading.set(false);
                }
            }
        });
    };
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
            onclick: move |_| {
                if !*loading.peek() {
                    props.on_close.call(())
                }
            },
            div {
                class: "bg-background border border-border rounded-lg p-6 max-w-lg mx-4 w-full max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex justify-between items-center mb-6",
                    h2 { class: "text-xl font-bold", "Create New List" }
                    button {
                        class: "text-muted-foreground hover:text-foreground disabled:opacity-50",
                        disabled: *loading.read(),
                        aria_busy: "{loading}",
                        onclick: move |_| {
                            if !*loading.peek() {
                                props.on_close.call(())
                            }
                        },
                        "✕"
                    }
                }
                div { class: "space-y-6",
                    div {
                        label { class: "block text-sm font-medium mb-3", "List Type" }
                        div { class: "grid grid-cols-2 gap-3",
                            for lt in [ListType::People, ListType::Relays, ListType::Bookmarks, ListType::Curations] {
                                {
                                    let lt_for_check = lt;
                                    let lt_for_set = lt;
                                    rsx! {
                                        button {
                                            key: "{lt.label()}",
                                            class: if *list_type.read() == lt_for_check { "p-3 border-2 border-primary rounded-lg text-left bg-primary/5" } else { "p-3 border border-border rounded-lg text-left hover:bg-accent" },
                                            onclick: move |_| {
                                                list_type.set(lt_for_set);
                                                if !lt_for_set.supports_privacy() {
                                                    is_private_list.set(false);
                                                }
                                            },
                                            div { class: "flex items-center gap-2 mb-1",
                                                span { class: "text-xl", "{lt.icon()}" }
                                                span { class: "font-medium", "{lt.label()}" }
                                            }
                                            div { class: "text-xs text-muted-foreground", "{lt.description()}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Name" }
                        input {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "My Awesome List",
                            maxlength: "100",
                            value: "{name}",
                            oninput: move |e| name.set(e.value().clone()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Description (optional)" }
                        textarea {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                            rows: "2",
                            placeholder: "What is this list for?",
                            maxlength: "500",
                            value: "{description}",
                            oninput: move |e| description.set(e.value().clone()),
                        }
                    }
                    if list_type.read().supports_privacy() {
                        div { class: "p-4 bg-muted/50 rounded-lg",
                            div { class: "flex items-start gap-3",
                                input {
                                    r#type: "checkbox",
                                    id: "private-list",
                                    class: "mt-1",
                                    checked: *is_private_list.read(),
                                    onchange: move |e| is_private_list.set(e.checked()),
                                }
                                div {
                                    label {
                                        r#for: "private-list",
                                        class: "font-medium cursor-pointer",
                                        "🔒 Private List"
                                    }
                                    p { class: "text-sm text-muted-foreground mt-1",
                                        "Members will be encrypted with NIP-44. Only you can see who is in this list. "
                                        "You can still add individual members as public or private later."
                                    }
                                }
                            }
                            div { class: "mt-3 p-3 bg-blue-500/10 border border-blue-500/20 rounded text-sm",
                                div { class: "flex items-start gap-2",
                                    span { "ℹ️" }
                                    div {
                                        p { class: "font-medium text-blue-600 dark:text-blue-400",
                                            "How private lists work:"
                                        }
                                        ul { class: "mt-1 text-muted-foreground space-y-1",
                                            li {
                                                "• Private members are encrypted in the event content"
                                            }
                                            li {
                                                "• Public members appear in tags (visible to everyone)"
                                            }
                                            li {
                                                "• You can mix public and private members in one list"
                                            }
                                            li {
                                                "• Other apps that support NIP-51 will decrypt your private members"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(err) = error_msg.read().as_ref() {
                        div { class: "p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-600",
                            "{err}"
                        }
                    }
                    div { class: "flex gap-3 justify-end pt-2",
                        button {
                            class: "px-4 py-2 text-muted-foreground hover:text-foreground",
                            disabled: *loading.read(),
                            onclick: move |_| on_close.call(()),
                            "Cancel"
                        }
                        button {
                            class: "px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 disabled:opacity-50 flex items-center gap-2",
                            disabled: *loading.read(),
                            onclick: handle_create,
                            if *loading.read() {
                                span { class: "animate-spin", "⏳" }
                                "Creating..."
                            } else {
                                span { "{list_type.read().icon()}" }
                                "Create List"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Create a new list
async fn create_list(
    list_type: ListType,
    name: String,
    description: String,
    is_private_default: bool,
) -> Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {}", e))?;
    let name = name.trim();
    if name.is_empty() {
        return Err("List name cannot be empty".to_string());
    }
    if name.chars().count() > 100 {
        return Err("List name too long (max 100 characters)".to_string());
    }
    let unique_id = uuid::Uuid::new_v4().to_string();
    let mut tags = vec![
        Tag::identifier(&unique_id),
        Tag::custom(
            nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("name")),
            vec![name.to_string()],
        ),
        Tag::custom(
            nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("alt")),
            vec![list_type.alt_text().to_string()],
        ),
    ];
    let desc = description.trim();
    if !desc.is_empty() {
        let bounded_desc: String = desc.chars().take(500).collect();
        tags.push(Tag::custom(
            nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("description")),
            vec![bounded_desc],
        ));
    }
    let content = if list_type.supports_privacy() && is_private_default {
        let pubkey = signer
            .get_public_key()
            .await
            .map_err(|e| format!("Failed to get pubkey: {}", e))?;
        signer
            .nip44_encrypt(&pubkey, "[]")
            .await
            .map_err(|e| format!("Failed to encrypt: {}", e))?
    } else {
        String::new()
    };
    let builder = EventBuilder::new(Kind::from_u16(list_type.kind()), content).tags(tags);
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to create list: {}", e))?;
    if output.success.is_empty() {
        return Err("Failed to create list: no relays accepted the event".to_string());
    }
    log::info!("Created new {} list: {}", list_type.label(), name);
    Ok(())
}
