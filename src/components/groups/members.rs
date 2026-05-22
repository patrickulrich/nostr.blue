use crate::stores::profiles;
use crate::stores::social::group_store::{remove_user_from_group, GroupAdmin};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

#[component]
pub fn GroupMembersList(
    members: Vec<String>,
    admins: Vec<GroupAdmin>,
    current_user: Option<String>,
    is_admin: bool,
    relay_url: String,
    group_id: String,
) -> Element {
    let admin_pubkeys: std::collections::HashSet<String> =
        admins.iter().map(|a| a.pubkey.clone()).collect();
    let mut sorted_members = members.clone();
    sorted_members.sort_by(|a, b| {
        let a_is_admin = admin_pubkeys.contains(a);
        let b_is_admin = admin_pubkeys.contains(b);
        match (a_is_admin, b_is_admin) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        }
    });

    rsx! {
        div { class: "p-4 space-y-2",
            h3 { class: "text-sm font-semibold text-muted-foreground mb-3",
                "{members.len()} members"
            }
            for pubkey in sorted_members {
                {
                    let is_admin_user = admin_pubkeys.contains(&pubkey);
                    let role = admins.iter().find(|a| a.pubkey == pubkey).map(|a| a.role.clone()).unwrap_or_default();
                    let is_self = current_user.as_ref().map(|u| u == &pubkey).unwrap_or(false);
                    let can_kick = is_admin && !is_self && !is_admin_user;
                    rsx! {
                        MemberItem {
                            key: "{pubkey}",
                            pubkey: pubkey.clone(),
                            is_self,
                            is_admin: is_admin_user,
                            role,
                            can_kick,
                            relay_url: relay_url.clone(),
                            group_id: group_id.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MemberItem(
    pubkey: String,
    is_self: bool,
    is_admin: bool,
    role: String,
    can_kick: bool,
    relay_url: String,
    group_id: String,
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let pk = pubkey.clone();

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
        .unwrap_or_else(|| truncate_pubkey(&pubkey));
    let initial = display_name.chars().next().unwrap_or('?');

    rsx! {
        div {
            class: "flex items-center gap-3 p-2 rounded-lg hover:bg-accent/50 transition",
            div { class: "flex-shrink-0",
                div { class: "w-8 h-8 rounded-full bg-muted flex items-center justify-center text-xs font-semibold text-muted-foreground overflow-hidden",
                    if let Some(url) = profile.read().as_ref().and_then(|p| p.picture.clone()).filter(|u| !u.is_empty()) {
                        img {
                            class: "w-full h-full object-cover",
                            src: "{url}",
                            loading: "lazy",
                        }
                    } else {
                        "{initial}"
                    }
                }
            }
            div { class: "flex-1",
                span { class: "text-sm text-foreground", "{display_name}" }
                if is_self {
                    span { class: "text-xs text-muted-foreground ml-2", "(you)" }
                }
            }
            if is_admin {
                MemberRoleBadge { role }
            }
            if can_kick {
                {
                    let relay = relay_url.clone();
                    let gid = group_id.clone();
                    let pk = pubkey.clone();
                    rsx! {
                        button {
                            class: "text-xs px-2 py-1 rounded bg-red-500/20 hover:bg-red-500/30 transition text-red-600 dark:text-red-400",
                            onclick: move |_| {
                                let relay = relay.clone();
                                let gid = gid.clone();
                                let pk = pk.clone();
                                spawn(async move {
                                    let _ = remove_user_from_group(&relay, &gid, &pk).await;
                                });
                            },
                            "Remove"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MemberRoleBadge(role: String) -> Element {
    let class = if role == "king" || role == "owner" || role == "admin" {
        "text-xs px-2 py-0.5 rounded bg-yellow-500/20 text-yellow-600 dark:text-yellow-400"
    } else if role == "bishop" || role == "moderator" || role == "mod" {
        "text-xs px-2 py-0.5 rounded bg-blue-500/20 text-blue-600 dark:text-blue-400"
    } else {
        "text-xs px-2 py-0.5 rounded bg-muted text-muted-foreground"
    };

    rsx! {
        span { class: "{class}", "{role}" }
    }
}
