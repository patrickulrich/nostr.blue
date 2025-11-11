use dioxus::prelude::*;
use crate::hooks::use_lists;
use crate::stores::nostr_client;
use crate::utils::list_kinds::get_item_count;
use nostr_sdk::{EventBuilder, Kind, Tag, EventId};
use uuid::Uuid;

#[derive(Props, Clone, PartialEq)]
pub struct AddToListModalProps {
    pub event_id: String,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn AddToListModal(props: AddToListModalProps) -> Element {
    let user_lists = use_lists::use_user_lists();
    let mut selected_list_id = use_signal(|| None::<String>);
    let mut new_list_name = use_signal(|| String::new());
    let mut create_new = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);

    // Filter for curation lists (kind 30004)
    let curation_lists = use_memo(move || {
        user_lists
            .0
            .read()
            .iter()
            .filter(|list| list.kind == 30004)
            .cloned()
            .collect::<Vec<_>>()
    });

    // Clone needed fields before creating the move closure
    let event_id = props.event_id.clone();
    let on_close = props.on_close.clone();

    let handle_add_to_list = move |_| {
        let event_id = event_id.clone();
        let on_close = on_close.clone();

        loading.set(true);
        error_msg.set(None);

        spawn(async move {
            let result = if *create_new.read() {
                // Create a new curation list with this event
                let list_name = new_list_name.read().clone();
                if list_name.is_empty() {
                    error_msg.set(Some("Please enter a list name".to_string()));
                    loading.set(false);
                    return;
                }

                create_new_curation_list(list_name, event_id).await
            } else {
                // Add to existing list
                let list_id = selected_list_id.read().clone();
                match list_id {
                    Some(id) => add_to_existing_list(id, event_id).await,
                    None => {
                        error_msg.set(Some("Please select a list".to_string()));
                        loading.set(false);
                        return;
                    }
                }
            };

            match result {
                Ok(_) => {
                    log::info!("Successfully added to list");
                    success.set(true);
                    loading.set(false);

                    // Auto-close after success
                    spawn(async move {
                        gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
                        on_close.call(());
                    });
                }
                Err(e) => {
                    log::error!("Failed to add to list: {}", e);
                    error_msg.set(Some(format!("Failed: {}", e)));
                    loading.set(false);
                }
            }
        });
    };

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
            onclick: move |_| props.on_close.call(()),

            // Modal content
            div {
                class: "bg-background border border-border rounded-lg p-6 max-w-md mx-4 w-full",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex justify-between items-center mb-4",
                    h2 {
                        class: "text-xl font-bold",
                        "Add to List"
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground",
                        onclick: move |_| props.on_close.call(()),
                        "✕"
                    }
                }

                // Success message
                if *success.read() {
                    div {
                        class: "mb-4 p-3 bg-green-500/10 border border-green-500/20 rounded-lg text-green-600",
                        "✓ Successfully added to list"
                    }
                }

                // Form
                if !*success.read() {
                    div {
                        class: "space-y-4",

                        // Toggle between existing and new list
                        div {
                            class: "flex gap-2 border-b border-border pb-2",
                            button {
                                class: if !*create_new.read() {
                                    "px-3 py-1 text-sm font-medium border-b-2 border-primary"
                                } else {
                                    "px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground"
                                },
                                onclick: move |_| create_new.set(false),
                                "Existing List"
                            }
                            button {
                                class: if *create_new.read() {
                                    "px-3 py-1 text-sm font-medium border-b-2 border-primary"
                                } else {
                                    "px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground"
                                },
                                onclick: move |_| create_new.set(true),
                                "Create New"
                            }
                        }

                        // Existing list selector
                        if !*create_new.read() {
                            div {
                                label {
                                    class: "block text-sm font-medium mb-2",
                                    "Select a curation list"
                                }

                                if curation_lists.read().is_empty() {
                                    div {
                                        class: "text-sm text-muted-foreground italic py-2",
                                        "You don't have any curation lists yet. Create one below!"
                                    }
                                } else {
                                    select {
                                        class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                        onchange: move |e| selected_list_id.set(Some(e.value().clone())),

                                        option {
                                            value: "",
                                            "Select a list..."
                                        }

                                        for list in curation_lists.read().iter() {
                                            option {
                                                value: "{list.id}",
                                                "{list.name} ({get_item_count(&list.tags)} items)"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // New list name input
                        if *create_new.read() {
                            div {
                                label {
                                    class: "block text-sm font-medium mb-2",
                                    "List name"
                                }
                                input {
                                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "e.g., Funny Posts, Interesting Articles...",
                                    value: "{new_list_name}",
                                    oninput: move |e| new_list_name.set(e.value().clone()),
                                }
                                p {
                                    class: "text-xs text-muted-foreground mt-1",
                                    "Create a new curation list to organize your favorite posts"
                                }
                            }
                        }

                        // Error message
                        if let Some(err) = error_msg.read().as_ref() {
                            div {
                                class: "text-red-500 text-sm",
                                "{err}"
                            }
                        }

                        // Action buttons
                        div {
                            class: "flex gap-2 justify-end pt-2",
                            button {
                                class: "px-4 py-2 text-sm text-muted-foreground hover:text-foreground",
                                disabled: *loading.read(),
                                onclick: move |_| props.on_close.call(()),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 text-sm bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *loading.read(),
                                onclick: handle_add_to_list,
                                if *loading.read() {
                                    "Adding..."
                                } else {
                                    "Add to List"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// Helper function to create a new curation list
async fn create_new_curation_list(name: String, event_id: String) -> Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Parse event ID
    let target_event_id = EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Generate a unique identifier for the d tag (independent of the display name)
    // This prevents name collisions from overwriting existing lists
    let unique_id = Uuid::new_v4().to_string();

    // Create curation list (kind 30004) with this event
    let tags = vec![
        Tag::identifier(&unique_id),  // Use UUID for d tag to prevent collisions
        Tag::custom(nostr_sdk::TagKind::Name, vec![name.clone()]),  // Human-readable name
        Tag::event(target_event_id),
    ];

    let builder = EventBuilder::new(Kind::from(30004), "").tags(tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to create list: {}", e))?;

    log::info!("Created new curation list: {}", name);
    Ok(())
}

// Helper function to add to an existing list
async fn add_to_existing_list(list_event_id: String, event_id: String) -> Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Parse IDs
    let target_event_id = EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let list_id = EventId::from_hex(&list_event_id)
        .map_err(|e| format!("Invalid list ID: {}", e))?;

    // Fetch the existing list event
    let list_event = client.database().event_by_id(&list_id).await
        .map_err(|e| format!("Failed to fetch list: {}", e))?
        .ok_or("List not found")?;

    // Extract existing tags and add new event
    let mut tags: Vec<Tag> = list_event.tags.into_iter().collect();

    // Check if event is already in the list
    let already_exists = tags.iter().any(|tag| {
        tag.kind() == nostr_sdk::TagKind::e() &&
        tag.content().map(|c| c == event_id).unwrap_or(false)
    });

    if already_exists {
        return Err("Event is already in this list".to_string());
    }

    // Add new event tag
    tags.push(Tag::event(target_event_id));

    // Publish updated list
    let builder = EventBuilder::new(Kind::from(30004), "").tags(tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to update list: {}", e))?;

    log::info!("Added event to existing list");
    Ok(())
}
