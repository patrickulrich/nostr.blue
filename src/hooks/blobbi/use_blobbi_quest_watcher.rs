use dioxus::prelude::*;
use nostr_sdk::Filter;

use crate::components::blobbi::actions::hatch_tasks;
use crate::components::blobbi::core::builders::publish_blobbi_state;
use crate::hooks::use_relay_subscription;
use crate::stores::blobbi_store;
use crate::utils::nip_bb::constants::*;

pub fn use_blobbi_quest_watcher() {
    let selected_d = {
        let store = blobbi_store::BLOBBI_COLLECTION.read();
        store.selected_d.clone()
    };

    let blobbi = selected_d.and_then(|d| {
        let store = blobbi_store::BLOBBI_COLLECTION.read();
        store.collection.iter().find(|b| b.d == d).cloned()
    });

    let pubkey = crate::stores::auth_store::get_pubkey();

    let (filter, is_baby) = match (&blobbi, &pubkey) {
        (Some(b), Some(pk)) if b.is_baby() => {
            let author = nostr_sdk::PublicKey::from_hex(pk).ok();
            match author {
                Some(a) => (
                    Some(
                        Filter::new()
                            .kind(nostr_sdk::Kind::TextNote)
                            .author(a)
                            .limit(20),
                    ),
                    true,
                ),
                None => (None, false),
            }
        }
        (Some(b), Some(pk)) if b.is_egg() => {
            let author = nostr_sdk::PublicKey::from_hex(pk).ok();
            match author {
                Some(a) => (
                    Some(
                        Filter::new()
                            .kind(nostr_sdk::Kind::TextNote)
                            .author(a)
                            .limit(20),
                    ),
                    false,
                ),
                None => (None, false),
            }
        }
        _ => (None, false),
    };

    let pk_for_cb = pubkey.clone();
    use_relay_subscription(filter, move |event| {
        if !is_baby && !blobbi_store::get_selected_blobbi().map(|b| b.is_egg()).unwrap_or(false) {
            return;
        }

        let pk = match &pk_for_cb {
            Some(p) => p.clone(),
            None => return,
        };

        if event.pubkey.to_hex() != pk {
            return;
        }

        let mut blobbi = match blobbi_store::get_selected_blobbi() {
            Some(b) => b,
            None => return,
        };

        if !blobbi.is_baby() && !blobbi.is_egg() {
            return;
        }

        let mut updated = false;

        let content_lower = event.content.to_lowercase();
        let has_t_blobbi = event.tags.iter().any(|t| {
            t.kind().to_string() == "t"
                && t.content().map(|v| v.to_lowercase()).as_deref() == Some("blobbi")
        });
        let has_blobbi_ref = content_lower.contains("#blobbi") || has_t_blobbi;

        if blobbi.is_egg() {
            if has_blobbi_ref && !hatch_tasks::is_task_completed(&blobbi, TASK_FIRST_POST) {
                if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == TASK_FIRST_POST) {
                    if !task.completed {
                        task.progress = 1;
                        task.completed = true;
                        updated = true;
                    }
                }
            }

            if !hatch_tasks::is_task_completed(&blobbi, TASK_POST_BLOBBI_PHOTO) {
                let has_image = event.content.contains("https://")
                    && (event.content.contains(".jpg")
                        || event.content.contains(".png")
                        || event.content.contains(".gif")
                        || event.content.contains(".webp")
                        || event.content.contains(".jpeg")
                        || event.tags.iter().any(|t| {
                            t.kind().to_string() == "image"
                                || (t.kind().to_string() == "m"
                                    && t.content()
                                        .map(|v| v.starts_with("image/"))
                                        .unwrap_or(false))
                        }));
                if has_image {
                    if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == TASK_POST_BLOBBI_PHOTO) {
                        if !task.completed {
                            task.progress = 1;
                            task.completed = true;
                            updated = true;
                        }
                    }
                }
            }
        }

        if blobbi.is_baby() {
            if !hatch_tasks::is_task_completed(&blobbi, QUEST_PUBLISH_5_POSTS) {
                if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == QUEST_PUBLISH_5_POSTS) {
                    if !task.completed {
                        task.progress = task.progress.saturating_add(1);
                        if task.progress >= task.target {
                            task.completed = true;
                        }
                        updated = true;
                    }
                }
            }

            if has_blobbi_ref && !hatch_tasks::is_task_completed(&blobbi, QUEST_USE_BLOBBI_HASHTAGS) {
                let evolving_tag = format!("#evolving{}", blobbi.name.to_lowercase());
                if content_lower.contains(&evolving_tag) {
                    if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == QUEST_USE_BLOBBI_HASHTAGS) {
                        if !task.completed {
                            task.progress = 1;
                            task.completed = true;
                            updated = true;
                        }
                    }
                }
            }

            if !hatch_tasks::is_task_completed(&blobbi, QUEST_SHARE_SONG) {
                let content = &event.content;
                let has_youtube = content.contains("youtube.com/watch")
                    || content.contains("youtu.be/")
                    || content.contains("youtube.com/embed/");
                if has_youtube {
                    if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == QUEST_SHARE_SONG) {
                        if !task.completed {
                            task.progress = 1;
                            task.completed = true;
                            updated = true;
                        }
                    }
                }
            }

            let has_p_tag = event.tags.iter().any(|t| t.kind().to_string() == "p");
            if has_p_tag && !hatch_tasks::is_task_completed(&blobbi, QUEST_MENTION_USER) {
                if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == QUEST_MENTION_USER) {
                    if !task.completed {
                        task.progress = 1;
                        task.completed = true;
                        updated = true;
                    }
                }
            }

            let has_e_tag = event.tags.iter().any(|t| t.kind().to_string() == "e");
            if has_e_tag && has_p_tag && !hatch_tasks::is_task_completed(&blobbi, QUEST_REPLY_TO_POST) {
                if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == QUEST_REPLY_TO_POST) {
                    if !task.completed {
                        task.progress = 1;
                        task.completed = true;
                        updated = true;
                    }
                }
            }
        }

        if updated {
            let blobbi = blobbi.clone();
            spawn(async move {
                if let Err(e) = publish_blobbi_state(&blobbi).await {
                    log::error!("Failed to publish quest progress: {}", e);
                } else {
                    blobbi_store::update_blobbi_in_collection(&blobbi);
                }
            });
        }
    });
}
