use dioxus::prelude::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CarouselEntry {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub meta: Option<String>,
}

#[component]
pub fn ItemCarousel(
    items: Vec<CarouselEntry>,
    on_use: EventHandler<String>,
    active_item_id: Option<String>,
    disabled: bool,
    on_focus_change: Option<EventHandler<CarouselEntry>>,
) -> Element {
    let mut index = use_signal(|| 0usize);
    let count = items.len();

    if count == 0 {
        return rsx! {
            div { class: "flex items-center justify-center h-[4.5rem] sm:h-[5.5rem]",
                p { class: "text-xs text-muted-foreground/50", "Nothing here yet" }
            }
        };
    }

    let current = &items[index()];
    let is_this_active = active_item_id.as_deref() == Some(&current.id);
    let show_previews = count >= 3;

    rsx! {
        div { class: "flex items-center justify-center",
            // Left arrow
            button {
                class: "size-7 sm:size-8 rounded-full flex items-center justify-center shrink-0 text-muted-foreground/40 hover:text-foreground/70 hover:bg-accent/40 transition-all duration-200 active:scale-90 disabled:opacity-30 disabled:pointer-events-none",
                disabled,
                onclick: {
                    let items = items.clone();
                    let on_focus_change = on_focus_change;
                    move |_| {
                        let n = (index() + count - 1) % count;
                        index.set(n);
                        if let Some(cb) = &on_focus_change {
                            cb.call(items[n].clone());
                        }
                    }
                },
                svg { class: "size-4", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
                    path { d: "M15 18l-6-6 6-6" }
                }
            }

            // Preview (prev) — desktop only
            if show_previews {
                {
                    let prev = &items[(index() + count - 1) % count];
                    rsx! {
                        div { class: "hidden sm:flex items-center justify-center w-10 h-12 shrink-0 overflow-hidden pointer-events-none select-none opacity-20",
                            span { class: "text-2xl leading-none block",
                                {prev.icon.clone().unwrap_or_else(|| prev.label.clone())}
                            }
                        }
                    }
                }
            }

            // Focused item
            button {
                class: if is_this_active {
                    "relative flex flex-col items-center justify-center shrink-0 overflow-hidden w-20 h-[4.5rem] sm:w-24 sm:h-[5.5rem] rounded-2xl transition-colors duration-200 bg-accent/40"
                } else {
                    "relative flex flex-col items-center justify-center shrink-0 overflow-hidden w-20 h-[4.5rem] sm:w-24 sm:h-[5.5rem] rounded-2xl transition-colors duration-200 hover:bg-accent/20 active:scale-95"
                },
                disabled: disabled && !is_this_active,
                onclick: {
                    let id = current.id.clone();
                    move |_| on_use.call(id.clone())
                },
                span { class: "text-4xl sm:text-5xl leading-none",
                    {current.icon.clone().unwrap_or_else(|| current.label.clone())}
                }
                span { class: "text-[10px] sm:text-xs font-medium text-foreground/70 mt-0.5 w-16 sm:w-20 text-center truncate",
                    "{current.label}"
                }
            }

            // Preview (next) — desktop only
            if show_previews {
                {
                    let next = &items[(index() + 1) % count];
                    rsx! {
                        div { class: "hidden sm:flex items-center justify-center w-10 h-12 shrink-0 overflow-hidden pointer-events-none select-none opacity-20",
                            span { class: "text-2xl leading-none block",
                                {next.icon.clone().unwrap_or_else(|| next.label.clone())}
                            }
                        }
                    }
                }
            }

            // Right arrow
            button {
                class: "size-7 sm:size-8 rounded-full flex items-center justify-center shrink-0 text-muted-foreground/40 hover:text-foreground/70 hover:bg-accent/40 transition-all duration-200 active:scale-90 disabled:opacity-30 disabled:pointer-events-none",
                disabled,
                onclick: {
                    let items = items.clone();
                    let on_focus_change = on_focus_change;
                    move |_| {
                        let n = (index() + 1) % count;
                        index.set(n);
                        if let Some(cb) = &on_focus_change {
                            cb.call(items[n].clone());
                        }
                    }
                },
                svg { class: "size-4", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
                    path { d: "M9 18l6-6-6-6" }
                }
            }
        }
    }
}
