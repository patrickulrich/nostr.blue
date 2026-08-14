//! People List Members Modal
//!
//! Modal for viewing and managing members of a people list.
//! Supports:
//! - Viewing all members (public + decrypted private)
//! - Showing which members are private (encrypted)
//! - Removing members from the list
use crate::hooks::UserList;
use crate::stores::profiles;
use crate::utils::list_encryption::{decrypt_private_tags, remove_person_from_list};
use dioxus::prelude::*;
use nostr_sdk::{PublicKey, ToBech32};
/// A member in a people list
#[derive(Clone, PartialEq)]
struct ListMember {
    pubkey: String,
    display_name: String,
    is_private: bool,
}
#[derive(Props, Clone, PartialEq)]
pub struct PeopleListMembersModalProps {
    /// The list to manage
    pub list: UserList,
    /// Handler when modal is closed
    pub on_close: EventHandler<()>,
    /// Handler when list is modified (member removed)
    pub on_modified: EventHandler<()>,
}
#[component]
pub fn PeopleListMembersModal(props: PeopleListMembersModalProps) -> Element {
    let mut members = use_signal(Vec::<ListMember>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let mut removing = use_signal(|| None::<String>);
    let mut remove_error = use_signal(|| None::<String>);
    let list = props.list.clone();
    let list_for_remove = props.list.clone();
    let on_modified = props.on_modified;
    use_effect({
        let list_event = list.event.clone();
        move || {
            let list_event = list_event.clone();
            spawn(async move {
                loading.set(true);
                error_msg.set(None);
                let mut all_members = Vec::new();
                for tag in list_event.tags.iter() {
                    if tag.kind() == nostr_sdk::TagKind::p() {
                        if let Some(pk_str) = tag.content() {
                            let display_name = get_member_display_name(pk_str);
                            all_members.push(ListMember {
                                pubkey: pk_str.to_string(),
                                display_name,
                                is_private: false,
                            });
                        }
                    }
                }
                match decrypt_private_tags(&list_event).await {
                    Ok(private_tags) => {
                        for tag in private_tags {
                            if tag.kind() == nostr_sdk::TagKind::p() {
                                if let Some(pk_str) = tag.content() {
                                    if !all_members.iter().any(|m| m.pubkey == pk_str) {
                                        let display_name = get_member_display_name(pk_str);
                                        all_members.push(ListMember {
                                            pubkey: pk_str.to_string(),
                                            display_name,
                                            is_private: true,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to decrypt private members: {}", e);
                    }
                }
                members.set(all_members);
                loading.set(false);
            });
        }
    });
    let mut pubkey_to_remove = use_signal(|| None::<String>);
    use_effect({
        let list_event = list_for_remove.event.clone();
        move || {
            if let Some(pk) = pubkey_to_remove.read().clone() {
                let list_event = list_event.clone();
                removing.set(Some(pk.clone()));
                remove_error.set(None);
                spawn(async move {
                    match remove_person_from_list(&list_event, &pk).await {
                        Ok(_) => {
                            log::info!("Removed {} from list", pk);
                            members.with_mut(|m| m.retain(|member| member.pubkey != pk));
                            removing.set(None);
                            pubkey_to_remove.set(None);
                            on_modified.call(());
                        }
                        Err(e) => {
                            log::error!("Failed to remove from list: {}", e);
                            remove_error.set(Some(e));
                            removing.set(None);
                            pubkey_to_remove.set(None);
                        }
                    }
                });
            }
        }
    });
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "bg-background border border-border rounded-lg p-6 max-w-lg mx-4 w-full max-h-[80vh] flex flex-col",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex justify-between items-center mb-4",
                    div {
                        h2 { class: "text-xl font-bold flex items-center gap-2", "👥 {list.name}" }
                        p { class: "text-sm text-muted-foreground", "{members.read().len()} members" }
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground text-xl",
                        onclick: move |_| props.on_close.call(()),
                        "×"
                    }
                }
                if let Some(err) = error_msg.read().as_ref() {
                    div { class: "mb-4 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-600",
                        "{err}"
                    }
                }
                if let Some(err) = remove_error.read().as_ref() {
                    div { class: "mb-4 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-600",
                        "Failed to remove: {err}"
                    }
                }
                if *loading.read() {
                    div { class: "flex-1 flex items-center justify-center py-8",
                        div { class: "text-center",
                            div { class: "text-4xl animate-spin mb-2", "⏳" }
                            p { class: "text-muted-foreground", "Loading members..." }
                        }
                    }
                } else if members.read().is_empty() {
                    div { class: "flex-1 flex items-center justify-center py-8",
                        div { class: "text-center",
                            div { class: "text-4xl mb-2", "👻" }
                            p { class: "text-muted-foreground", "This list has no members yet." }
                        }
                    }
                } else {
                    div { class: "flex-1 overflow-y-auto space-y-2",
                        for member in members.read().iter() {
                            {
                                let member_pk = member.pubkey.clone();
                                let is_removing_this = removing.read().as_ref() == Some(&member.pubkey);
                                rsx! {
                                    MemberRow {
                                        key: "{member_pk}",
                                        member: member.clone(),
                                        is_removing: is_removing_this,
                                        on_remove: move |pk: String| pubkey_to_remove.set(Some(pk)),
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "mt-4 pt-4 border-t border-border flex justify-end",
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90",
                        onclick: move |_| props.on_close.call(()),
                        "Done"
                    }
                }
            }
        }
    }
}
/// Individual member row component
#[component]
fn MemberRow(member: ListMember, is_removing: bool, on_remove: EventHandler<String>) -> Element {
    let navigator = use_navigator();
    let pubkey = member.pubkey.clone();
    let pubkey_for_nav = member.pubkey.clone();
    let pubkey_for_remove = member.pubkey.clone();
    let npub = PublicKey::parse(&pubkey)
        .ok()
        .and_then(|pk| pk.to_bech32().ok())
        .unwrap_or_else(|| pubkey.clone());
    let pubkey_short = if pubkey.len() >= 16 {
        format!("{}...", &pubkey[..16])
    } else {
        pubkey.clone()
    };
    rsx! {
        div { class: "flex items-center gap-3 p-3 rounded-lg hover:bg-muted/50 transition group",
            div {
                class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center text-lg cursor-pointer",
                onclick: move |_| {
                    navigator.push(format!("/profile/{}", npub.clone()));
                },
                "👤"
            }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    span {
                        class: "font-medium truncate cursor-pointer hover:underline",
                        onclick: move |_| {
                            let npub = PublicKey::parse(&pubkey_for_nav)
                                .ok()
                                .and_then(|pk| pk.to_bech32().ok())
                                .unwrap_or_else(|| pubkey_for_nav.clone());
                            navigator.push(format!("/profile/{}", npub));
                        },
                        "{member.display_name}"
                    }
                    if member.is_private {
                        span {
                            class: "text-sm px-1.5 py-0.5 bg-muted rounded text-muted-foreground",
                            title: "This member is encrypted (private)",
                            "🔒 Private"
                        }
                    }
                }
                p { class: "text-xs text-muted-foreground truncate", "{pubkey_short}" }
            }
            button {
                class: "px-3 py-1.5 text-sm text-red-600 hover:bg-red-50 dark:hover:bg-red-950 rounded-lg transition opacity-0 group-hover:opacity-100 disabled:opacity-50",
                disabled: is_removing,
                onclick: move |_| on_remove.call(pubkey_for_remove.clone()),
                if is_removing {
                    span { class: "animate-spin", "⏳" }
                } else {
                    "Remove"
                }
            }
        }
    }
}
/// Helper to get display name for a pubkey
fn get_member_display_name(pubkey: &str) -> String {
    profiles::get_profile(pubkey)
        .and_then(|m| crate::stores::profiles::display_name_or_name(&m))
        .unwrap_or_else(|| {
            if pubkey.len() >= 12 {
                format!("{}...", &pubkey[..12])
            } else {
                pubkey.to_string()
            }
        })
}
