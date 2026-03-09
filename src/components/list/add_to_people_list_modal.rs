//! Add to People List Modal
//!
//! Modal for adding a person to an existing people list from the profile page.
//! Supports:
//! - Selecting from existing people lists
//! - Adding as public or private member (NIP-44 encrypted)
//! - Creating a new list inline
use crate::hooks::{use_user_lists, UserList};
use crate::stores::profiles;
use crate::utils::list_encryption::add_person_to_list;
use crate::utils::list_kinds::NAMED_PEOPLE;
use dioxus::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
/// Global counter for generating unique modal IDs
static MODAL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
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
    let modal_id = use_signal(|| MODAL_ID_COUNTER.fetch_add(1, Ordering::Relaxed));
    let title_id = format!("add-to-list-modal-title-{}", modal_id());
    let select_id = format!("add-to-list-select-{}", modal_id());
    let mut active_tab = use_signal(|| ModalTab::ExistingList);
    let mut selected_list = use_signal(|| None::<UserList>);
    let mut add_as_private = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut success_msg = use_signal(|| None::<String>);
    let mut new_list_name = use_signal(String::new);
    let mut new_list_private = use_signal(|| false);
    let (all_lists, lists_loading, lists_error, _) = use_user_lists();
    let people_lists = use_memo(move || {
        all_lists
            .read()
            .iter()
            .filter(|list| list.kind == NAMED_PEOPLE)
            .cloned()
            .collect::<Vec<_>>()
    });
    let person_pubkey = props.person_pubkey.clone();
    let person_pubkey_for_effect = props.person_pubkey.clone();
    let mut person_metadata = use_signal(move || profiles::get_profile(&person_pubkey));
    let mut profile_fetch_version = use_signal(|| 0u32);
    use_effect(use_reactive((&person_pubkey_for_effect,), move |(pk,)| {
        let version = profile_fetch_version.with_mut(|v| {
            *v = v.wrapping_add(1);
            *v
        });
        person_metadata.set(profiles::get_profile(&pk));
        spawn(async move {
            if profiles::fetch_profile(pk.clone()).await.is_ok()
                && *profile_fetch_version.peek() == version
            {
                person_metadata.set(profiles::get_profile(&pk));
            }
        });
    }));
    let person_pubkey_for_display = props.person_pubkey.clone();
    let person_name = use_memo(move || {
        person_metadata
            .read()
            .as_ref()
            .and_then(|m| m.display_name.clone().or(m.name.clone()))
            .unwrap_or_else(|| {
                if person_pubkey_for_display.chars().count() >= 12 {
                    person_pubkey_for_display
                        .chars()
                        .take(12)
                        .collect::<String>()
                } else {
                    person_pubkey_for_display.clone()
                }
            })
    });
    let on_close = props.on_close;
    let on_added = props.on_added;
    let person_pk = props.person_pubkey.clone();
    let mut handle_add_to_list = move |_| {
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
                    log::debug!("Added person to list (private: {})", is_private);
                    success_msg.set(Some(format!("Added to \"{}\"", list.name)));
                    crate::platform::timer::sleep_ms(1000).await;
                    loading.set(false);
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
    let mut handle_create_and_add = {
        let pubkey = props.person_pubkey.clone();
        move |_| {
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
                match crate::utils::list_encryption::create_people_list(
                    name.clone(),
                    None,
                    is_private,
                )
                .await
                {
                    Ok(event) => {
                        log::debug!("Created new people list");
                        match add_person_to_list(&event, &pubkey, is_private).await {
                            Ok(_) => {
                                log::debug!("Added person to new list");
                                success_msg.set(Some(format!("Created \"{}\" and added", name)));
                                crate::platform::timer::sleep_ms(1000).await;
                                loading.set(false);
                                on_added.call(());
                            }
                            Err(e) => {
                                error_msg
                                    .set(
                                        Some(
                                            format!(
                                                "Created \"{}\" but failed to add person: {} — list created, you can add the person later",
                                                name,
                                                e,
                                            ),
                                        ),
                                    );
                                loading.set(false);
                                // Note: on_added is NOT called here because adding the person failed
                                // The list was created but the modal should stay open to show the error
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
            tabindex: "-1",
            onclick: move |_| {
                if !*loading.read() {
                    props.on_close.call(());
                }
            },
            onkeydown: move |e| {
                if e.key() == Key::Escape && !*loading.read() {
                    props.on_close.call(());
                }
            },
            div {
                class: "bg-background border border-border rounded-lg p-6 max-w-md mx-4 w-full max-h-[90vh] overflow-y-auto",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "{title_id}",
                tabindex: "-1",
                onclick: move |e| e.stop_propagation(),
                onmounted: move |evt| {
                    #[cfg(feature = "web")]
                    {
                        let element = evt.data();
                        if let Some(html_element) = element.downcast::<web_sys::HtmlElement>() {
                            let _ = html_element.focus();
                        }
                    }
                    #[cfg(not(feature = "web"))]
                    let _ = evt;
                },
                div { class: "flex justify-between items-center mb-4",
                    h2 { class: "text-xl font-bold", id: "{title_id}", "Add to List" }
                    button {
                        class: "text-muted-foreground hover:text-foreground text-xl",
                        aria_label: "Close dialog",
                        onclick: move |_| {
                            if !*loading.read() {
                                props.on_close.call(());
                            }
                        },
                        "×"
                    }
                }
                div { class: "p-3 bg-muted/50 rounded-lg mb-4",
                    div { class: "flex items-center gap-2",
                        span { class: "text-lg", "👤" }
                        span { class: "font-medium", "{person_name}" }
                    }
                }
                div { class: "flex border-b border-border mb-4",
                    button {
                        class: if *active_tab.read() == ModalTab::ExistingList { "flex-1 py-2 text-center font-medium border-b-2 border-primary" } else { "flex-1 py-2 text-center text-muted-foreground hover:text-foreground" },
                        onclick: move |_| active_tab.set(ModalTab::ExistingList),
                        "Existing List"
                    }
                    button {
                        class: if *active_tab.read() == ModalTab::CreateNew { "flex-1 py-2 text-center font-medium border-b-2 border-primary" } else { "flex-1 py-2 text-center text-muted-foreground hover:text-foreground" },
                        onclick: move |_| active_tab.set(ModalTab::CreateNew),
                        "Create New"
                    }
                }
                match *active_tab.read() {
                    ModalTab::ExistingList => rsx! {
                        div { class: "space-y-4",
                            if *lists_loading.read() {
                                div { class: "text-center py-8 text-muted-foreground", "Loading your lists..." }
                            } else if let Some(err) = lists_error.read().as_ref() {
                                div { class: "text-center py-8",
                                    div { class: "text-4xl mb-2", "⚠️" }
                                    p { class: "text-red-500", "Failed to load lists: {err}" }
                                }
                            } else if people_lists.read().is_empty() {
                                div { class: "text-center py-8",
                                    div { class: "text-4xl mb-2", "📋" }
                                    p { class: "text-muted-foreground mb-4", "You don't have any people lists yet." }
                                    button {
                                        class: "text-primary hover:underline",
                                        onclick: move |_| active_tab.set(ModalTab::CreateNew),
                                        "Create your first list →"
                                    }
                                }
                            } else {
                                div { class: "space-y-2",
                                    label { class: "block text-sm font-medium", r#for: "{select_id}", "Select a list" }
                                    select {
                                        class: "w-full px-3 py-2 bg-background border border-border rounded-lg",
                                        id: "{select_id}",
                                        value: "{selected_list.read().as_ref().map(|l| l.id.clone()).unwrap_or_default()}",
                                        onchange: move |e| {
                                            let value = e.value();
                                            let list = if value.is_empty() {
                                                None
                                            } else {
                                                people_lists.read().iter().find(|l| l.id == value).cloned()
                                            };
                                            selected_list.set(list);
                                        },
                                        option { value: "", "Choose a list..." }
                                        for list in people_lists.read().iter() {
                                            option { key: "{list.id}", value: "{list.id}",
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
                                div { class: "flex items-start gap-3 p-3 bg-muted/30 rounded-lg",
                                    input {
                                        r#type: "checkbox",
                                        id: "add-private",
                                        class: "mt-1",
                                        checked: *add_as_private.read(),
                                        onchange: move |e| add_as_private.set(e.checked()),
                                    }
                                    div {
                                        label { r#for: "add-private", class: "font-medium cursor-pointer",
                                            "🔒 Add as private member"
                                        }
                                        p { class: "text-sm text-muted-foreground mt-1",
                                            "Only you will be able to see this person in the list"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    ModalTab::CreateNew => rsx! {
                        div { class: "space-y-4",
                            div {
                                label { class: "block text-sm font-medium mb-2", "List Name" }
                                input {
                                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "e.g., Friends, Work Colleagues",
                                    maxlength: "100",
                                    value: "{new_list_name}",
                                    oninput: move |e| new_list_name.set(e.value().clone()),
                                }
                            }
                            div { class: "flex items-start gap-3 p-3 bg-muted/30 rounded-lg",
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
                                    p { class: "text-sm text-muted-foreground mt-1", "This person will be added privately" }
                                }
                            }
                        }
                    },
                }
                if let Some(err) = error_msg.read().as_ref() {
                    div { class: "mt-4 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-600",
                        "{err}"
                    }
                }
                if let Some(msg) = success_msg.read().as_ref() {
                    div { class: "mt-4 p-3 bg-green-500/10 border border-green-500/20 rounded-lg text-green-600",
                        "✓ {msg}"
                    }
                }
                div { class: "flex gap-3 justify-end mt-6",
                    button {
                        class: "px-4 py-2 text-muted-foreground hover:text-foreground",
                        disabled: *loading.read(),
                        onclick: move |_| {
                            if !*loading.read() {
                                on_close.call(());
                            }
                        },
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
