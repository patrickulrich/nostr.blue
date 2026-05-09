use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::rooms::room_hero::RoomHero;
use crate::hooks::blobbi::use_blobbi_sleep::{put_to_sleep, wake_up};
use crate::stores::blobbi_store;

#[component]
pub fn RestRoom(blobbi: BlobbiCompanion) -> Element {
    let toggling = use_signal(|| false);
    let sleeping = blobbi.is_sleeping();
    let is_egg = blobbi.is_egg();

    let reaction = crate::components::blobbi::rooms::reaction_state::reaction_string();

    rsx! {
        div { class: "flex flex-col min-h-full",
            RoomHero { blobbi: blobbi.clone(), reaction: reaction.clone() }

            // Center content
            div { class: "flex-1 flex flex-col items-center justify-center gap-4 px-4",
                div { class: "text-sm text-muted-foreground text-center",
                    if sleeping {
                        "Your Blobbi is sleeping peacefully..."
                    } else {
                        "Put your Blobbi to bed to restore energy"
                    }
                }

                if sleeping {
                    div { class: "text-4xl animate-[blobbi-sleep-breathe_3s_ease-in-out_infinite]",
                        "💤"
                    }
                    span { class: "text-xs text-green-500",
                        "Energy regenerating"
                    }
                }

                span { class: "text-xs text-muted-foreground",
                    "Energy: {blobbi.stats.energy:.0}/100"
                }
            }

            // Bottom area — large sleep/wake button
            div { class: "px-4 pb-6 flex justify-center",
                button {
                    class: if is_egg {
                        "flex flex-col items-center gap-2 px-10 py-5 rounded-2xl bg-muted/30 border border-border/30 text-muted-foreground/40 cursor-not-allowed"
                    } else if sleeping {
                        "flex flex-col items-center gap-2 px-10 py-5 rounded-2xl bg-amber-500/10 border border-amber-500/20 text-amber-500 hover:bg-amber-500/20 transition active:scale-95"
                    } else {
                        "flex flex-col items-center gap-2 px-10 py-5 rounded-2xl bg-indigo-500/10 border border-indigo-500/20 text-indigo-500 hover:bg-indigo-500/20 transition active:scale-95"
                    },
                    disabled: is_egg || toggling(),
                    onclick: {
                        let mut toggling = toggling;
                        move |_| {
                            toggling.set(true);
                            spawn(async move {
                                if let Some(blobbi) = blobbi_store::get_selected_blobbi() {
                                    let result = if blobbi.is_sleeping() {
                                        wake_up(&blobbi).await
                                    } else {
                                        put_to_sleep(&blobbi).await
                                    };
                                    match result {
                                        Ok(updated) => blobbi_store::update_blobbi_in_collection(&updated),
                                        Err(e) => log::error!("Sleep toggle failed: {}", e),
                                    }
                                }
                                toggling.set(false);
                            });
                        }
                    },

                    if toggling() {
                        div { class: "size-10 border-2 border-current border-t-transparent rounded-full animate-spin" }
                    } else if sleeping {
                        // Sun icon — wake up
                        svg { class: "size-10", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
                            circle { cx: "12", cy: "12", r: "5" }
                            path { d: "M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" }
                        }
                    } else {
                        // Moon icon — sleep
                        svg { class: "size-10", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
                            path { d: "M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" }
                        }
                    }

                    span { class: "text-sm font-medium",
                        if is_egg {
                            "Eggs don't sleep"
                        } else if sleeping {
                            "Wake Up"
                        } else {
                            "Go to Sleep"
                        }
                    }
                }
            }
        }
    }
}
