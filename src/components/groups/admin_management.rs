use crate::stores::profiles;
use crate::stores::social::group_store::{
    add_permission, get_admin_permissions, get_cached_admins,
    remove_permission, GroupAdmin, GroupAdminPermission,
};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

fn has_perm(perms: &[GroupAdminPermission], perm: GroupAdminPermission) -> bool {
    perms.contains(&perm)
}

#[component]
pub fn GroupAdminManagement(
    relay_url: String,
    group_id: String,
    current_user: String,
) -> Element {
    let mut show_add_admin = use_signal(|| false);
    let mut editing_admin = use_signal(|| None::<String>);
    let my_permissions = get_admin_permissions(&relay_url, &group_id, &current_user);
    let can_manage = has_perm(&my_permissions, GroupAdminPermission::AddPermission);
    let admins = get_cached_admins(&relay_url, &group_id);

    rsx! {
        div { class: "space-y-3",
            div { class: "flex items-center justify-between",
                h3 { class: "text-sm font-semibold text-foreground", "Administrators ({admins.len()})" }
                if can_manage {
                    button {
                        class: "px-3 py-1.5 bg-primary text-primary-foreground rounded-lg text-xs",
                        onclick: move |_| show_add_admin.set(true),
                        "+ Add Admin"
                    }
                }
            }

            for admin in &admins {
                {
                    let relay = relay_url.clone();
                    let gid = group_id.clone();
                    let admin_pk = admin.pubkey.clone();
                    let is_editing = editing_admin().as_ref() == Some(&admin.pubkey);
                    rsx! {
                        AdminRow {
                            key: "{admin.pubkey}",
                            admin: admin.clone(),
                            relay_url: relay,
                            group_id: gid,
                            can_manage,
                            is_editing,
                            on_edit: {
                                let pk = admin_pk.clone();
                                move |_| {
                                    let pk = pk.clone();
                                    if editing_admin().as_ref() == Some(&pk) {
                                        editing_admin.set(None);
                                    } else {
                                        editing_admin.set(Some(pk));
                                    }
                                }
                            },
                            current_user: current_user.clone(),
                        }
                    }
                }
            }

            if show_add_admin() && can_manage {
                {
                    let relay = relay_url.clone();
                    let gid = group_id.clone();
                    rsx! {
                        AddAdminModal {
                            relay_url: relay,
                            group_id: gid,
                            on_close: move |_| show_add_admin.set(false),
                        }
                    }
                }
            }

            if let Some(pk) = editing_admin() {
                {
                    let relay = relay_url.clone();
                    let gid = group_id.clone();
                    rsx! {
                        EditPermissionsModal {
                            key: "{pk}",
                            relay_url: relay,
                            group_id: gid,
                            admin_pubkey: pk,
                            on_close: move |_| editing_admin.set(None),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AdminRow(
    admin: GroupAdmin,
    relay_url: String,
    group_id: String,
    can_manage: bool,
    is_editing: bool,
    on_edit: EventHandler<()>,
    current_user: String,
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let pk = admin.pubkey.clone();

    {
        let pk = pk.clone();
        use_effect(move || {
            let pk = pk.clone();
            spawn(async move {
                if let Ok(p) = profiles::fetch_profile(pk).await {
                    profile.set(Some(p));
                }
            });
        });
    }

    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| truncate_pubkey(&pk));
    let is_self = current_user == admin.pubkey;

    rsx! {
        div { class: "flex items-center gap-3 p-2 rounded-lg hover:bg-accent/50 transition",
            div { class: "flex-1 min-w-0",
                span { class: "text-sm text-foreground", "{display_name}" }
                if is_self {
                    span { class: "text-xs text-muted-foreground ml-2", "(you)" }
                }
            }
            if !admin.role.is_empty() {
                span { class: "text-xs px-2 py-0.5 rounded bg-yellow-500/20 text-yellow-600 dark:text-yellow-400", "{admin.role}" }
            }
            if !admin.permissions.is_empty() {
                div { class: "flex flex-wrap gap-1",
                    for perm in &admin.permissions {
                        span { class: "text-xs px-1.5 py-0.5 rounded bg-muted text-muted-foreground", "{perm.as_str()}" }
                    }
                }
            }
            if can_manage && !is_self {
                button {
                    class: "text-xs px-2 py-1 rounded bg-accent hover:bg-accent/80 transition",
                    onclick: move |_| on_edit.call(()),
                    if is_editing { "Close" } else { "Edit" }
                }
            }
        }
    }
}

#[component]
fn AddAdminModal(
    relay_url: String,
    group_id: String,
    on_close: EventHandler<()>,
) -> Element {
    let mut pubkey_input = use_signal(String::new);
    let mut saving = use_signal(|| false);

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card border border-border rounded-lg p-6 max-w-sm w-full mx-4 space-y-4",
                onclick: move |e| e.stop_propagation(),
                h3 { class: "text-lg font-semibold text-foreground", "Add Admin" }
                p { class: "text-sm text-muted-foreground", "Enter the pubkey of a group member to promote to admin (bishop permissions)." }
                div { class: "flex gap-2",
                    input {
                        class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary",
                        placeholder: "Pubkey (hex)",
                        value: "{pubkey_input}",
                        oninput: move |e| pubkey_input.set(e.value()),
                    }
                }
                div { class: "flex gap-2 justify-end",
                    button {
                        class: "px-4 py-2 rounded-lg hover:bg-accent transition text-sm",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50 text-sm",
                        disabled: pubkey_input().is_empty() || *saving.read(),
                        onclick: {
                            let relay = relay_url.clone();
                            let gid = group_id.clone();
                            move |_| {
                                let relay = relay.clone();
                                let gid = gid.clone();
                                let pk = pubkey_input();
                                saving.set(true);
                                spawn(async move {
                                    let bishop_perms = vec![
                                        "add-user", "remove-user", "edit-metadata",
                                        "delete-event", "edit-group-status", "create-invite",
                                    ];
                                    for perm in bishop_perms {
                                        let _ = add_permission(&relay, &gid, &pk, perm).await;
                                    }
                                    saving.set(false);
                                });
                            }
                        },
                        if *saving.read() { "Adding..." } else { "Add Admin" }
                    }
                }
            }
        }
    }
}

#[component]
fn EditPermissionsModal(
    relay_url: String,
    group_id: String,
    admin_pubkey: String,
    on_close: EventHandler<()>,
) -> Element {
    let admins = get_cached_admins(&relay_url, &group_id);
    let admin = admins.iter().find(|a| a.pubkey == admin_pubkey);
    let current_perms: Vec<GroupAdminPermission> = admin
        .map(|a| a.permissions.clone())
        .unwrap_or_default();
    let all_perms = vec![
        GroupAdminPermission::AddUser,
        GroupAdminPermission::RemoveUser,
        GroupAdminPermission::EditMetadata,
        GroupAdminPermission::AddPermission,
        GroupAdminPermission::RemovePermission,
        GroupAdminPermission::DeleteEvent,
        GroupAdminPermission::EditGroupStatus,
        GroupAdminPermission::DeleteGroup,
        GroupAdminPermission::CreateInvite,
    ];

    let mut saving = use_signal(|| false);
    let mut local_perms = use_signal(move || current_perms);
    let initial_perms = local_perms();
    let all_perms_cloned = all_perms.clone();

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card border border-border rounded-lg p-6 max-w-sm w-full mx-4 space-y-4",
                onclick: move |e| e.stop_propagation(),
                h3 { class: "text-lg font-semibold text-foreground", "Edit Permissions" }
                p { class: "text-sm text-muted-foreground", "Toggle permissions for {truncate_pubkey(&admin_pubkey)}" }
                div { class: "grid grid-cols-2 gap-2",
                    for perm in &all_perms {
                        {
                            let is_active = local_perms().contains(perm);
                            let label = perm.as_str();
                            rsx! {
                                button {
                                    key: "{label}",
                                    class: if is_active {
                                        "p-2 rounded-lg border-2 border-primary bg-primary/10 text-left text-sm"
                                    } else {
                                        "p-2 rounded-lg border border-border bg-background text-left text-sm hover:border-primary/50 transition"
                                    },
                                    onclick: {
                                        let perm = perm.clone();
                                        move |_| {
                                            let mut current = local_perms();
                                            if current.contains(&perm) {
                                                current.retain(|p| p != &perm);
                                            } else {
                                                current.push(perm.clone());
                                            }
                                            local_perms.set(current);
                                        }
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
                div { class: "flex gap-2 justify-end",
                    button {
                        class: "px-4 py-2 rounded-lg hover:bg-accent transition text-sm",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50 text-sm",
                        disabled: *saving.read(),
                        onclick: {
                            let relay = relay_url.clone();
                            let gid = group_id.clone();
                            let pk = admin_pubkey.clone();
                            let initial = initial_perms.clone();
                            let perms_list = all_perms_cloned.clone();
                            move |_| {
                                let relay = relay.clone();
                                let gid = gid.clone();
                                let pk = pk.clone();
                                let initial = initial.clone();
                                let perms = perms_list.clone();
                                saving.set(true);
                                spawn(async move {
                                    let target = local_perms();
                                    for perm in &perms {
                                        let was_active = initial.contains(perm);
                                        let should_be = target.contains(perm);
                                        if should_be && !was_active {
                                            let _ = add_permission(&relay, &gid, &pk, perm.as_str()).await;
                                        } else if !should_be && was_active {
                                            let _ = remove_permission(&relay, &gid, &pk, perm.as_str()).await;
                                        }
                                    }
                                    saving.set(false);
                                });
                            }
                        },
                        if *saving.read() { "Saving..." } else { "Save" }
                    }
                }
            }
        }
    }
}
