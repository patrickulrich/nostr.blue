use crate::components::blobbi::core::types::{BlobbiCompanion, BlobbiTaskProgress};
use crate::utils::nip_bb::constants::*;
use crate::utils::nip_bb::BlobbiStage;

#[derive(Clone, Debug, PartialEq)]
pub struct TaskDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub target: u32,
    pub is_dynamic: bool,
}

pub fn egg_tasks() -> Vec<TaskDefinition> {
    vec![
        TaskDefinition {
            id: TASK_FIRST_POST,
            name: "First #Blobbi Post",
            description: "Publish your first kind:1 post with #Blobbi hashtag",
            icon: "\u{1F4DD}",
            target: 1,
            is_dynamic: false,
        },
        TaskDefinition {
            id: TASK_POST_BLOBBI_PHOTO,
            name: "Post a Blobbi Photo",
            description: "Post a photo of your Blobbi (image in kind:1)",
            icon: "\u{1F4F8}",
            target: 1,
            is_dynamic: false,
        },
        TaskDefinition {
            id: TASK_INTERACT_7,
            name: "Interact 7 Times",
            description: "Perform 7 care interactions with your egg",
            icon: "\u{1F446}",
            target: 7,
            is_dynamic: false,
        },
        TaskDefinition {
            id: TASK_SHARE_YOUR_EGG,
            name: "Share Your Egg",
            description: "Post mentioning your egg with #BlobbiEgg hashtag",
            icon: "\u{1F31F}",
            target: 1,
            is_dynamic: false,
        },
    ]
}

pub fn baby_quests() -> Vec<TaskDefinition> {
    vec![
        TaskDefinition {
            id: QUEST_PUBLISH_5_POSTS,
            name: "Publish 5 Posts",
            description: "Post 5 kind:1 events authored by you",
            icon: "\u{1F4DD}",
            target: 5,
            is_dynamic: false,
        },
        TaskDefinition {
            id: QUEST_SHARE_SONG,
            name: "Share a Song",
            description: "Post a kind:1 event with a YouTube link",
            icon: "\u{1F3B5}",
            target: 1,
            is_dynamic: false,
        },
        TaskDefinition {
            id: QUEST_USE_BLOBBI_HASHTAGS,
            name: "Use Blobbi Hashtags",
            description: "Post with #Blobbi and #Evolving<Name>",
            icon: "\u{1F3F7}\u{FE0F}",
            target: 1,
            is_dynamic: false,
        },
        TaskDefinition {
            id: QUEST_MENTION_USER,
            name: "Mention a User",
            description: "Post tagging another user with p tag",
            icon: "\u{1F4AC}",
            target: 1,
            is_dynamic: false,
        },
        TaskDefinition {
            id: QUEST_REPLY_TO_POST,
            name: "Reply to a Post",
            description: "Post a reply with e and p tags",
            icon: "\u{1F4AC}",
            target: 1,
            is_dynamic: false,
        },
        TaskDefinition {
            id: QUEST_FOLLOW_5_USERS,
            name: "Follow 5 Users",
            description: "Send a kind:3 event with at least 5 p tags",
            icon: "\u{1F91D}",
            target: 5,
            is_dynamic: false,
        },
        TaskDefinition {
            id: QUEST_REACT_TO_5_POSTS,
            name: "React to 5 Posts",
            description: "Send 5 unique kind:7 reaction events",
            icon: "\u{2764}\u{FE0F}",
            target: 5,
            is_dynamic: false,
        },
        TaskDefinition {
            id: QUEST_REPOST_3_POSTS,
            name: "Repost 3 Posts",
            description: "Send 3 kind:6 repost events",
            icon: "\u{1F504}",
            target: 3,
            is_dynamic: false,
        },
        TaskDefinition {
            id: QUEST_REACT_OR_REPOST_BLOBBI,
            name: "React/Repost #Blobbi",
            description: "React to or repost a #Blobbi post",
            icon: "\u{2B50}",
            target: 1,
            is_dynamic: false,
        },
        TaskDefinition {
            id: QUEST_MAINTAIN_STATS,
            name: "Peak Condition",
            description: "Keep all stats above 80",
            icon: "\u{1F4AA}",
            target: EVOLVE_STAT_THRESHOLD as u32,
            is_dynamic: true,
        },
        TaskDefinition {
            id: QUEST_EDIT_PROFILE,
            name: "Edit Your Profile",
            description: "Update your profile info or customize your profile tabs",
            icon: "\u{270F}\u{FE0F}",
            target: 1,
            is_dynamic: false,
        },
    ]
}

pub fn tasks_for_stage(stage: BlobbiStage) -> Vec<TaskDefinition> {
    match stage {
        BlobbiStage::Egg => egg_tasks(),
        BlobbiStage::Baby => baby_quests(),
        BlobbiStage::Adult => vec![],
    }
}

pub fn is_task_completed(blobbi: &BlobbiCompanion, task_id: &str) -> bool {
    if task_id == QUEST_MAINTAIN_STATS {
        return blobbi.stats.hunger >= EVOLVE_STAT_THRESHOLD
            && blobbi.stats.happiness >= EVOLVE_STAT_THRESHOLD
            && blobbi.stats.health >= EVOLVE_STAT_THRESHOLD
            && blobbi.stats.hygiene >= EVOLVE_STAT_THRESHOLD
            && blobbi.stats.energy >= EVOLVE_STAT_THRESHOLD;
    }
    blobbi.tasks.iter().any(|t| t.id == task_id && t.completed)
}

pub fn all_tasks_completed(blobbi: &BlobbiCompanion) -> bool {
    let defs = tasks_for_stage(blobbi.stage);
    if defs.is_empty() {
        return false;
    }
    defs.iter().all(|t| is_task_completed(blobbi, t.id))
}

pub fn update_task_progress(blobbi: &mut BlobbiCompanion, action_name: &str) {
    let defs = tasks_for_stage(blobbi.stage);
    for def in &defs {
        if def.is_dynamic {
            continue;
        }

        if action_name == def.id || matches_interaction_action(action_name, def.id) {
            if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == def.id) {
                if !task.completed {
                    task.progress = task.progress.saturating_add(1);
                    if task.progress >= task.target {
                        task.completed = true;
                    }
                }
            }
        }
    }
}

fn matches_interaction_action(action: &str, task_id: &str) -> bool {
    if task_id == TASK_INTERACT_7 {
        return ["clean", "medicine", "sing", "play_music", "feed", "play"].contains(&action);
    }
    false
}

pub fn task_progress_summary(blobbi: &BlobbiCompanion) -> (u32, u32) {
    let defs = tasks_for_stage(blobbi.stage);
    let completed = defs
        .iter()
        .filter(|d| is_task_completed(blobbi, d.id))
        .count() as u32;
    (completed, defs.len() as u32)
}

pub fn initialize_tasks_for_stage(blobbi: &mut BlobbiCompanion) {
    let defs = tasks_for_stage(blobbi.stage);
    blobbi.tasks = defs
        .into_iter()
        .filter(|d| !d.is_dynamic)
        .map(|d| {
            let existing = blobbi.tasks.iter().find(|t| t.id == d.id);
            BlobbiTaskProgress {
                id: d.id.to_string(),
                completed: existing.map(|t| t.completed).unwrap_or(false),
                progress: existing.map(|t| t.progress).unwrap_or(0),
                target: d.target,
            }
        })
        .collect();
}
