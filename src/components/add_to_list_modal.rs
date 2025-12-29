use dioxus::prelude::*;
use crate::hooks::{use_user_lists, UserList};
use crate::stores::nostr_client;
use crate::utils::list_kinds::{get_item_count, NAMED_PEOPLE, NAMED_CURATIONS};
use crate::utils::list_encryption::add_person_to_list;
use nostr_sdk::{EventBuilder, Kind, Tag, EventId};
use uuid::Uuid;
use gloo_timers::future::TimeoutFuture;

/// Mode for the add to list modal
#[derive(Clone, PartialEq, Debug)]
enum AddMode {
    /// Initial selection - "Add this post" or "Add this person"
    SelectMode,
    /// Adding the post/event to a curation list (kind 30004)
    AddPost,
    /// Adding the author to a people list (kind 30000)
    AddPerson,
}

#[derive(Props, Clone, PartialEq)]
pub struct AddToListModalProps {
    pub event_id: String,
    /// The author's pubkey - needed for "Add to People List" option
    #[props(default = String::new())]
    pub author_pubkey: String,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn AddToListModal(props: AddToListModalProps) -> Element {
    let (lists_signal, lists_loading, lists_error, mut refresh_trigger) = use_user_lists();
    let mut selected_list_id = use_signal(|| None::<String>);
    let mut selected_people_list = use_signal(|| None::<UserList>);
    let mut new_list_name = use_signal(String::new);
    let mut create_new = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut add_mode = use_signal(|| {
        // Skip mode selection if no author pubkey provided
        if props.author_pubkey.is_empty() {
            AddMode::AddPost
        } else {
            AddMode::SelectMode
        }
    });
    let mut add_as_private = use_signal(|| false);

    // Filter for curation lists (kind 30004)
    let curation_lists = use_memo(move || {
        lists_signal
            .read()
            .iter()
            .filter(|list| list.kind == NAMED_CURATIONS)
            .cloned()
            .collect::<Vec<_>>()
    });

    // Filter for people lists (kind 30000)
    let people_lists = use_memo(move || {
        lists_signal
            .read()
            .iter()
            .filter(|list| list.kind == NAMED_PEOPLE)
            .cloned()
            .collect::<Vec<_>>()
    });

    // Clone needed fields before creating the move closure
    let event_id = props.event_id.clone();
    let author_pubkey = props.author_pubkey.clone();
    let on_close = props.on_close;

    // Handle adding post to curation list
    let mut handle_add_post = {
        let event_id = event_id.clone();
        move |_| {
            let event_id = event_id.clone();

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
                        log::info!("Successfully added post to list");
                        success.set(true);
                        loading.set(false);

                        // Refresh the lists to show the update
                        refresh_trigger.with_mut(|val| *val = val.wrapping_add(1));

                        // Auto-close after success
                        spawn(async move {
                            gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
                            on_close.call(());
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to add post to list: {}", e);
                        error_msg.set(Some(format!("Failed: {}", e)));
                        loading.set(false);
                    }
                }
            });
        }
    };

    // Handle adding person to people list
    let mut handle_add_person = {
        let author_pubkey = author_pubkey.clone();
        move |_| {
            let pubkey = author_pubkey.clone();

            loading.set(true);
            error_msg.set(None);

            spawn(async move {
                let result = if *create_new.read() {
                    // Create a new people list with this person
                    let list_name = new_list_name.read().clone();
                    if list_name.is_empty() {
                        error_msg.set(Some("Please enter a list name".to_string()));
                        loading.set(false);
                        return;
                    }

                    let is_private = *add_as_private.read();

                    // Create the list first
                    match crate::utils::list_encryption::create_people_list(
                        list_name.clone(),
                        None,
                        is_private,
                    )
                    .await
                    {
                        Ok(event) => {
                            // Then add the person
                            add_person_to_list(&event, &pubkey, is_private).await
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    // Add to existing people list
                    let list = selected_people_list.read().clone();
                    match list {
                        Some(l) => {
                            let is_private = *add_as_private.read();
                            add_person_to_list(&l.event, &pubkey, is_private).await
                        }
                        None => {
                            error_msg.set(Some("Please select a list".to_string()));
                            loading.set(false);
                            return;
                        }
                    }
                };

                match result {
                    Ok(_) => {
                        log::info!("Successfully added person to list");
                        success.set(true);
                        loading.set(false);

                        // Refresh the lists to show the update
                        refresh_trigger.with_mut(|val| *val = val.wrapping_add(1));

                        // Auto-close after success
                        spawn(async move {
                            gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
                            on_close.call(());
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to add person to list: {}", e);
                        error_msg.set(Some(format!("Failed: {}", e)));
                        loading.set(false);
                    }
                }
            });
        }
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
                        match *add_mode.read() {
                            AddMode::SelectMode => "Add to List",
                            AddMode::AddPost => "Add Post to List",
                            AddMode::AddPerson => "Add Person to List",
                        }
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

                // Mode selection
                if !*success.read() && *add_mode.read() == AddMode::SelectMode {
                    div {
                        class: "space-y-3",
                        p {
                            class: "text-sm text-muted-foreground mb-4",
                            "What would you like to add?"
                        }

                        // Add post button
                        button {
                            class: "w-full p-4 border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3 text-left",
                            onclick: move |_| add_mode.set(AddMode::AddPost),
                            span { class: "text-2xl", "📝" }
                            div {
                                div { class: "font-medium", "Add this post" }
                                div { class: "text-sm text-muted-foreground", "Save to a curation list" }
                            }
                        }

                        // Add person button
                        button {
                            class: "w-full p-4 border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3 text-left",
                            onclick: move |_| add_mode.set(AddMode::AddPerson),
                            span { class: "text-2xl", "👤" }
                            div {
                                div { class: "font-medium", "Add this person" }
                                div { class: "text-sm text-muted-foreground", "Add author to a people list" }
                            }
                        }
                    }
                }

                // Add Post Form
                if !*success.read() && *add_mode.read() == AddMode::AddPost {
                    div {
                        class: "space-y-4",

                        // Back button (if mode selection was shown)
                        if !props.author_pubkey.is_empty() {
                            button {
                                class: "text-sm text-muted-foreground hover:text-foreground flex items-center gap-1 mb-2",
                                onclick: move |_| {
                                    add_mode.set(AddMode::SelectMode);
                                    create_new.set(false);
                                    error_msg.set(None);
                                },
                                "← Back"
                            }
                        }

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

                                if *lists_loading.read() {
                                    div {
                                        class: "text-sm text-muted-foreground italic py-2",
                                        "Loading lists..."
                                    }
                                } else if let Some(err) = lists_error.read().as_ref() {
                                    div {
                                        class: "text-sm text-red-500 py-2",
                                        "Error loading lists: {err}"
                                    }
                                } else if curation_lists.read().is_empty() {
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
                                onclick: move |_| handle_add_post(()),
                                if *loading.read() {
                                    "Adding..."
                                } else {
                                    "Add to List"
                                }
                            }
                        }
                    }
                }

                // Add Person Form
                if !*success.read() && *add_mode.read() == AddMode::AddPerson {
                    div {
                        class: "space-y-4",

                        // Back button
                        button {
                            class: "text-sm text-muted-foreground hover:text-foreground flex items-center gap-1 mb-2",
                            onclick: move |_| {
                                add_mode.set(AddMode::SelectMode);
                                create_new.set(false);
                                error_msg.set(None);
                            },
                            "← Back"
                        }

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

                        // Existing people list selector
                        if !*create_new.read() {
                            div {
                                label {
                                    class: "block text-sm font-medium mb-2",
                                    "Select a people list"
                                }

                                if *lists_loading.read() {
                                    div {
                                        class: "text-sm text-muted-foreground italic py-2",
                                        "Loading lists..."
                                    }
                                } else if let Some(err) = lists_error.read().as_ref() {
                                    div {
                                        class: "text-sm text-red-500 py-2",
                                        "Error loading lists: {err}"
                                    }
                                } else if people_lists.read().is_empty() {
                                    div {
                                        class: "text-sm text-muted-foreground italic py-2",
                                        "You don't have any people lists yet. Create one below!"
                                    }
                                } else {
                                    select {
                                        class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                        onchange: move |e| {
                                            let value = e.value();
                                            let list = people_lists.read()
                                                .iter()
                                                .find(|l| l.id == value)
                                                .cloned();
                                            selected_people_list.set(list);
                                        },

                                        option {
                                            value: "",
                                            "Select a list..."
                                        }

                                        for list in people_lists.read().iter() {
                                            option {
                                                value: "{list.id}",
                                                "{list.name}"
                                                if list.has_private_content { " 🔒" } else { "" }
                                            }
                                        }
                                    }
                                }
                            }

                            // Privacy toggle for existing lists
                            div {
                                class: "flex items-start gap-3 p-3 bg-muted/30 rounded-lg",
                                input {
                                    r#type: "checkbox",
                                    id: "add-private",
                                    class: "mt-1",
                                    checked: *add_as_private.read(),
                                    onchange: move |e| add_as_private.set(e.checked()),
                                }
                                div {
                                    label {
                                        r#for: "add-private",
                                        class: "font-medium cursor-pointer",
                                        "🔒 Add as private member"
                                    }
                                    p {
                                        class: "text-sm text-muted-foreground mt-1",
                                        "Only you will be able to see this person in the list"
                                    }
                                }
                            }
                        }

                        // New people list name input
                        if *create_new.read() {
                            div {
                                label {
                                    class: "block text-sm font-medium mb-2",
                                    "List name"
                                }
                                input {
                                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "e.g., Friends, Work Colleagues...",
                                    value: "{new_list_name}",
                                    oninput: move |e| new_list_name.set(e.value().clone()),
                                }
                            }

                            // Privacy toggle for new list
                            div {
                                class: "flex items-start gap-3 p-3 bg-muted/30 rounded-lg",
                                input {
                                    r#type: "checkbox",
                                    id: "new-list-private",
                                    class: "mt-1",
                                    checked: *add_as_private.read(),
                                    onchange: move |e| add_as_private.set(e.checked()),
                                }
                                div {
                                    label {
                                        r#for: "new-list-private",
                                        class: "font-medium cursor-pointer",
                                        "🔒 Make list private"
                                    }
                                    p {
                                        class: "text-sm text-muted-foreground mt-1",
                                        "All members will be encrypted (NIP-44)"
                                    }
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
                                onclick: move |_| handle_add_person(()),
                                if *loading.read() {
                                    "Adding..."
                                } else if *create_new.read() {
                                    "Create & Add"
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

    // Validate list name
    const MAX_LIST_NAME_LENGTH: usize = 100;
    let name = name.trim();

    if name.is_empty() {
        return Err("List name cannot be empty".to_string());
    }

    if name.len() > MAX_LIST_NAME_LENGTH {
        return Err(format!("List name cannot exceed {} characters", MAX_LIST_NAME_LENGTH));
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
        Tag::custom(nostr_sdk::TagKind::Name, vec![name.to_string()]),  // Human-readable name
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

    // Fetch the existing list event with timeout
    let list_event = {
        tokio::select! {
            result = client.database().event_by_id(&list_id) => {
                result
                    .map_err(|e| format!("Failed to fetch list: {}", e))?
                    .ok_or("List not found")?
            }
            _ = TimeoutFuture::new(5_000) => {
                return Err("Database fetch timeout (5s)".to_string());
            }
        }
    };

    // Preserve the existing content before consuming the event
    let existing_content = list_event.content.clone();

    // Extract existing tags and add new event
    let mut tags: Vec<Tag> = list_event.tags.into_iter().collect();

    // Check if event is already in the list (use normalized lowercase hex for comparison)
    let normalized_event_id = target_event_id.to_hex();
    let already_exists = tags.iter().any(|tag| {
        tag.kind() == nostr_sdk::TagKind::e() &&
        tag.content().map(|c| c.eq_ignore_ascii_case(&normalized_event_id)).unwrap_or(false)
    });

    if already_exists {
        return Err("Event is already in this list".to_string());
    }

    // Add new event tag
    tags.push(Tag::event(target_event_id));

    // Publish updated list with preserved content
    let builder = EventBuilder::new(Kind::from(30004), existing_content).tags(tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to update list: {}", e))?;

    log::info!("Added event to existing list");
    Ok(())
}
