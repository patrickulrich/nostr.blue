use crate::components::groups::admin_management::GroupAdminManagement;
use crate::components::groups::join_requests::GroupJoinRequests;
use crate::stores::social::group_store::{
    add_user_to_group, create_invite, delete_group, edit_group_metadata, edit_group_status,
    get_admin_permissions, get_cached_roles, remove_user_from_group, Group,
    GroupAdminPermission,
};
use dioxus::prelude::*;

fn has_permission(perms: &[GroupAdminPermission], perm: GroupAdminPermission) -> bool {
    perms.contains(&perm)
}

#[component]
pub fn GroupSettings(group: Group, current_user: String) -> Element {
    let permissions = get_admin_permissions(&group.relay_url, &group.id, &current_user);
    let can_edit = has_permission(&permissions, GroupAdminPermission::EditMetadata);
    let can_add_user = has_permission(&permissions, GroupAdminPermission::AddUser);
    let can_remove_user = has_permission(&permissions, GroupAdminPermission::RemoveUser);
    let can_edit_status = has_permission(&permissions, GroupAdminPermission::EditGroupStatus);
    let can_delete = has_permission(&permissions, GroupAdminPermission::DeleteGroup);
    let can_invite = has_permission(&permissions, GroupAdminPermission::CreateInvite);

    let mut name = use_signal(|| group.name.clone().unwrap_or_default());
    let mut about = use_signal(|| group.about.clone().unwrap_or_default());
    let mut picture = use_signal(|| group.picture.clone().unwrap_or_default());
    let mut add_pubkey = use_signal(String::new);
    let mut selected_roles = use_signal(Vec::<String>::new);
    let mut remove_pubkey = use_signal(String::new);
    let mut invite_code = use_signal(String::new);
    let mut generated_code = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);
    let relay_url = group.relay_url.clone();
    let group_id = group.id.clone();

    rsx! {
        div { class: "p-4 space-y-6",
            if can_edit {
                div { class: "space-y-4",
                    h3 { class: "text-sm font-semibold text-muted-foreground", "Group Info" }
                    div { class: "space-y-2",
                        label { class: "text-sm text-foreground", "Name" }
                        input {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary",
                            value: "{name}",
                            oninput: move |e| name.set(e.value()),
                        }
                    }
                    div { class: "space-y-2",
                        label { class: "text-sm text-foreground", "About" }
                        textarea {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary resize-none",
                            rows: 3,
                            value: "{about}",
                            oninput: move |e| about.set(e.value()),
                        }
                    }
                    div { class: "space-y-2",
                        label { class: "text-sm text-foreground", "Picture URL" }
                        input {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary",
                            value: "{picture}",
                            oninput: move |e| picture.set(e.value()),
                        }
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50 text-sm",
                        disabled: *saving.read(),
                        onclick: {
                            let relay = relay_url.clone();
                            let gid = group_id.clone();
                            move |_| {
                                let relay = relay.clone();
                                let gid = gid.clone();
                                let n = if name().is_empty() { None } else { Some(name()) };
                                let a = if about().is_empty() { None } else { Some(about()) };
                                let p = if picture().is_empty() { None } else { Some(picture()) };
                                saving.set(true);
                                spawn(async move {
                                    let _ = edit_group_metadata(&relay, &gid, n.as_deref(), a.as_deref(), p.as_deref()).await;
                                    saving.set(false);
                                });
                            }
                        },
                        if *saving.read() { "Saving..." } else { "Save Changes" }
                    }
                }
            }

            if can_edit_status {
                div { class: "space-y-4 border-t border-border pt-4",
                    h3 { class: "text-sm font-semibold text-muted-foreground", "Group Status" }
                    div { class: "grid grid-cols-2 gap-3",
                        {
                            let relay = relay_url.clone();
                            let gid = group_id.clone();
                            rsx! {
                                StatusToggle {
                                    label: "Private",
                                    description: "Only members can read messages",
                                    active: group.is_private,
                                    relay_url: relay.clone(),
                                    group_id: gid.clone(),
                                    field: "private",
                                }
                                StatusToggle {
                                    label: "Closed",
                                    description: "Join requests are ignored",
                                    active: group.is_closed,
                                    relay_url: relay.clone(),
                                    group_id: gid.clone(),
                                    field: "closed",
                                }
                                StatusToggle {
                                    label: "Restricted",
                                    description: "Only members can write",
                                    active: group.is_restricted,
                                    relay_url: relay.clone(),
                                    group_id: gid.clone(),
                                    field: "restricted",
                                }
                                StatusToggle {
                                    label: "Hidden",
                                    description: "Metadata hidden from non-members",
                                    active: group.is_hidden,
                                    relay_url: relay,
                                    group_id: gid,
                                    field: "hidden",
                                }
                            }
                        }
                    }
                }
            }

            if can_add_user {
                div { class: "space-y-4 border-t border-border pt-4",
                    h3 { class: "text-sm font-semibold text-muted-foreground", "Join Requests" }
                    GroupJoinRequests {
                        relay_url: relay_url.clone(),
                        group_id: group_id.clone(),
                    }
                }
            }

            {
                let relay = relay_url.clone();
                let gid = group_id.clone();
                let pk = current_user.clone();
                rsx! {
                    div { class: "space-y-4 border-t border-border pt-4",
                        h3 { class: "text-sm font-semibold text-muted-foreground", "Admin Management" }
                        GroupAdminManagement {
                            relay_url: relay,
                            group_id: gid,
                            current_user: pk,
                        }
                    }
                }
            }

            if can_add_user {
                div { class: "space-y-4 border-t border-border pt-4",
                    h3 { class: "text-sm font-semibold text-muted-foreground", "Add Member" }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary",
                            placeholder: "Pubkey (hex)",
                            value: "{add_pubkey}",
                            oninput: move |e| add_pubkey.set(e.value()),
                        }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm",
                            disabled: add_pubkey().is_empty(),
                            onclick: {
                                let relay = relay_url.clone();
                                let gid = group_id.clone();
                                move |_| {
                                    let relay = relay.clone();
                                    let gid = gid.clone();
                                    let pk = add_pubkey();
                                    let selected = selected_roles();
                                    add_pubkey.set(String::new());
                                    selected_roles.set(Vec::new());
                                    spawn(async move {
                                        let _ = add_user_to_group(&relay, &gid, &pk, selected).await;
                                    });
                                }
                            },
                            "Add"
                        }
                    }
                    {
                        let roles = get_cached_roles(&relay_url, &group_id);
                        if roles.is_empty() {
                            rsx! { div {} }
                        } else {
                            rsx! {
                                div { class: "mt-2 space-y-1",
                                    span { class: "text-xs text-muted-foreground", "Assign roles:" }
                                    div { class: "flex flex-wrap gap-1.5 mt-1",
                                        for role in &roles {
                                            {
                                                let role_name = role.name.clone();
                                                let is_selected = selected_roles().contains(&role_name);
                                                rsx! {
                                                    button {
                                                        key: "{role_name}",
                                                        class: if is_selected {
                                                            "text-xs px-2 py-1 rounded-md border border-primary bg-primary/10 text-foreground"
                                                        } else {
                                                            "text-xs px-2 py-1 rounded-md border border-border bg-background text-muted-foreground hover:border-primary/50 transition"
                                                        },
                                                        onclick: {
                                                            let rn = role_name.clone();
                                                            move |_| {
                                                                let mut current = selected_roles();
                                                                if current.contains(&rn) {
                                                                    current.retain(|r| r != &rn);
                                                                } else {
                                                                    current.push(rn.clone());
                                                                }
                                                                selected_roles.set(current);
                                                            }
                                                        },
                                                        "{role_name}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if can_remove_user {
                div { class: "space-y-4 border-t border-border pt-4",
                    h3 { class: "text-sm font-semibold text-muted-foreground", "Remove Member" }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary",
                            placeholder: "Pubkey (hex)",
                            value: "{remove_pubkey}",
                            oninput: move |e| remove_pubkey.set(e.value()),
                        }
                        button {
                            class: "px-4 py-2 bg-red-500/20 text-red-600 dark:text-red-400 rounded-lg hover:bg-red-500/30 transition text-sm",
                            disabled: remove_pubkey().is_empty(),
                            onclick: {
                                let relay = relay_url.clone();
                                let gid = group_id.clone();
                                move |_| {
                                    let relay = relay.clone();
                                    let gid = gid.clone();
                                    let pk = remove_pubkey();
                                    remove_pubkey.set(String::new());
                                    spawn(async move {
                                        let _ = remove_user_from_group(&relay, &gid, &pk).await;
                                    });
                                }
                            },
                            "Remove"
                        }
                    }
                }
            }

            if can_invite {
                div { class: "space-y-4 border-t border-border pt-4",
                    h3 { class: "text-sm font-semibold text-muted-foreground", "Invite Code" }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary",
                            placeholder: "Custom code (optional)",
                            value: "{invite_code}",
                            oninput: move |e| invite_code.set(e.value()),
                        }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm",
                            onclick: {
                                let relay = relay_url.clone();
                                let gid = group_id.clone();
                                move |_| {
                                    let relay = relay.clone();
                                    let gid = gid.clone();
                                    let code = if invite_code().is_empty() {
                                        uuid::Uuid::new_v4().to_string()[..8].to_string()
                                    } else {
                                        invite_code()
                                    };
                                    let code_display = code.clone();
                                    spawn(async move {
                                        let _ = create_invite(&relay, &gid, &code).await;
                                    });
                                    generated_code.set(Some(code_display));
                                }
                            },
                            "Generate"
                        }
                    }
                    if let Some(code) = generated_code.read().as_ref() {
                        div { class: "px-3 py-2 bg-accent rounded-lg text-sm text-foreground font-mono select-all",
                            "{code}"
                        }
                    }
                }
            }

            if can_delete {
                div { class: "space-y-4 border-t border-border pt-4",
                    button {
                        class: "w-full px-4 py-2 bg-red-500/20 text-red-600 dark:text-red-400 rounded-lg hover:bg-red-500/30 transition text-sm font-medium",
                        onclick: {
                            let relay = relay_url.clone();
                            let gid = group_id.clone();
                            move |_| {
                                let relay = relay.clone();
                                let gid = gid.clone();
                                spawn(async move {
                                    let _ = delete_group(&relay, &gid).await;
                                });
                            }
                        },
                        "Delete Group"
                    }
                }
            }
        }
    }
}

#[component]
fn StatusToggle(
    label: String,
    description: String,
    active: bool,
    relay_url: String,
    group_id: String,
    field: String,
) -> Element {
    rsx! {
        button {
            class: if active {
                "p-3 rounded-lg border-2 border-primary bg-primary/10 text-left"
            } else {
                "p-3 rounded-lg border border-border bg-background text-left hover:border-primary/50 transition"
            },
            onclick: {
                let relay = relay_url.clone();
                let gid = group_id.clone();
                let f = field.clone();
                move |_| {
                    let relay = relay.clone();
                    let gid = gid.clone();
                    let f = f.clone();
                    spawn(async move {
                        let (is_private, is_closed, is_restricted, is_hidden) = match f.as_str() {
                            "private" => (Some(true), None, None, None),
                            "closed" => (None, Some(true), None, None),
                            "restricted" => (None, None, Some(true), None),
                            "hidden" => (None, None, None, Some(true)),
                            _ => (None, None, None, None),
                        };
                        let _ = edit_group_status(&relay, &gid, is_private, is_closed, is_restricted, is_hidden).await;
                    });
                }
            },
            div { class: "text-sm font-medium text-foreground", "{label}" }
            div { class: "text-xs text-muted-foreground mt-0.5", "{description}" }
        }
    }
}
