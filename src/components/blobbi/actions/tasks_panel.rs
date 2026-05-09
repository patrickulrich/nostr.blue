use dioxus::prelude::*;

use crate::components::blobbi::actions::hatch_tasks;
use crate::components::blobbi::actions::stage_transition;
use crate::components::blobbi::core::types::BlobbiCompanion;

#[component]
pub fn TasksPanel(blobbi: BlobbiCompanion) -> Element {
    let defs = hatch_tasks::tasks_for_stage(blobbi.stage);
    let can_evolve = stage_transition::can_transition(&blobbi);
    let completed_count = blobbi.tasks.iter().filter(|t| t.completed).count();

    rsx! {
        div { class: "px-4 mt-4",
            div { class: "flex items-center justify-between mb-2",
                span { class: "text-sm font-medium",
                    if blobbi.is_egg() { "Hatch Tasks" } else if blobbi.is_baby() { "Evolve Tasks" } else { "Tasks" }
                }
                span { class: "text-[10px] text-muted-foreground",
                    "{completed_count}/{defs.len()} done"
                }
            }

            div { class: "space-y-2",
                for def in &defs {
                    {render_task(def, &blobbi)}
                }
            }

            if can_evolve {
                div { class: "text-center mt-2",
                    span { class: "text-[10px] text-green-500",
                        if blobbi.is_egg() { "All tasks complete! Go to Hatch Now!" } else { "All tasks complete! Ready to evolve!" }
                    }
                }
            }
        }
    }
}

fn render_task(def: &hatch_tasks::TaskDefinition, blobbi: &BlobbiCompanion) -> Element {
    let completed = hatch_tasks::is_task_completed(blobbi, def.id);
    let task = blobbi.tasks.iter().find(|t| t.id == def.id);
    let current = task.map(|t| t.progress).unwrap_or(0);
    let target = def.target;

    let has_action = !completed && matches!(def.id, crate::utils::nip_bb::constants::TASK_FIRST_POST | crate::utils::nip_bb::constants::TASK_POST_BLOBBI_PHOTO);

    let action_id = def.id.to_string();

    rsx! {
        div {
            class: if completed {
                "flex items-center gap-3 p-2.5 rounded-lg bg-green-500/10 border border-green-500/20"
            } else {
                "flex items-center gap-3 p-2.5 rounded-lg bg-card border border-border"
            },
            span { class: "text-lg", "{def.icon}" }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    span { class: "text-xs font-medium", "{def.name}" }
                    if completed {
                        span { class: "text-[10px] text-green-500", "\u{2713}" }
                    }
                }
                if !completed {
                    p { class: "text-[10px] text-muted-foreground mt-0.5", "{def.description}" }
                }
                div { class: "w-full h-1.5 bg-muted rounded-full overflow-hidden mt-1",
                    div {
                        class: if completed { "h-full bg-green-500 rounded-full" } else { "h-full bg-blue-500 rounded-full" },
                        style: "width: {((current as f64 / target as f64) * 100.0).min(100.0):.0}%",
                    }
                }
            }
            div { class: "flex flex-col items-end gap-1",
                span { class: "text-[10px] text-muted-foreground",
                    "{current}/{target}"
                }
                if has_action {
                    {
                        let nav = navigator();
                        rsx! {
                            button {
                                class: "text-[9px] px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-400 hover:bg-blue-500/30 transition",
                                onclick: move |_| {
                                    nav.push(crate::routes::Route::NoteNew { quote: None });
                                },
                                if action_id == crate::utils::nip_bb::constants::TASK_FIRST_POST {
                                    "Post"
                                } else {
                                    "Post Photo"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
