use crate::components::groups::system_message::GroupSystemMessage;
use crate::components::rich_content::RichContent;
use crate::stores::auth_store;
use crate::stores::profiles;
use crate::stores::social::group_store::{
    delete_group_event, edit_group_message, is_pinned, send_group_message, send_group_reaction,
    toggle_pin_message, GroupMessage,
};
use crate::utils::time;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

const QUICK_EMOJIS: &[&str] = &["❤️", "👍", "😂", "🔥", "👏"];

const PUBKEY_COLORS: &[&str] = &[
    "text-red-500", "text-orange-500", "text-amber-500", "text-yellow-500",
    "text-lime-500", "text-green-500", "text-emerald-500", "text-teal-500",
    "text-cyan-500", "text-sky-500", "text-blue-500", "text-indigo-500",
    "text-violet-500", "text-purple-500", "text-fuchsia-500", "text-pink-500",
    "text-rose-500",
];

fn pubkey_color(pubkey: &str) -> &'static str {
    let hash = pubkey.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    PUBKEY_COLORS[(hash as usize) % PUBKEY_COLORS.len()]
}

#[component]
pub fn GroupChatView(
    messages: Vec<GroupMessage>,
    is_admin: bool,
    relay_url: String,
    group_id: String,
    sentinel_id: String,
    has_more: bool,
    pagination_loading: bool,
    on_reply: EventHandler<GroupMessage>,
    on_edit: EventHandler<GroupMessage>,
    on_delete: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "flex-1 overflow-y-auto p-4 space-y-3 flex flex-col-reverse",
            for msg in messages {
                if msg.is_system {
                    GroupSystemMessage {
                        key: "{msg.id}",
                        message: msg.clone(),
                    }
                } else {
                    GroupMessageItem {
                        key: "{msg.id}",
                        message: msg.clone(),
                        is_admin,
                        relay_url: relay_url.clone(),
                        group_id: group_id.clone(),
                        on_reply,
                        on_edit,
                        on_delete,
                    }
                }
            }
            if has_more {
                div {
                    id: "{sentinel_id}",
                    class: "py-2 flex justify-center",
                    if pagination_loading {
                        span { class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin text-muted-foreground" }
                    }
                }
            }
        }
    }
}

