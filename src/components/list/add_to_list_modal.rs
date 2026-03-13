use crate::hooks::{use_user_lists, UserList};
use crate::stores::nostr_client;
use crate::utils::list_encryption::add_person_to_list;
use crate::utils::list_kinds::{get_item_count, NAMED_CURATIONS, NAMED_PEOPLE};
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, EventId, Kind, Tag};
use uuid::Uuid;
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
    let existing_lists_supported = cfg!(feature = "native");
    let create_new_default = !existing_lists_supported;
    let mut selected_list_id = use_signal(|| None::<String>);
    let mut selected_people_list = use_signal(|| None::<UserList>);
    let mut new_list_name = use_signal(String::new);
    let mut create_new = use_signal(move || create_new_default);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut add_mode = use_signal(|| {
        if props.author_pubkey.is_empty() {
            AddMode::AddPost
        } else {
            AddMode::SelectMode
        }
    });
    let mut add_as_private = use_signal(|| false);
    let curation_lists = use_memo(move || {
        lists_signal
            .read()
            .iter()
            .filter(|list| list.kind == NAMED_CURATIONS)
            .cloned()
            .collect::<Vec<_>>()
    });
    let people_lists = use_memo(move || {
        lists_signal
            .read()
            .iter()
            .filter(|list| list.kind == NAMED_PEOPLE)
            .cloned()
            .collect::<Vec<_>>()
    });
    use_effect(move || {
        let mode = add_mode.read().clone();
        let should_create_new = match mode {
            AddMode::SelectMode => create_new_default,
            AddMode::AddPost => !existing_lists_supported || curation_lists.read().is_empty(),
            AddMode::AddPerson => people_lists.read().is_empty(),
        };
        create_new.set(should_create_new);
        selected_list_id.set(None);
        selected_people_list.set(None);
        new_list_name.set(String::new());
        add_as_private.set(false);
        error_msg.set(None);
    });
    let event_id = props.event_id.clone();
    let author_pubkey = props.author_pubkey.clone();
    let on_close = props.on_close;
    let mut handle_add_post = {
        let event_id = event_id.clone();
        move |_| {
            let event_id = event_id.clone();
            loading.set(true);
            error_msg.set(None);
            spawn(async move {
                let result = if *create_new.read() {
                    let list_name = new_list_name.read().clone();
                    if list_name.is_empty() {
                        error_msg.set(Some("Please enter a list name".to_string()));
                        loading.set(false);
                        return;
                    }
                    create_new_curation_list(list_name, event_id).await
                } else {
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
                        refresh_trigger.with_mut(|val| *val = val.wrapping_add(1));
                        spawn(async move {
                            crate::platform::timer::sleep(std::time::Duration::from_secs(2)).await;
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
    let mut handle_add_person = {
        let author_pubkey = author_pubkey.clone();
        move |_| {
            let pubkey = author_pubkey.clone();
            loading.set(true);
            error_msg.set(None);
            spawn(async move {
                let result = if *create_new.read() {
                    let list_name = new_list_name.read().clone();
                    if list_name.is_empty() {
                        error_msg.set(Some("Please enter a list name".to_string()));
                        loading.set(false);
                        return;
                    }
                    let is_private = *add_as_private.read();
                    match crate::utils::list_encryption::create_people_list(
                        list_name.clone(),
                        None,
                        is_private,
                    )
                    .await
                    {
                        Ok(event) => match add_person_to_list(&event, &pubkey, is_private).await {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                refresh_trigger.with_mut(|val| *val = val.wrapping_add(1));
                                Err(format!("List created but adding person failed: {}", e))
                            }
                        },
                        Err(e) => Err(e),
                    }
                } else {
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
                        refresh_trigger.with_mut(|val| *val = val.wrapping_add(1));
                        spawn(async move {
                            crate::platform::timer::sleep(std::time::Duration::from_secs(2)).await;
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
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "bg-background border border-border rounded-lg p-6 max-w-md mx-4 w-full",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex justify-between items-center mb-4",
                    h2 { class: "text-xl font-bold",
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
                if *success.read() {
                    div { class: "mb-4 p-3 bg-green-500/10 border border-green-500/20 rounded-lg text-green-600",
                        "✓ Successfully added to list"
                    }
                }
                if !*success.read() && *add_mode.read() == AddMode::SelectMode {
                    div { class: "space-y-3",
                        p { class: "text-sm text-muted-foreground mb-4",
                            "What would you like to add?"
                        }
                        button {
                            class: "w-full p-4 border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3 text-left",
                            onclick: move |_| add_mode.set(AddMode::AddPost),
                            span { class: "text-2xl", "📝" }
                            div {
                                div { class: "font-medium", "Add this post" }
                                div { class: "text-sm text-muted-foreground", "Save to a curation list" }
                            }
                        }
                        button {
                            class: "w-full p-4 border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3 text-left",
                            onclick: move |_| add_mode.set(AddMode::AddPerson),
                            span { class: "text-2xl", "👤" }
                            div {
                                div { class: "font-medium", "Add this person" }
                                div { class: "text-sm text-muted-foreground",
                                    "Add author to a people list"
                                }
                            }
                        }
                    }
                }
                if !*success.read() && *add_mode.read() == AddMode::AddPost {
                    div { class: "space-y-4",
                        if !props.author_pubkey.is_empty() {
                            button {
                                class: "text-sm text-muted-foreground hover:text-foreground flex items-center gap-1 mb-2",
                                onclick: move |_| {
                                    add_mode.set(AddMode::SelectMode);
                                },
                                "← Back"
                            }
                        }
                        div { class: "flex gap-2 border-b border-border pb-2",
                            if existing_lists_supported {
                                button {
                                    class: if !*create_new.read() { "px-3 py-1 text-sm font-medium border-b-2 border-primary" } else { "px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground" },
                                    onclick: move |_| create_new.set(false),
                                    "Existing List"
                                }
                            }
                            button {
                                class: if *create_new.read() { "px-3 py-1 text-sm font-medium border-b-2 border-primary" } else { "px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground" },
                                onclick: move |_| create_new.set(true),
                                "Create New"
                            }
                        }
                        if !existing_lists_supported {
                            div { class: "text-sm text-muted-foreground",
                                "Existing curation lists can only be updated on native builds right now. Create a new list instead."
                            }
                        }
                        if existing_lists_supported && !*create_new.read() {
                            div {
                                label { class: "block text-sm font-medium mb-2", "Select a curation list" }
                                if *lists_loading.read() {
                                    div { class: "text-sm text-muted-foreground italic py-2",
                                        "Loading lists..."
                                    }
                                } else if let Some(err) = lists_error.read().as_ref() {
                                    div { class: "text-sm text-red-500 py-2",
                                        "Error loading lists: {err}"
                                    }
                                } else if curation_lists.read().is_empty() {
                                    div { class: "text-sm text-muted-foreground italic py-2",
                                        "You don't have any curation lists yet. Create one below!"
                                    }
                                } else {
                                    select {
                                        class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                        onchange: move |e| {
                                            let value = e.value();
                                            if value.is_empty() {
                                                selected_list_id.set(None);
                                            } else {
                                                selected_list_id.set(Some(value.clone()));
                                            }
                                        },
                                        option { value: "", "Select a list..." }
                                        for list in curation_lists.read().iter() {
                                            option { value: "{list.id}",
                                                "{list.name} ({get_item_count(&list.tags)} items)"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if *create_new.read() {
                            div {
                                label { class: "block text-sm font-medium mb-2", "List name" }
                                input {
                                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "e.g., Funny Posts, Interesting Articles...",
                                    value: "{new_list_name}",
                                    oninput: move |e| new_list_name.set(e.value().clone()),
                                }
                                p { class: "text-xs text-muted-foreground mt-1",
                                    "Create a new curation list to organize your favorite posts"
                                }
                            }
                        }
                        if let Some(err) = error_msg.read().as_ref() {
                            div { class: "text-red-500 text-sm", "{err}" }
                        }
                        div { class: "flex gap-2 justify-end pt-2",
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
                if !*success.read() && *add_mode.read() == AddMode::AddPerson {
                    div { class: "space-y-4",
                        button {
                            class: "text-sm text-muted-foreground hover:text-foreground flex items-center gap-1 mb-2",
                            onclick: move |_| {
                                add_mode.set(AddMode::SelectMode);
                            },
                            "← Back"
                        }
                        div { class: "flex gap-2 border-b border-border pb-2",
                            button {
                                class: if !*create_new.read() { "px-3 py-1 text-sm font-medium border-b-2 border-primary" } else { "px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground" },
                                onclick: move |_| create_new.set(false),
                                "Existing List"
                            }
                            button {
                                class: if *create_new.read() { "px-3 py-1 text-sm font-medium border-b-2 border-primary" } else { "px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground" },
                                onclick: move |_| create_new.set(true),
                                "Create New"
                            }
                        }
                        if !*create_new.read() {
                            div {
                                label { class: "block text-sm font-medium mb-2", "Select a people list" }
                                if *lists_loading.read() {
                                    div { class: "text-sm text-muted-foreground italic py-2",
                                        "Loading lists..."
                                    }
                                } else if let Some(err) = lists_error.read().as_ref() {
                                    div { class: "text-sm text-red-500 py-2",
                                        "Error loading lists: {err}"
                                    }
                                } else if people_lists.read().is_empty() {
                                    div { class: "text-sm text-muted-foreground italic py-2",
                                        "You don't have any people lists yet. Create one below!"
                                    }
                                } else {
                                    select {
                                        class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                        onchange: move |e| {
                                            let value = e.value();
                                            let list = people_lists.read().iter().find(|l| l.id == value).cloned();
                                            selected_people_list.set(list);
                                        },
                                        option { value: "", "Select a list..." }
                                        for list in people_lists.read().iter() {
                                            option { value: "{list.id}",
                                                "{list.name}"
                                                if list.has_private_content {
                                                    " 🔒"
                                                } else {
                                                    ""
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "flex items-start gap-3 p-3 bg-muted/30 rounded-lg",
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
                                    p { class: "text-sm text-muted-foreground mt-1",
                                        "Only you will be able to see this person in the list"
                                    }
                                }
                            }
                        }
                        if *create_new.read() {
                            div {
                                label { class: "block text-sm font-medium mb-2", "List name" }
                                input {
                                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "e.g., Friends, Work Colleagues...",
                                    value: "{new_list_name}",
                                    oninput: move |e| new_list_name.set(e.value().clone()),
                                }
                            }
                            div { class: "flex items-start gap-3 p-3 bg-muted/30 rounded-lg",
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
                                    p { class: "text-sm text-muted-foreground mt-1",
                                        "All members will be encrypted (NIP-44)"
                                    }
                                }
                            }
                        }
                        if let Some(err) = error_msg.read().as_ref() {
                            div { class: "text-red-500 text-sm", "{err}" }
                        }
                        div { class: "flex gap-2 justify-end pt-2",
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
async fn create_new_curation_list(name: String, event_id: String) -> Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    const MAX_LIST_NAME_LENGTH: usize = 100;
    let name = name.trim();
    if name.is_empty() {
        return Err("List name cannot be empty".to_string());
    }
    if name.len() > MAX_LIST_NAME_LENGTH {
        return Err(format!(
            "List name cannot exceed {} characters",
            MAX_LIST_NAME_LENGTH
        ));
    }
    let target_event_id =
        EventId::from_hex(&event_id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let unique_id = Uuid::new_v4().to_string();
    let tags = vec![
        Tag::identifier(&unique_id),
        Tag::custom(nostr_sdk::TagKind::Name, vec![name.to_string()]),
        Tag::event(target_event_id),
    ];
    let builder = EventBuilder::new(Kind::from(30004), "").tags(tags);
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to create list: {}", e))?;
    log::info!("Created new curation list: {}", name);
    Ok(())
}
#[cfg(feature = "native")]
async fn add_to_existing_list(list_event_id: String, event_id: String) -> Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let target_event_id =
        EventId::from_hex(&event_id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let list_id =
        EventId::from_hex(&list_event_id).map_err(|e| format!("Invalid list ID: {}", e))?;
    let list_event = {
        tokio::select! {
            result = client.database().event_by_id(& list_id) => { result.map_err(| e |
            format!("Failed to fetch list: {}", e)) ? .ok_or("List not found") ? } _ =
            crate::platform::timer::sleep_ms(5_000) => { return Err("Database fetch timeout (5s)"
            .to_string()); }
        }
    };
    let existing_content = list_event.content.clone();
    let mut tags: Vec<Tag> = list_event.tags.into_iter().collect();
    let normalized_event_id = target_event_id.to_hex();
    let already_exists = tags.iter().any(|tag| {
        tag.kind() == nostr_sdk::TagKind::e()
            && tag
                .content()
                .map(|c| c.eq_ignore_ascii_case(&normalized_event_id))
                .unwrap_or(false)
    });
    if already_exists {
        return Err("Event is already in this list".to_string());
    }
    tags.push(Tag::event(target_event_id));
    let builder = EventBuilder::new(Kind::from(30004), existing_content).tags(tags);
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to update list: {}", e))?;
    log::info!("Added event to existing list");
    Ok(())
}

#[cfg(not(feature = "native"))]
async fn add_to_existing_list(_list_event_id: String, _event_id: String) -> Result<(), String> {
    Err("Adding to existing lists is not supported on web".to_string())
}
