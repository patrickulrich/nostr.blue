use dioxus::prelude::*;

use crate::components::blobbi::actions::hatch_tasks;
use crate::components::blobbi::actions::stage_transition;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::stores::blobbi_store;

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
                button {
                    class: "w-full mt-3 py-2.5 bg-green-500 hover:bg-green-600 text-white rounded-xl font-medium transition text-sm",
                    onclick: move |_| {
                        let b = blobbi.clone();
                        spawn(async move {
                            let pubkey = crate::stores::auth_store::get_pubkey().unwrap_or_default();
                            let updated = stage_transition::transition_stage(&b, &pubkey);
                            match crate::components::blobbi::core::builders::publish_blobbi_state(&updated).await {
                                Ok(()) => blobbi_store::update_blobbi_in_collection(&updated),
                                Err(e) => log::error!("Transition failed: {}", e),
                            }
                        });
                    },
                    if blobbi.is_egg() {
                        "🥚 Hatch Now!"
                    } else {
                        "✨ Evolve Now!"
                    }
                }
            }
        }
    }
}

fn render_task(def: &hatch_tasks::TaskDefinition, blobbi: &BlobbiCompanion) -> Element {
    let task = blobbi.tasks.iter().find(|t| t.id == def.id);
    let completed = hatch_tasks::is_task_completed(blobbi, def.id);
    let current = task.map(|t| t.progress).unwrap_or(0);
    let target = def.target;

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
                div { class: "w-full h-1.5 bg-muted rounded-full overflow-hidden mt-1",
                    div {
                        class: if completed { "h-full bg-green-500 rounded-full" } else { "h-full bg-blue-500 rounded-full" },
                        style: "width: {((current as f64 / target as f64) * 100.0).min(100.0):.0}%",
                    }
                }
            }
            span { class: "text-[10px] text-muted-foreground",
                "{current}/{target}"
            }
        }
    }
}