#[component]
fn GroupMessageItem(
    message: GroupMessage,
    is_admin: bool,
    relay_url: String,
    group_id: String,
    on_reply: EventHandler<GroupMessage>,
    on_edit: EventHandler<GroupMessage>,
    on_delete: EventHandler<String>,
) -> Element {
    let ts = time::format_time_ago(message.created_at);
    let mut show_actions = use_signal(|| false);
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let author_pk = message.author.clone();

    {
        let pk = author_pk.clone();
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
        .unwrap_or_else(|| truncate_pubkey(&author_pk));
    let color_class = pubkey_color(&author_pk);
    let initial = display_name.chars().next().unwrap_or('?');

    rsx! {
        div { class: "flex gap-3 group/msg",
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
            div { class: "flex-1 min-w-0",
                div { class: "flex items-baseline gap-2",
                    span { class: "text-sm font-semibold {color_class}", "{display_name}" }
                    span { class: "text-xs text-muted-foreground", "{ts}" }
                    button {
                        class: "text-xs text-muted-foreground opacity-0 group-hover/msg:opacity-100 transition hover:text-foreground ml-auto",
                        onclick: move |_| show_actions.set(!show_actions()),
                        "..."
                    }
                }
                if let Some(reply_id) = &message.reply_to {
                    div { class: "text-xs text-muted-foreground mb-1 border-l-2 border-primary/30 pl-2",
                        "Reply to {truncate_pubkey(reply_id)}"
                    }
                }
                div { class: "text-sm text-foreground",
                    RichContent {
                        content: message.content.clone(),
                        tags: message.event.tags.iter().cloned().collect(),
                    }
                }
                if !message.reactions.is_empty() {
                    div { class: "flex flex-wrap gap-1 mt-1",
                        for (emoji, pubkeys) in message.reactions.iter() {
                            {
                                let my_pk = auth_store::get_pubkey();
                                let is_mine = my_pk.as_ref().map(|pk| pubkeys.contains(pk)).unwrap_or(false);
                                let count = pubkeys.len();
                                let border = if is_mine { "border border-primary/30" } else { "border border-border" };
                                let bg = if is_mine { "bg-primary/10" } else { "bg-accent/50" };
                                rsx! {
                                    span {
                                        key: "{emoji}",
                                        class: "text-xs px-1.5 py-0.5 rounded-full {border} {bg} cursor-default",
                                        "{emoji} {count}"
                                    }
                                }
                            }
                        }
                    }
                }
                if show_actions() {
                    div { class: "flex gap-2 mt-1",
                        for emoji in QUICK_EMOJIS {
                            {
                                let relay = relay_url.clone();
                                let gid = group_id.clone();
                                let eid = message.id.clone();
                                let author = message.author.clone();
                                let emoji_str = emoji.to_string();
                                rsx! {
                                    button {
                                        key: "{emoji_str}",
                                        class: "text-xs px-1.5 py-0.5 rounded bg-accent hover:bg-accent/80 transition",
                                        onclick: move |_| {
                                            let relay = relay.clone();
                                            let gid = gid.clone();
                                            let eid = eid.clone();
                                            let author = author.clone();
                                            let emoji = emoji_str.clone();
                                            spawn(async move {
                                                let _ = send_group_reaction(&relay, &gid, &eid, &author, &emoji).await;
                                            });
                                        },
                                        "{emoji_str}"
                                    }
                                }
                            }
                        }
                        button {
                            class: "text-xs px-2 py-1 rounded bg-accent hover:bg-accent/80 transition text-foreground",
                            onclick: {
                                let msg = message.clone();
                                move |_| on_reply.call(msg.clone())
                            },
                            "Reply"
                        }
                        {
                            let my_pk = auth_store::get_pubkey();
                            let is_own = my_pk.as_ref().map(|pk| pk == &message.author).unwrap_or(false);
                            if is_own {
                                let msg = message.clone();
                                rsx! {
                                    button {
                                        class: "text-xs px-2 py-1 rounded bg-accent hover:bg-accent/80 transition text-foreground",
                                        onclick: move |_| on_edit.call(msg.clone()),
                                        "Edit"
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                        if is_admin {
                            {
                                let relay = relay_url.clone();
                                let gid = group_id.clone();
                                let eid = message.id.clone();
                                let pinned = is_pinned(&relay_url, &group_id, &message.id);
                                rsx! {
                                    if pinned {
                                        span { class: "text-xs px-2 py-1 rounded bg-primary/20 text-primary", "📌" }
                                    }
                                    button {
                                        class: "text-xs px-2 py-1 rounded bg-accent hover:bg-accent/80 transition text-foreground",
                                        onclick: {
                                            let relay = relay.clone();
                                            let gid = gid.clone();
                                            let eid = eid.clone();
                                            move |_| {
                                                let relay = relay.clone();
                                                let gid = gid.clone();
                                                let eid = eid.clone();
                                                toggle_pin_message(&relay, &gid, &eid);
                                            }
                                        },
                                        if pinned { "Unpin" } else { "Pin" }
                                    }
                                    button {
                                        class: "text-xs px-2 py-1 rounded bg-red-500/20 hover:bg-red-500/30 transition text-red-600 dark:text-red-400",
                                        onclick: move |_| {
                                            let relay = relay.clone();
                                            let gid = gid.clone();
                                            let eid = eid.clone();
                                            on_delete.call(eid.clone());
                                            spawn(async move {
                                                let _ = delete_group_event(&relay, &gid, &eid).await;
                                            });
                                        },
                                        "Delete"
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

#[component]
pub fn GroupChatComposer(
    relay_url: String,
    group_id: String,
    reply_to: Signal<Option<GroupMessage>>,
    editing: Signal<Option<GroupMessage>>,
) -> Element {
    let mut content = use_signal(String::new);
    let mut sending = use_signal(|| false);

    {
        let editing_msg = editing;
        use_effect(move || {
            if let Some(msg) = editing_msg.read().as_ref() {
                content.set(msg.content.clone());
            }
        });
    }

    let is_editing = editing.read().is_some();

    let relay_for_key = relay_url.clone();
    let group_for_key = group_id.clone();
    let relay_for_btn = relay_url.clone();
    let group_for_btn = group_id.clone();

    rsx! {
        div { class: "border-t border-border p-3",
            if let Some(reply) = reply_to.as_ref() {
                div { class: "flex items-center gap-2 mb-2 text-xs text-muted-foreground",
                    span { "Replying to {truncate_pubkey(&reply.author)}" }
                    button {
                        class: "ml-auto text-muted-foreground hover:text-foreground",
                        onclick: move |_| reply_to.set(None),
                        "x"
                    }
                }
            }
            if let Some(msg) = editing.as_ref() {
                div { class: "flex items-center gap-2 mb-2 text-xs text-primary",
                    span { "Editing message" }
                    span { class: "text-muted-foreground truncate max-w-48", "{truncate_pubkey(&msg.id)}" }
                    button {
                        class: "ml-auto text-muted-foreground hover:text-foreground",
                        onclick: move |_| {
                            editing.set(None);
                            content.set(String::new());
                        },
                        "x"
                    }
                }
            }
            div { class: "flex gap-2",
                input {
                    class: "flex-1 px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary text-sm",
                    r#type: "text",
                    placeholder: if is_editing { "Edit message..." } else { "Message..." },
                    value: "{content}",
                    oninput: move |e| content.set(e.value()),
                    onkeypress: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter && !content().is_empty() && !*sending.read() {
                            let relay = relay_for_key.clone();
                            let grp = group_for_key.clone();
                            let msg_text = content();
                            content.set(String::new());
                            sending.set(true);
                            let edit_msg = editing();
                            if let Some(em) = edit_msg {
                                let eid = em.id.clone();
                                editing.set(None);
                                spawn(async move {
                                    let _ = edit_group_message(&relay, &grp, &eid, &msg_text).await;
                                    sending.set(false);
                                });
                            } else {
                                let reply = reply_to.as_ref().map(|r| r.id.clone());
                                reply_to.set(None);
                                spawn(async move {
                                    let _ = send_group_message(&relay, &grp, &msg_text, reply.as_deref()).await;
                                    sending.set(false);
                                });
                            }
                        }
                    },
                }
                button {
                    class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50 text-sm",
                    disabled: content().is_empty() || *sending.read(),
                    onclick: move |_| {
                        if !content().is_empty() && !*sending.read() {
                            let relay = relay_for_btn.clone();
                            let grp = group_for_btn.clone();
                            let msg_text = content();
                            content.set(String::new());
                            sending.set(true);
                            let edit_msg = editing();
                            if let Some(em) = edit_msg {
                                let eid = em.id.clone();
                                editing.set(None);
                                spawn(async move {
                                    let _ = edit_group_message(&relay, &grp, &eid, &msg_text).await;
                                    sending.set(false);
                                });
                            } else {
                                let reply = reply_to.as_ref().map(|r| r.id.clone());
                                reply_to.set(None);
                                spawn(async move {
                                    let _ = send_group_message(&relay, &grp, &msg_text, reply.as_deref()).await;
                                    sending.set(false);
                                });
                            }
                        }
                    },
                    if *sending.read() { "..." } else if is_editing { "Save" } else { "Send" }
                }
            }
        }
    }
}
