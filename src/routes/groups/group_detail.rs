use crate::components::groups::chat::{GroupChatComposer, GroupChatView};
use crate::components::groups::members::GroupMembersList;
use crate::components::groups::posts::GroupPostsView;
use crate::components::groups::settings::GroupSettings;
use crate::components::groups::share::GroupShareModal;
use crate::hooks::use_group_subscription;
use crate::hooks::use_infinite_scroll;
use crate::stores::auth_store;
use crate::stores::nostr_client::CLIENT_INITIALIZED;
use crate::stores::social::group_store::{
    add_group_to_user_list, cache_roles, check_membership_status, decode_relay_url,
    fetch_group_full, fetch_group_messages, fetch_group_notes, get_cached_pinned, is_group_muted,
    join_group, leave_group, parse_group_note, remove_group_from_user_list,
    toggle_group_mute, track_previous_event, Group, GroupAdmin,
    GroupMembershipStatus, GroupMessage, GroupNote, GroupRole, SystemMessageType,
    KIND_GROUP_NOTE, KIND_GROUP_NOTE_REPLY, KIND_ZAP_RECEIPT,
};
use dioxus::prelude::*;
use nostr::TagKind;

#[component]
pub fn GroupDetail(encoded_relay: String, group_id: String) -> Element {
    let relay_url = decode_relay_url(&encoded_relay).unwrap_or_default();
    let mut group = use_signal(|| None::<Group>);
    let mut messages = use_signal(Vec::<GroupMessage>::new);
    let mut notes_list = use_signal(Vec::<GroupNote>::new);
    let mut members_list = use_signal(Vec::<String>::new);
    let mut admins_list = use_signal(Vec::<GroupAdmin>::new);
    let mut membership = use_signal(|| GroupMembershipStatus::NotInGroup);
    let mut active_tab = use_signal(|| 0u8);
    let mut loading = use_signal(|| true);
    let mut joining = use_signal(|| false);
    let mut reply_to: Signal<Option<GroupMessage>> = use_signal(|| None);
    let mut editing: Signal<Option<GroupMessage>> = use_signal(|| None);
    let mut show_leave_confirm = use_signal(|| false);
    let mut show_share = use_signal(|| false);
    let invite_code = use_signal(String::new);
    let show_invite_field = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut pagination_loading = use_signal(|| false);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    let user_pubkey = auth_store::get_pubkey();

    let relay_for_sub = relay_url.clone();
    let group_id_for_sub = group_id.clone();
    {
        let mut messages_signal = messages;
        let mut group_signal = group;
        let mut members_signal = members_list;
        let mut admins_signal = admins_list;
        let mut membership_signal = membership;
        let mut notes_signal = notes_list;
        let user_pk = user_pubkey.clone();
        let sub_relay = relay_url.clone();
        let sub_group_id = group_id.clone();

        use_group_subscription(&relay_for_sub, &group_id_for_sub, move |event| {
            let kind = event.kind.as_u16();
            match kind {
                9 | 10 => {
                    if let Some(msg) =
                        crate::stores::social::group_store::parse_group_message(event)
                    {
                        track_previous_event(&sub_relay, &sub_group_id, &msg.id);
                        let mut current = messages_signal.write();
                        if !current.iter().any(|m: &GroupMessage| m.id == msg.id) {
                            current.insert(0, msg);
                        }
                    }
                }
                7 => {
                    let target_id = event.tags.iter().find(|t| {
                        let s = t.as_slice();
                        s.first().map(|s| s.as_str()) == Some("e")
                    }).and_then(|t| t.as_slice().get(1).cloned());
                    if let Some(target_id) = target_id {
                        let reaction_content = if event.content.trim().is_empty() {
                            "+".to_string()
                        } else {
                            event.content.trim().to_string()
                        };
                        let reactor = event.pubkey.to_hex();
                        let mut current = messages_signal.write();
                        if let Some(msg) = current.iter_mut().find(|m| m.id == target_id) {
                            msg.reactions.entry(reaction_content).or_default().push(reactor);
                        }
                    }
                }
                KIND_GROUP_NOTE | KIND_GROUP_NOTE_REPLY => {
                    if let Some(note) = parse_group_note(event) {
                        let mut current = notes_signal.write();
                        if !current.iter().any(|n| n.id == note.id) {
                            current.insert(0, note);
                        }
                    }
                }
                KIND_ZAP_RECEIPT => {
                    let target_id = event.tags.iter().find(|t| {
                        let s = t.as_slice();
                        s.first().map(|s| s.as_str()) == Some("e")
                    }).and_then(|t| t.as_slice().get(1).cloned());
                    if let Some(_target_id) = target_id {
                        let amount: u64 = event.tags.iter()
                            .find(|t| {
                                let s = t.as_slice();
                                s.first().map(|s| s.as_str()) == Some("amount")
                            })
                            .and_then(|t| t.as_slice().get(1).and_then(|v| v.parse().ok()))
                            .unwrap_or(0);
                        if amount > 0 {
                            let zapper = event.pubkey.to_hex();
                            let zap_msg = GroupMessage {
                                id: format!("zap-{}-{}", event.id.to_hex(), amount),
                                group_id: sub_group_id.clone(),
                                author: zapper,
                                content: format!("⚡ zapped {} sats", amount),
                                created_at: event.created_at.as_secs(),
                                reply_to: None,
                                reactions: std::collections::HashMap::new(),
                                event: event.clone(),
                                is_system: true,
                                system_type: None,
                                edited: false,
                            };
                            let mut current = messages_signal.write();
                            current.insert(0, zap_msg);
                        }
                    }
                }
                39000 => {
                    if let Some(g) =
                        crate::stores::social::group_store::parse_group_metadata(event, &sub_relay)
                    {
                        group_signal.set(Some(g));
                    }
                }
                39001 => {
                    let admins = crate::stores::social::group_store::parse_group_admins(event);
                    admins_signal.set(admins);
                }
                39002 => {
                    let members: Vec<String> =
                        crate::stores::social::group_store::parse_group_members(event);
                    members_signal.set(members);
                }
                39003 => {
                    let roles: Vec<GroupRole> =
                        crate::stores::social::group_store::parse_group_roles(event);
                    cache_roles(&sub_relay, &sub_group_id, roles);
                }
                9000 => {
                    if let Some(ref pk) = user_pk {
                        let relay = sub_relay.clone();
                        let gid = sub_group_id.clone();
                        let pk = pk.clone();
                        spawn(async move {
                            if let Ok(status) =
                                check_membership_status(&relay, &gid, &pk).await
                            {
                                membership_signal.set(status);
                            }
                        });
                    }
                    let added_pubkeys: Vec<String> = event.tags.iter()
                        .filter(|t| t.kind() == TagKind::p())
                        .filter_map(|t| t.content().map(|s| s.to_string()))
                        .collect();
                    for pk in added_pubkeys {
                        let is_self = user_pk.as_ref().map(|u| u == &pk).unwrap_or(false);
                        if !is_self {
                            let msg = GroupMessage {
                                id: format!("sys-join-{}-{}", event.id.to_hex(), &pk[..8.min(pk.len())]),
                                group_id: sub_group_id.clone(),
                                author: pk.clone(),
                                content: String::new(),
                                created_at: event.created_at.as_secs(),
                                reply_to: None,
                                reactions: std::collections::HashMap::new(),
                                event: event.clone(),
                                is_system: true,
                                system_type: Some(SystemMessageType::UserJoined { pubkey: pk }),
                                edited: false,
                            };
                            let mut current = messages_signal.write();
                            current.insert(0, msg);
                        }
                    }
                }
                9001 => {
                    if let Some(ref pk) = user_pk {
                        let relay = sub_relay.clone();
                        let gid = sub_group_id.clone();
                        let pk = pk.clone();
                        spawn(async move {
                            if let Ok(status) =
                                check_membership_status(&relay, &gid, &pk).await
                            {
                                membership_signal.set(status);
                            }
                        });
                    }
                }
                9005 => {
                    let deleted_by = event.pubkey.to_hex();
                    let msg = GroupMessage {
                        id: format!("sys-del-{}", event.id.to_hex()),
                        group_id: sub_group_id.clone(),
                        author: deleted_by,
                        content: String::new(),
                        created_at: event.created_at.as_secs(),
                        reply_to: None,
                        reactions: std::collections::HashMap::new(),
                        event: event.clone(),
                        is_system: true,
                        system_type: Some(SystemMessageType::MessageDeleted { by: event.pubkey.to_hex() }),
                        edited: false,
                    };
                    messages_signal.write().insert(0, msg);
                }
                9022 => {
                    let leaver = event.pubkey.to_hex();
                    let is_self = user_pk.as_ref().map(|u| u == &leaver).unwrap_or(false);
                    if !is_self {
                        let msg = GroupMessage {
                            id: format!("sys-leave-{}", event.id.to_hex()),
                            group_id: sub_group_id.clone(),
                            author: leaver.clone(),
                            content: String::new(),
                            created_at: event.created_at.as_secs(),
                            reply_to: None,
                            reactions: std::collections::HashMap::new(),
                            event: event.clone(),
                            is_system: true,
                            system_type: Some(SystemMessageType::UserLeft { pubkey: leaver }),
                            edited: false,
                        };
                        messages_signal.write().insert(0, msg);
                    }
                }
                _ => {}
            }
        });
    }

    {
        let relay_url = relay_url.clone();
        let group_id = group_id.clone();
        let user_pubkey = user_pubkey.clone();
        use_effect(move || {
            if !*CLIENT_INITIALIZED.read() {
                return;
            }
            let relay_url = relay_url.clone();
            let group_id = group_id.clone();
            let user_pubkey = user_pubkey.clone();
            spawn(async move {
                loading.set(true);

                let (group_result, messages_result, members_result, admins_result) =
                    futures::join!(
                        fetch_group_full(&relay_url, &group_id),
                        fetch_group_messages(&relay_url, &group_id, 50, None),
                        async {
                            let r = crate::stores::social::group_store::fetch_group_members(
                                &relay_url, &group_id,
                            )
                            .await;
                            r.map(|s| s.into_iter().collect::<Vec<_>>())
                        },
                        crate::stores::social::group_store::fetch_group_admins(
                            &relay_url, &group_id
                        ),
                    );

                match group_result {
                    Ok(g) => group.set(Some(g)),
                    Err(_) => group.set(Some(Group {
                        id: group_id.clone(),
                        relay_url: relay_url.clone(),
                        name: None,
                        about: None,
                        picture: None,
                        is_private: false,
                        is_restricted: false,
                        is_hidden: false,
                        is_closed: false,
                        created_at: 0,
                        event: None,
                    })),
                }
                if let Ok(msgs) = messages_result {
                    if let Some(oldest) = msgs.iter().map(|m| m.created_at).min() {
                        oldest_timestamp.set(Some(oldest));
                    }
                    has_more.set(msgs.len() >= 50);
                    messages.set(msgs);
                }
                if let Ok(m) = members_result {
                    members_list.set(m);
                }
                if let Ok(a) = admins_result {
                    admins_list.set(a);
                }

                if let Ok(notes) = fetch_group_notes(&relay_url, &group_id, 50, None).await {
                    notes_list.set(notes);
                }

                if let Some(ref pk) = user_pubkey {
                    if let Ok(status) =
                        check_membership_status(&relay_url, &group_id, pk).await
                    {
                        membership.set(status);
                    }
                }

                loading.set(false);
            });
        });
    }

    let load_more = {
        let relay_url = relay_url.clone();
        let group_id = group_id.clone();
        move || {
            if *pagination_loading.read() || !*has_more.read() {
                return;
            }
            let until = *oldest_timestamp.read();
            if until.is_none() {
                return;
            }
            let relay_url = relay_url.clone();
            let group_id = group_id.clone();
            pagination_loading.set(true);
            spawn(async move {
                if let Ok(new_msgs) =
                    fetch_group_messages(&relay_url, &group_id, 50, until).await
                {
                    if new_msgs.is_empty() {
                        has_more.set(false);
                    } else {
                        if let Some(oldest) = new_msgs.iter().map(|m| m.created_at).min() {
                            oldest_timestamp.set(Some(oldest));
                        }
                        has_more.set(new_msgs.len() >= 50);
                        let mut current = messages.write();
                        let existing_ids: std::collections::HashSet<String> =
                            current.iter().map(|m| m.id.clone()).collect();
                        for msg in new_msgs {
                            if !existing_ids.contains(&msg.id) {
                                current.push(msg);
                            }
                        }
                    }
                }
                pagination_loading.set(false);
            });
        }
    };

    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);

    let is_member = matches!(
        *membership.read(),
        GroupMembershipStatus::Admin { .. } | GroupMembershipStatus::Member
    );
    let is_admin = matches!(*membership.read(), GroupMembershipStatus::Admin { .. });
    let muted = is_group_muted(&relay_url, &group_id);
    let group_name = group
        .read()
        .as_ref()
        .and_then(|g| g.name.clone())
        .unwrap_or_else(|| group_id.clone());
    let is_closed_group = group.read().as_ref().map(|g| g.is_closed).unwrap_or(false);

    let handle_reply = move |msg: GroupMessage| {
        reply_to.set(Some(msg));
    };

    let handle_edit = move |msg: GroupMessage| {
        editing.set(Some(msg));
    };

    let handle_delete = move |event_id: String| {
        messages.write().retain(|m| m.id != event_id);
    };

    let pinned = get_cached_pinned(&relay_url, &group_id);

    rsx! {
        div { class: "min-h-screen flex flex-col",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "flex items-center gap-3 px-4 py-3",
                    button {
                        class: "p-1 hover:bg-accent rounded-lg transition",
                        onclick: move |_| {
                            let nav = use_navigator();
                            nav.push(crate::routes::Route::Groups {});
                        },
                        "←"
                    }
                    h1 { class: "text-lg font-semibold text-foreground truncate flex-1", "{group_name}" }
                    if group.read().as_ref().map(|g| g.is_private).unwrap_or(false) {
                        span { class: "text-xs px-2 py-0.5 rounded bg-yellow-500/20 text-yellow-600 dark:text-yellow-400", "Private" }
                    }
                    if group.read().as_ref().map(|g| g.name.is_none()).unwrap_or(false) {
                        span { class: "text-xs px-2 py-0.5 rounded bg-muted text-muted-foreground", "Unmanaged" }
                    }
                    if is_member && !*loading.read() {
                        button {
                            class: "p-1 hover:bg-accent rounded-lg transition text-muted-foreground",
                            onclick: {
                                let r = relay_url.clone();
                                let g = group_id.clone();
                                move |_| { toggle_group_mute(&r, &g); }
                            },
                            if muted { "🔇" } else { "🔔" }
                        }
                        button {
                            class: "p-1 hover:bg-accent rounded-lg transition text-muted-foreground text-sm",
                            onclick: move |_| show_share.set(true),
                            "↗"
                        }
                        div { class: "relative",
                            button {
                                class: "p-1 hover:bg-accent rounded-lg transition text-muted-foreground text-sm",
                                onclick: move |_| show_leave_confirm.set(!show_leave_confirm()),
                                "..."
                            }
                            if show_leave_confirm() {
                                {
                                    let relay_leave = relay_url.clone();
                                    let gid_leave = group_id.clone();
                                    rsx! {
                                        div { class: "absolute right-0 top-full mt-1 bg-card border border-border rounded-lg shadow-lg z-50 py-1 min-w-[140px]",
                                            button {
                                                class: "w-full text-left px-3 py-2 text-sm text-red-600 dark:text-red-400 hover:bg-accent/50 transition",
                                                onclick: move |_| {
                                                    let relay = relay_leave.clone();
                                                    let gid = gid_leave.clone();
                                                    show_leave_confirm.set(false);
                                                    spawn(async move {
                                                        let _ = leave_group(&relay, &gid, None).await;
                                                        let _ = remove_group_from_user_list(&relay, &gid).await;
                                                    });
                                                },
                                                "Leave Group"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if is_member {
                    div { class: "flex border-t border-border",
                        button {
                            class: if active_tab() == 0 {
                                "flex-1 px-4 py-2 text-sm font-medium text-foreground border-b-2 border-primary"
                            } else {
                                "flex-1 px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| active_tab.set(0),
                            "Chat"
                        }
                        button {
                            class: if active_tab() == 1 {
                                "flex-1 px-4 py-2 text-sm font-medium text-foreground border-b-2 border-primary"
                            } else {
                                "flex-1 px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| active_tab.set(1),
                            "Posts"
                        }
                        button {
                            class: if active_tab() == 2 {
                                "flex-1 px-4 py-2 text-sm font-medium text-foreground border-b-2 border-primary"
                            } else {
                                "flex-1 px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| active_tab.set(2),
                            "Members"
                        }
                        if is_admin {
                            button {
                                class: if active_tab() == 3 {
                                    "flex-1 px-4 py-2 text-sm font-medium text-foreground border-b-2 border-primary"
                                } else {
                                    "flex-1 px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                                },
                                onclick: move |_| active_tab.set(3),
                                "Settings"
                            }
                        }
                    }
                }
            }

            if *loading.read() {
                div { class: "flex-1 flex items-center justify-center",
                    p { class: "text-muted-foreground", "Loading..." }
                }
            } else if !is_member {
                {
                    let relay_url_join = relay_url.clone();
                    let group_id_join = group_id.clone();
                    let about_text = group.read().as_ref().and_then(|g| g.about.clone());
                    let mut show_invite = show_invite_field;
                    let mut inv_code = invite_code;
                    rsx! {
                        div { class: "flex-1 flex flex-col items-center justify-center p-8 space-y-4",
                            if let Some(about) = about_text {
                                p { class: "text-muted-foreground text-center max-w-md", "{about}" }
                            }
                            match (*membership.read()).clone() {
                                GroupMembershipStatus::Pending => {
                                    rsx! {
                                        span { class: "px-4 py-2 rounded-lg text-sm bg-yellow-500/20 text-yellow-600 dark:text-yellow-400",
                                            "Join request pending"
                                        }
                                    }
                                }
                                GroupMembershipStatus::NotInGroupButKnown => {
                                    rsx! {
                                        button {
                                            class: "px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                            disabled: *joining.read(),
                                            onclick: {
                                                let relay = relay_url_join.clone();
                                                let gid = group_id_join.clone();
                                                move |_| {
                                                    let relay = relay.clone();
                                                    let gid = gid.clone();
                                                    joining.set(true);
                                                    spawn(async move {
                                                        let _ = add_group_to_user_list(&relay, &gid).await;
                                                        let _ = join_group(&relay, &gid, None, None).await;
                                                        joining.set(false);
                                                    });
                                                }
                                            },
                                            if *joining.read() { "Joining..." } else { "Join Group" }
                                        }
                                    }
                                }
                                _ => {
                                    rsx! {
                                        div { class: "space-y-3 flex flex-col items-center",
                                            if is_closed_group {
                                                button {
                                                    class: "text-sm text-primary hover:underline",
                                                    onclick: move |_| show_invite.set(!show_invite()),
                                                    if show_invite() { "Hide invite code" } else { "Have an invite code?" }
                                                }
                                                if show_invite() {
                                                    input {
                                                        class: "w-full max-w-xs px-3 py-2 bg-background border border-border rounded-lg text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary",
                                                        placeholder: "Enter invite code",
                                                        value: "{inv_code}",
                                                        oninput: move |e| inv_code.set(e.value()),
                                                    }
                                                }
                                            }
                                            button {
                                                class: "px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                                disabled: *joining.read(),
                                                onclick: {
                                                    let relay = relay_url_join.clone();
                                                    let gid = group_id_join.clone();
                                                    move |_| {
                                                        let relay = relay.clone();
                                                        let gid = gid.clone();
                                                        let code = if inv_code().is_empty() { None } else { Some(inv_code()) };
                                                        joining.set(true);
                                                        spawn(async move {
                                                            let _ = join_group(&relay, &gid, None, code.as_deref()).await;
                                                            joining.set(false);
                                                        });
                                                    }
                                                },
                                                if *joining.read() { "Requesting..." } else { "Request to Join" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                {
                    let mut sorted_messages: Vec<GroupMessage> = messages();
                    sorted_messages.sort_by_key(|a| a.created_at);
                    rsx! {
                        div { class: "flex-1 flex flex-col",
                            match active_tab() {
                                0 => rsx! {
                                    if !pinned.is_empty() {
                                        div { class: "border-b border-border bg-primary/5 px-4 py-2",
                                            div { class: "flex items-center gap-2 text-xs font-medium text-primary",
                                                "📌 Pinned ({pinned.len()})"
                                            }
                                            for id in &pinned {
                                                div { class: "text-sm text-muted-foreground truncate mt-0.5", "{id}" }
                                            }
                                        }
                                    }
                                    GroupChatView {
                                        messages: sorted_messages,
                                        is_admin,
                                        relay_url: relay_url.clone(),
                                        group_id: group_id.clone(),
                                        sentinel_id: sentinel_id.clone(),
                                        has_more: *has_more.read(),
                                        pagination_loading: *pagination_loading.read(),
                                        on_reply: handle_reply,
                                        on_edit: handle_edit,
                                        on_delete: handle_delete,
                                    }
                                    GroupChatComposer {
                                        relay_url: relay_url.clone(),
                                        group_id: group_id.clone(),
                                        reply_to,
                                        editing,
                                    }
                                },
                                1 => rsx! {
                                    GroupPostsView {
                                        notes: notes_list(),
                                        relay_url: relay_url.clone(),
                                        group_id: group_id.clone(),
                                    }
                                },
                                2 => rsx! {
                                    GroupMembersList {
                                        members: members_list(),
                                        admins: admins_list(),
                                        current_user: user_pubkey.clone(),
                                        is_admin,
                                        relay_url: relay_url.clone(),
                                        group_id: group_id.clone(),
                                    }
                                },
                                3 if is_admin => {
                                    if let Some(g) = group() {
                                        let pk = user_pubkey.unwrap_or_default();
                                        rsx! {
                                            GroupSettings { group: g, current_user: pk }
                                        }
                                    } else {
                                        rsx! { div { class: "p-4 text-muted-foreground", "Loading settings..." } }
                                    }
                                },
                                _ => rsx! { div {} },
                            }
                        }
                    }
                }
            }
        }

        if show_share() {
            if let Some(g) = group() {
                GroupShareModal {
                    group: g,
                    on_close: move |_| show_share.set(false),
                }
            }
        }
    }
}
