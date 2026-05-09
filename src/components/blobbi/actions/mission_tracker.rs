use crate::components::blobbi::actions::action_types::BlobbiActionType;
use crate::components::blobbi::actions::missions_modal::build_active_missions;
use crate::components::blobbi::core::types::BlobbiTaskProgress;
use crate::stores::blobbi_store;

pub fn track_mission_progress(action: BlobbiActionType) {
    let Some(mut blobbi) = blobbi_store::get_selected_blobbi() else {
        return;
    };

    let missions = build_active_missions(&blobbi);
    let action_str = action.as_str();
    let daily_task_id = format!("daily_{}", action_str);
    let mut changed = false;

    let interact_actions = crate::components::blobbi::actions::missions_pool::INTERACT_ACTIONS;
    let is_interact = interact_actions.contains(&action_str);

    for mission in &missions {
        let def = crate::components::blobbi::actions::missions_pool::get_mission_by_id(&mission.id);

        let matches = def.map(|d| d.matches_actions.contains(&action_str)).unwrap_or(false);

        if !matches {
            continue;
        }

        if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == mission.id) {
            if !task.completed {
                task.progress = task.progress.saturating_add(1);
                if task.progress >= task.target {
                    task.completed = true;
                }
                changed = true;
            }
        } else {
            let target = def.map(|d| d.required_count).unwrap_or(1);
            blobbi.tasks.push(BlobbiTaskProgress {
                id: mission.id.clone(),
                completed: target <= 1,
                progress: 1,
                target,
            });
            changed = true;
        }
    }

    if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == daily_task_id) {
        if !task.completed {
            task.progress = task.progress.saturating_add(1);
            changed = true;
        }
    } else {
        blobbi.tasks.push(BlobbiTaskProgress {
            id: daily_task_id,
            completed: false,
            progress: 1,
            target: 999,
        });
        changed = true;
    }

    if is_interact {
        if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == "daily_interact") {
            if !task.completed {
                task.progress = task.progress.saturating_add(1);
                changed = true;
            }
        } else {
            blobbi.tasks.push(BlobbiTaskProgress {
                id: "daily_interact".to_string(),
                completed: false,
                progress: 1,
                target: 999,
            });
            changed = true;
        }
    }

    if changed {
        blobbi_store::update_blobbi_in_collection(&blobbi);
    }
}

pub fn track_photo() {
    let Some(mut blobbi) = blobbi_store::get_selected_blobbi() else {
        return;
    };

    let missions = build_active_missions(&blobbi);
    let daily_task_id = "daily_take_photo";
    let mut changed = false;

    if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == daily_task_id) {
        if !task.completed {
            task.progress = task.progress.saturating_add(1);
            changed = true;
        }
    } else {
        blobbi.tasks.push(BlobbiTaskProgress {
            id: daily_task_id.to_string(),
            completed: false,
            progress: 1,
            target: 999,
        });
        changed = true;
    }

    for mission in &missions {
        let def = crate::components::blobbi::actions::missions_pool::get_mission_by_id(&mission.id);
        let matches = def
            .map(|d| d.matches_actions.contains(&"take_photo"))
            .unwrap_or(false);
        if !matches {
            continue;
        }
        if let Some(task) = blobbi.tasks.iter_mut().find(|t| t.id == mission.id) {
            if !task.completed {
                task.progress = task.progress.saturating_add(1);
                if task.progress >= task.target {
                    task.completed = true;
                }
                changed = true;
            }
        } else {
            let target = crate::components::blobbi::actions::missions_pool::get_mission_by_id(&mission.id)
                .map(|d| d.required_count)
                .unwrap_or(1);
            blobbi.tasks.push(BlobbiTaskProgress {
                id: mission.id.clone(),
                completed: target <= 1,
                progress: 1,
                target,
            });
            changed = true;
        }
    }

    if changed {
        blobbi_store::update_blobbi_in_collection(&blobbi);
    }
}
