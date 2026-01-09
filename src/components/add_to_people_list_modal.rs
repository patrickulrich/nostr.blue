//! Add to People List Modal
//!
//! Modal for adding a person to an existing people list from the profile page.
//! Supports:
//! - Selecting from existing people lists
//! - Adding as public or private member (NIP-44 encrypted)
//! - Creating a new list inline

use dioxus::prelude::*;

use crate::hooks::{use_user_lists, UserList};
use crate::stores::profiles;
use crate::utils::list_kinds::NAMED_PEOPLE;
use crate::utils::list_encryption::add_person_to_list;

#[derive(Clone, PartialEq)]
enum ModalTab {
    ExistingList,
    CreateNew,
}

#[derive(Props, Clone, PartialEq)]
pub struct AddToPeopleListModalProps {
    /// The pubkey of the person to add
    pub person_pubkey: String,
    /// Handler when modal is closed
    pub on_close: EventHandler<()>,
    /// Handler when person is successfully added
    pub on_added: EventHandler<()>,
}

#[component]
pub fn AddToPeopleListModal(props: AddToPeopleListModalProps) -> Element {
    let mut active_tab = use_signal(|| ModalTab::ExistingList);
    let mut selected_list = use_signal(|| None::<UserList>);
    let mut add_as_private = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut success_msg = use_signal(|| None::<String>);

    // Create list form state
    let mut new_list_name = use_signal(String::new);
    let mut new_list_private = use_signal(|| false);

    // Fetch user's lists
    let (all_lists, lists_loading, lists_error, _) = use_user_lists();

    // Filter to only people lists
    let people_lists = use_memo(move || {
        all_lists.read()
            .iter()
            .filter(|list| list.kind == NAMED_PEOPLE)
            .cloned()
            .collect::<Vec<_>>()
    });

    // Get person's display name
    let person_pubkey = props.person_pubkey.clone();
    let person_name = use_memo(move || {
        profiles::get_profile(&person_pubkey)
            .and_then(|m| m.display_name.clone().or(m.name.clone()))
            .unwrap_or_else(|| {
                if person_pubkey.len() >= 12 {
                    person_pubkey[..12].to_string()
                } else {
                    person_pubkey.clone()
                }
            })
    });

    let on_close = props.on_close;
    let on_added = props.on_added;
    let person_pk = props.person_pubkey.clone();

    // Handle adding to existing list
    let mut handle_add_to_list = move |_| {
        // Guard against concurrent clicks
        if *loading.read() {
            return;
        }
        let list = match selected_list.read().clone() {
            Some(l) => l,
            None => {
                error_msg.set(Some("Please select a list".to_string()));
                return;
            }
        };

        let is_private = *add_as_private.read();
        let pubkey = person_pk.clone();

        loading.set(true);
        error_msg.set(None);
        success_msg.set(None);

        spawn(async move {
            match add_person_to_list(&list.event, &pubkey, is_private).await {
                Ok(_) => {
                    log::info!("Added person to list '{}' (private: {})", list.name, is_private);
                    success_msg.set(Some(format!("Added to \"{}\"", list.name)));
                    loading.set(false);

                    // Close after brief delay to show success
                    #[cfg(target_arch = "wasm32")]
                    {
                        use gloo_timers::future::TimeoutFuture;
                        TimeoutFuture::new(1000).await;
                    }
                    on_added.call(());
                }
                Err(e) => {
                    log::error!("Failed to add to list: {}", e);
                    error_msg.set(Some(e));
                    loading.set(false);
                }
            }
        });
    };

    // Handle creating new list and adding person
    let mut handle_create_and_add = {
        let pubkey = props.person_pubkey.clone();
        move |_| {
            // Guard against concurrent clicks
            if *loading.read() {
                return;
            }
            let name = new_list_name.read().trim().to_string();
            if name.is_empty() {
                error_msg.set(Some("Please enter a list name".to_string()));
                return;
            }

            let is_private = *new_list_private.read();
            let pubkey = pubkey.clone();

            loading.set(true);
            error_msg.set(None);
            success_msg.set(None);

            spawn(async move {
                // First create the list
                match crate::utils::list_encryption::create_people_list(name.clone(), None, is_private).await {
                    Ok(event) => {
                        log::info!("Created new list '{}'", name);

                        // Now add the person to the new list
                        match add_person_to_list(&event, &pubkey, is_private).await {
                            Ok(_) => {
                                log::info!("Added person to new list '{}'", name);
                                success_msg.set(Some(format!("Created \"{}\" and added", name)));
                                loading.set(false);

                                // Close after brief delay
                                #[cfg(target_arch = "wasm32")]
                                {
                                    use gloo_timers::future::TimeoutFuture;
                                    TimeoutFuture::new(1000).await;
                                }
                                on_added.call(());
                            }
                            Err(e) => {
                                error_msg.set(Some(format!("Created list but failed to add: {}", e)));
                                loading.set(false);
                            }
                        }
                    }
                    Err(e) => {
                        error_msg.set(Some(e));
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
                class: "bg-background border border-border rounded-lg p-6 max-w-md mx-4 w-full max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex justify-between items-center mb-4",
                    h2 {
                        class: "text-xl font-bold",
                        "Add to List"
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground text-xl",
                        onclick: move |_| props.on_close.call(()),
                        "×"
                    }
                }

                // Person being added
                div {
                    class: "p-3 bg-muted/50 rounded-lg mb-4",
                    div {
                        class: "flex items-center gap-2",
                        span { class: "text-lg", "👤" }
                        span { class: "font-medium", "{person_name}" }
                    }
                }

                // Tab selector
                div {
                    class: "flex border-b border-border mb-4",
                    button {
                        class: if *active_tab.read() == ModalTab::ExistingList {
                            "flex-1 py-2 text-center font-medium border-b-2 border-primary"
                        } else {
                            "flex-1 py-2 text-center text-muted-foreground hover:text-foreground"
                        },
                        onclick: move |_| active_tab.set(ModalTab::ExistingList),
                        "Existing List"
                    }
                    button {
                        class: if *active_tab.read() == ModalTab::CreateNew {
                            "flex-1 py-2 text-center font-medium border-b-2 border-primary"
                        } else {
                            "flex-1 py-2 text-center text-muted-foreground hover:text-foreground"
                        },
                        onclick: move |_| active_tab.set(ModalTab::CreateNew),
                        "Create New"
                    }
                }

                // Tab content
                match *active_tab.read() {
                    ModalTab::ExistingList => rsx! {
                        div {
                            class: "space-y-4",

                            // List loading
                            if *lists_loading.read() {
                                div {
                                    class: "text-center py-8 text-muted-foreground",
                                    "Loading your lists..."
                                }
                            }
                            // Error loading lists
                            else if let Some(err) = lists_error.read().as_ref() {
                                div {
                                    class: "text-center py-8",
                                    div { class: "text-4xl mb-2", "⚠️" }
                                    p {
                                        class: "text-red-500",
                                        "Failed to load lists: {err}"
                                    }
                                }
                            }
                            // No lists
                            else if people_lists.read().is_empty() {
                                div {
                                    class: "text-center py-8",
                                    div { class: "text-4xl mb-2", "📋" }
                                    p {
                                        class: "text-muted-foreground mb-4",
                                        "You don't have any people lists yet."
                                    }
                                    button {
                                        class: "text-primary hover:underline",
                                        onclick: move |_| active_tab.set(ModalTab::CreateNew),
                                        "Create your first list →"
                                    }
                                }
                            }
                            // List selector
                            else {
                                div {
                                    class: "space-y-2",
                                    label {
                                        class: "block text-sm font-medium",
                                        "Select a list"
                                    }
                                    select {
                                        class: "w-full px-3 py-2 bg-background border border-border rounded-lg",
                                        onchange: move |e| {
                                            let value = e.value();
                                            let list = people_lists.read()
                                                .iter()
                                                .find(|l| l.id == value)
                                                .cloned();
                                            selected_list.set(list);
                                        },
                                        option {
                                            value: "",
                                            "Choose a list..."
                                        }
                                        for list in people_lists.read().iter() {
                                            option {
                                                key: "{list.id}",
                                                value: "{list.id}",
                                                "{list.name}"
                                                if list.has_private_content { " 🔒" } else { "" }
                                            }
                                        }
                                    }
                                }

                                // Privacy toggle
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
                        }
                    },
                    ModalTab::CreateNew => rsx! {
                        div {
                            class: "space-y-4",

                            // List name input
                            div {
                                label {
                                    class: "block text-sm font-medium mb-2",
                                    "List Name"
                                }
                                input {
                                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "e.g., Friends, Work Colleagues",
                                    maxlength: "100",
                                    value: "{new_list_name}",
                                    oninput: move |e| new_list_name.set(e.value().clone()),
                                }
                            }

                            // Privacy toggle
                            div {
                                class: "flex items-start gap-3 p-3 bg-muted/30 rounded-lg",
                                input {
                                    r#type: "checkbox",
                                    id: "new-list-private",
                                    class: "mt-1",
                                    checked: *new_list_private.read(),
                                    onchange: move |e| new_list_private.set(e.checked()),
                                }
                                div {
                                    label {
                                        r#for: "new-list-private",
                                        class: "font-medium cursor-pointer",
                                        "🔒 Make list private"
                                    }
                                    p {
                                        class: "text-sm text-muted-foreground mt-1",
                                        "This person will be added privately"
                                    }
                                }
                            }
                        }
                    }
                }

                // Error message
                if let Some(err) = error_msg.read().as_ref() {
                    div {
                        class: "mt-4 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-600",
                        "{err}"
                    }
                }

                // Success message
                if let Some(msg) = success_msg.read().as_ref() {
                    div {
                        class: "mt-4 p-3 bg-green-500/10 border border-green-500/20 rounded-lg text-green-600",
                        "✓ {msg}"
                    }
                }

                // Action buttons
                div {
                    class: "flex gap-3 justify-end mt-6",
                    button {
                        class: "px-4 py-2 text-muted-foreground hover:text-foreground",
                        disabled: *loading.read(),
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 disabled:opacity-50 flex items-center gap-2",
                        disabled: *loading.read(),
                        onclick: move |_| {
                            if *active_tab.read() == ModalTab::ExistingList {
                                handle_add_to_list(());
                            } else {
                                handle_create_and_add(());
                            }
                        },
                        if *loading.read() {
                            span { class: "animate-spin", "⏳" }
                            "Adding..."
                        } else if *active_tab.read() == ModalTab::CreateNew {
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
