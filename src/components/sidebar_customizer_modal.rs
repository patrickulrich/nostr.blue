//! Modal for customizing sidebar navigation layout
//! Supports drag-to-reorder and moving items between active/available pools

use dioxus::prelude::*;
use crate::stores::sidebar_store::{
    SidebarItem, SIDEBAR_ITEMS, SIDEBAR_SLOT_COUNT, DEFAULT_MAIN_SIDEBAR_SLOTS, MAX_MAIN_SIDEBAR_SLOTS,
    save_sidebar_preferences, default_sidebar_items, get_available_items
};
use crate::components::icons::{
    HomeIcon, CompassIcon, BookOpenIcon, BellIcon, MailIcon,
    BookmarkIcon, UserIcon, SettingsIcon, CameraIcon, VideoIcon, PinIcon,
    ShoppingBagIcon,
};

#[derive(Props, Clone, PartialEq)]
pub struct SidebarCustomizerModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn SidebarCustomizerModal(props: SidebarCustomizerModalProps) -> Element {
    // Local editing state
    let mut local_active_items = use_signal(|| SIDEBAR_ITEMS.read().clone());
    let mut local_slot_count = use_signal(|| *SIDEBAR_SLOT_COUNT.read());
    let mut saving = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    // Desktop drag state
    let mut dragging_from_active = use_signal(|| None::<usize>);
    let mut dragging_from_available = use_signal(|| None::<SidebarItem>);
    let mut drag_over_active_index = use_signal(|| None::<usize>);
    let mut drag_over_available = use_signal(|| false);
    let mut drag_over_active_end = use_signal(|| false);

    // Touch drag state (mobile)
    let mut touch_dragging_index = use_signal(|| None::<usize>);
    let _touch_dragging_item = use_signal(|| None::<SidebarItem>);
    let mut touch_over_index = use_signal(|| None::<usize>);
    let mut is_touch_dragging = use_signal(|| false);

    // Compute available items (items not in active list)
    let available_items = use_memo(move || {
        get_available_items(&local_active_items.read())
    });

    // Remove item from active list (move to available)
    let mut remove_from_active = move |index: usize| {
        local_active_items.with_mut(|items| {
            if index < items.len() {
                items.remove(index);
            }
        });
    };

    // Add item to active list from available (at end)
    let mut add_to_active = move |item: SidebarItem| {
        local_active_items.with_mut(|items| {
            if !items.contains(&item) {
                items.push(item);
            }
        });
    };

    // Add item to active list at specific position
    let mut add_to_active_at = move |item: SidebarItem, index: usize| {
        local_active_items.with_mut(|items| {
            if !items.contains(&item) {
                let insert_idx = index.min(items.len());
                items.insert(insert_idx, item);
            }
        });
    };

    // Handle reorder within active items
    let mut reorder_active = move |from_idx: usize, to_idx: usize| {
        local_active_items.with_mut(|items| {
            if from_idx != to_idx && from_idx < items.len() && to_idx <= items.len() {
                let item = items.remove(from_idx);
                let insert_idx = if from_idx < to_idx { to_idx - 1 } else { to_idx };
                let insert_idx = insert_idx.min(items.len());
                items.insert(insert_idx, item);
            }
        });
    };

    // Handle save
    let handle_save = move |_| {
        saving.set(true);
        error_msg.set(None);
        let items = local_active_items.read().clone();
        let slot_count = *local_slot_count.read();

        spawn(async move {
            match save_sidebar_preferences(items, slot_count).await {
                Ok(_) => {
                    props.on_close.call(());
                }
                Err(e) => {
                    error_msg.set(Some(e));
                    saving.set(false);
                }
            }
        });
    };

    // Handle backdrop click
    let handle_backdrop_click = move |_| {
        if !*saving.read() {
            props.on_close.call(());
        }
    };

    let active_count = local_active_items.read().len();
    let slot_count = *local_slot_count.read();
    let main_count = active_count.min(slot_count);
    let more_count = active_count.saturating_sub(slot_count);

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4",
            onclick: handle_backdrop_click,

            // Modal content
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-lg w-full p-6 max-h-[90vh] overflow-y-auto",
                onclick: |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between mb-4",
                    div {
                        class: "flex items-center gap-2",
                        SettingsIcon { class: "w-5 h-5 text-gray-500" }
                        h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                            "Customize Sidebar"
                        }
                    }
                    button {
                        class: "text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 text-xl font-bold",
                        onclick: move |_| props.on_close.call(()),
                        disabled: *saving.read(),
                        "×"
                    }
                }

                p {
                    class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                    "Drag to reorder items. Adjust how many appear in the main sidebar vs More menu."
                }

                // Slot count adjuster
                div {
                    class: "flex items-center gap-3 mb-4 p-2 bg-gray-100 dark:bg-gray-700 rounded-lg",
                    span {
                        class: "text-sm text-gray-700 dark:text-gray-300",
                        "Main sidebar items:"
                    }
                    div {
                        class: "flex items-center gap-1",
                        button {
                            class: "w-8 h-8 flex items-center justify-center rounded bg-gray-200 dark:bg-gray-600 \
                                    hover:bg-gray-300 dark:hover:bg-gray-500 disabled:opacity-50 transition",
                            disabled: *local_slot_count.read() <= 1,
                            onclick: move |_| {
                                local_slot_count.with_mut(|c| {
                                    if *c > 1 { *c -= 1; }
                                });
                            },
                            "−"
                        }
                        span {
                            class: "w-8 text-center font-medium text-gray-800 dark:text-gray-200",
                            "{slot_count}"
                        }
                        button {
                            class: "w-8 h-8 flex items-center justify-center rounded bg-gray-200 dark:bg-gray-600 \
                                    hover:bg-gray-300 dark:hover:bg-gray-500 disabled:opacity-50 transition",
                            disabled: *local_slot_count.read() >= MAX_MAIN_SIDEBAR_SLOTS,
                            onclick: move |_| {
                                local_slot_count.with_mut(|c| {
                                    if *c < MAX_MAIN_SIDEBAR_SLOTS { *c += 1; }
                                });
                            },
                            "+"
                        }
                    }
                    span {
                        class: "text-xs text-gray-500 dark:text-gray-400",
                        "(1-{MAX_MAIN_SIDEBAR_SLOTS})"
                    }
                }

                // Active Items Section
                div { class: "mb-6",
                    div { class: "flex items-center justify-between mb-2",
                        h4 { class: "font-medium text-gray-700 dark:text-gray-300",
                            "Active Items"
                        }
                        span {
                            class: "text-xs text-gray-500",
                            "{main_count} main + {more_count} more"
                        }
                    }

                    // Droppable active items container
                    div {
                        class: format!(
                            "flex flex-wrap gap-2 p-3 bg-gray-100 dark:bg-gray-700 rounded-lg min-h-[60px] transition-colors {}",
                            if *drag_over_active_end.read() && dragging_from_available().is_some() {
                                "ring-2 ring-blue-500"
                            } else { "" }
                        ),
                        ondragover: move |e| {
                            e.prevent_default();
                            drag_over_active_end.set(true);
                        },
                        ondragleave: move |_| {
                            drag_over_active_end.set(false);
                        },
                        ondrop: move |e| {
                            e.prevent_default();
                            // Handle drop from available to end of active
                            if let Some(item) = *dragging_from_available.read() {
                                add_to_active(item);
                            }
                            dragging_from_available.set(None);
                            dragging_from_active.set(None);
                            drag_over_active_end.set(false);
                        },

                        // Render active items with divider at slot count position
                        for (index, item) in local_active_items.read().iter().cloned().enumerate() {
                            Fragment {
                                key: "{index}-{item:?}",

                                // Visual divider between main sidebar and More menu
                                if index == *local_slot_count.read() {
                                    div {
                                        class: "w-full flex items-center gap-2 my-1",
                                        div { class: "flex-1 border-t border-dashed border-gray-400 dark:border-gray-500" }
                                        span {
                                            class: "text-xs text-gray-500 dark:text-gray-400 px-2",
                                            "More Menu"
                                        }
                                        div { class: "flex-1 border-t border-dashed border-gray-400 dark:border-gray-500" }
                                    }
                                }

                                // Sidebar item chip
                                div {
                                    id: "sidebar-item-{index}",
                                class: "relative group",
                                draggable: "true",

                                // Desktop drag events
                                ondragstart: move |e| {
                                    dragging_from_active.set(Some(index));
                                    dragging_from_available.set(None);
                                    let _ = e.data_transfer().set_data("text/plain", &format!("active:{}", index));
                                },
                                ondragend: move |_| {
                                    dragging_from_active.set(None);
                                    drag_over_active_index.set(None);
                                    drag_over_active_end.set(false);
                                },
                                ondragover: move |e| {
                                    e.prevent_default();
                                    drag_over_active_index.set(Some(index));
                                    drag_over_active_end.set(false);
                                },
                                ondragleave: move |_| {
                                    if drag_over_active_index() == Some(index) {
                                        drag_over_active_index.set(None);
                                    }
                                },
                                ondrop: move |e| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    // Reorder within active
                                    if let Some(from_idx) = *dragging_from_active.read() {
                                        reorder_active(from_idx, index);
                                    }
                                    // Add from available at this position
                                    if let Some(avail_item) = *dragging_from_available.read() {
                                        add_to_active_at(avail_item, index);
                                    }
                                    drag_over_active_index.set(None);
                                    dragging_from_active.set(None);
                                    dragging_from_available.set(None);
                                },

                                // Touch events for mobile
                                ontouchstart: move |_| {
                                    touch_dragging_index.set(Some(index));
                                    touch_over_index.set(Some(index));
                                    is_touch_dragging.set(true);
                                },
                                ontouchmove: move |e| {
                                    if !*is_touch_dragging.read() {
                                        return;
                                    }
                                    e.prevent_default();

                                    // Find element under touch point
                                    #[cfg(target_family = "wasm")]
                                    if let Some(touch) = e.touches().first() {
                                        let coords = touch.client_coordinates();
                                        let x = coords.x;
                                        let y = coords.y;

                                        if let Some(window) = web_sys::window() {
                                            if let Some(document) = window.document() {
                                                if let Some(element) = document.element_from_point(x as f32, y as f32) {
                                                    let mut current = Some(element);
                                                    while let Some(el) = current {
                                                        if let Some(id) = el.get_attribute("id") {
                                                            if id.starts_with("sidebar-item-") {
                                                                if let Ok(target_idx) = id.replace("sidebar-item-", "").parse::<usize>() {
                                                                    touch_over_index.set(Some(target_idx));
                                                                }
                                                                break;
                                                            }
                                                        }
                                                        current = el.parent_element();
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                ontouchend: move |_| {
                                    if !*is_touch_dragging.read() {
                                        return;
                                    }

                                    if let (Some(from_idx), Some(to_idx)) = (*touch_dragging_index.read(), *touch_over_index.read()) {
                                        if from_idx != to_idx {
                                            reorder_active(from_idx, to_idx);
                                        }
                                    }

                                    touch_dragging_index.set(None);
                                    touch_over_index.set(None);
                                    is_touch_dragging.set(false);
                                },
                                ontouchcancel: move |_| {
                                    touch_dragging_index.set(None);
                                    touch_over_index.set(None);
                                    is_touch_dragging.set(false);
                                },

                                // Item chip
                                div {
                                    class: format!(
                                        "px-3 py-2 bg-white dark:bg-gray-600 rounded-lg cursor-move \
                                         transition-all flex items-center gap-2 {} {}",
                                        if dragging_from_active() == Some(index) || touch_dragging_index() == Some(index) {
                                            "opacity-50 scale-95"
                                        } else { "opacity-100" },
                                        if (drag_over_active_index() == Some(index) && dragging_from_active() != Some(index)) ||
                                           (touch_over_index() == Some(index) && touch_dragging_index() != Some(index) && *is_touch_dragging.read()) {
                                            "ring-2 ring-blue-500 ring-offset-2"
                                        } else { "" }
                                    ),

                                    // Item icon
                                    {render_sidebar_icon(&item, "w-5 h-5")}

                                    // Item label
                                    span {
                                        class: "text-sm text-gray-700 dark:text-gray-200",
                                        "{item.label()}"
                                    }

                                    // Remove button (X)
                                    button {
                                        class: "ml-1 w-5 h-5 bg-red-500 hover:bg-red-600 text-white \
                                                rounded-full text-xs opacity-0 group-hover:opacity-100 \
                                                transition-opacity flex items-center justify-center",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            remove_from_active(index);
                                        },
                                        "×"
                                    }
                                }
                                }
                            }
                        }

                        // Empty state
                        if local_active_items.read().is_empty() {
                            div {
                                class: "w-full text-center text-gray-400 py-4",
                                "No items. Drag items from below to add them."
                            }
                        }
                    }
                }

                // Available Items Section
                div { class: "mb-4",
                    h4 {
                        class: "font-medium text-gray-700 dark:text-gray-300 mb-2",
                        "Available Items (Hidden)"
                    }

                    div {
                        class: format!(
                            "flex flex-wrap gap-2 p-3 bg-gray-50 dark:bg-gray-900 rounded-lg \
                             min-h-[40px] border-2 border-dashed {} transition-colors",
                            if *drag_over_available.read() || dragging_from_active().is_some() {
                                "border-blue-400"
                            } else {
                                "border-gray-300 dark:border-gray-600"
                            }
                        ),
                        ondragover: move |e| {
                            e.prevent_default();
                            drag_over_available.set(true);
                        },
                        ondragleave: move |_| {
                            drag_over_available.set(false);
                        },
                        ondrop: move |e| {
                            e.prevent_default();
                            // Move from active to available (hide it)
                            if let Some(from_idx) = *dragging_from_active.read() {
                                remove_from_active(from_idx);
                            }
                            drag_over_available.set(false);
                            dragging_from_active.set(None);
                        },

                        for item in available_items.read().iter().cloned() {
                            div {
                                key: "{item:?}-available",
                                class: "relative",
                                draggable: "true",

                                ondragstart: move |e| {
                                    dragging_from_available.set(Some(item));
                                    dragging_from_active.set(None);
                                    let _ = e.data_transfer().set_data("text/plain", &format!("available:{:?}", item));
                                },
                                ondragend: move |_| {
                                    dragging_from_available.set(None);
                                    drag_over_active_end.set(false);
                                },

                                div {
                                    class: format!(
                                        "px-3 py-2 bg-gray-200 dark:bg-gray-700 rounded-lg \
                                         cursor-move flex items-center gap-2 opacity-60 \
                                         hover:opacity-100 transition-opacity {}",
                                        if dragging_from_available() == Some(item) {
                                            "opacity-40 scale-95"
                                        } else { "" }
                                    ),

                                    {render_sidebar_icon(&item, "w-5 h-5")}
                                    span {
                                        class: "text-sm text-gray-600 dark:text-gray-300",
                                        "{item.label()}"
                                    }

                                    // Quick add button
                                    button {
                                        class: "ml-1 w-5 h-5 bg-green-500 hover:bg-green-600 \
                                                text-white rounded-full text-xs flex items-center \
                                                justify-center",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            add_to_active(item);
                                        },
                                        "+"
                                    }
                                }
                            }
                        }

                        if available_items.read().is_empty() {
                            div {
                                class: "w-full text-center text-gray-400 py-2 text-sm",
                                "All items are active. Drag items here to hide them."
                            }
                        }
                    }
                }

                // Error message
                if let Some(err) = error_msg.read().as_ref() {
                    div {
                        class: "bg-red-50 dark:bg-red-900/30 border border-red-200 \
                                dark:border-red-800 rounded p-3 mb-4",
                        p { class: "text-red-600 dark:text-red-400 text-sm", "{err}" }
                    }
                }

                // Footer buttons
                div {
                    class: "flex justify-between items-center pt-2 border-t \
                            border-gray-200 dark:border-gray-700",

                    button {
                        class: "text-sm text-gray-500 hover:text-gray-700 \
                                dark:hover:text-gray-300",
                        onclick: move |_| {
                            local_active_items.set(default_sidebar_items());
                            local_slot_count.set(DEFAULT_MAIN_SIDEBAR_SLOTS);
                        },
                        disabled: *saving.read(),
                        "Reset to defaults"
                    }

                    div { class: "flex gap-2",
                        button {
                            class: "px-4 py-2 text-gray-600 dark:text-gray-400 \
                                    hover:text-gray-800 dark:hover:text-gray-200",
                            onclick: move |_| props.on_close.call(()),
                            disabled: *saving.read(),
                            "Cancel"
                        }
                        button {
                            class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white \
                                    rounded font-medium disabled:opacity-50",
                            disabled: *saving.read() || local_active_items.read().is_empty(),
                            onclick: handle_save,
                            if *saving.read() { "Saving..." } else { "Save" }
                        }
                    }
                }
            }
        }
    }
}

/// Helper function to render the icon for a sidebar item
fn render_sidebar_icon(item: &SidebarItem, class: &str) -> Element {
    match item {
        SidebarItem::Home => rsx! { HomeIcon { class: class.to_string() } },
        SidebarItem::Explore => rsx! { CompassIcon { class: class.to_string() } },
        SidebarItem::Articles => rsx! { BookOpenIcon { class: class.to_string() } },
        SidebarItem::Music => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M9 18V5l12-2v13" }
                circle { cx: "6", cy: "18", r: "3" }
                circle { cx: "18", cy: "16", r: "3" }
            }
        },
        SidebarItem::Photos => rsx! { CameraIcon { class: class.to_string() } },
        SidebarItem::Videos => rsx! { VideoIcon { class: class.to_string() } },
        SidebarItem::Live => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                fill: "none",
                view_box: "0 0 24 24",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" }
            }
        },
        SidebarItem::Notifications => rsx! { BellIcon { class: class.to_string() } },
        SidebarItem::Messages => rsx! { MailIcon { class: class.to_string() } },
        SidebarItem::Bookmarks => rsx! { BookmarkIcon { class: class.to_string() } },
        SidebarItem::Profile => rsx! { UserIcon { class: class.to_string() } },
        SidebarItem::Settings => rsx! { SettingsIcon { class: class.to_string() } },
        SidebarItem::VoiceMessages => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" }
                path { d: "M19 10v2a7 7 0 0 1-14 0v-2" }
                line { x1: "12", x2: "12", y1: "19", y2: "22" }
            }
        },
        SidebarItem::Polls => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                line { x1: "3", y1: "9", x2: "21", y2: "9" }
                line { x1: "9", y1: "21", x2: "9", y2: "9" }
            }
        },
        SidebarItem::WebBookmarks => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                circle { cx: "12", cy: "12", r: "10" }
                line { x1: "2", y1: "12", x2: "22", y2: "12" }
                path { d: "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" }
            }
        },
        SidebarItem::Podcasts => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                circle { cx: "12", cy: "11", r: "1" }
                path { d: "M11 17a1 1 0 0 1 2 0c0 .5-.34 3-.5 4.5a.5.5 0 0 1-1 0c-.16-1.5-.5-4-.5-4.5Z" }
                path { d: "M8 14a5 5 0 1 1 8 0" }
                path { d: "M17 18.5a9 9 0 1 0-10 0" }
            }
        },
        SidebarItem::Radio => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M4.9 19.1C1 15.2 1 8.8 4.9 4.9" }
                path { d: "M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5" }
                circle { cx: "12", cy: "12", r: "2" }
                path { d: "M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5" }
                path { d: "M19.1 4.9C23 8.8 23 15.1 19.1 19" }
            }
        },
        SidebarItem::Wallet => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M21 12V7H5a2 2 0 0 1 0-4h14v4" }
                path { d: "M3 5v14a2 2 0 0 0 2 2h16v-5" }
                path { d: "M18 12a2 2 0 0 0 0 4h4v-4Z" }
            }
        },
        SidebarItem::P2PTrading => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M16 3h5v5" }
                path { d: "M21 3l-7 7" }
                path { d: "M8 21H3v-5" }
                path { d: "M3 21l7-7" }
            }
        },
        SidebarItem::Communities => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" }
                circle { cx: "9", cy: "7", r: "4" }
                path { d: "M23 21v-2a4 4 0 0 0-3-3.87" }
                path { d: "M16 3.13a4 4 0 0 1 0 7.75" }
            }
        },
        SidebarItem::Events => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z" }
                circle { cx: "12", cy: "10", r: "3" }
            }
        },
        SidebarItem::Calendar => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "3", y: "4", width: "18", height: "18", rx: "2", ry: "2" }
                line { x1: "16", y1: "2", x2: "16", y2: "6" }
                line { x1: "8", y1: "2", x2: "8", y2: "6" }
                line { x1: "3", y1: "10", x2: "21", y2: "10" }
            }
        },
        SidebarItem::Recipes => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M6 13.87A4 4 0 0 1 7.41 6a5.11 5.11 0 0 1 1.05-1.54 5 5 0 0 1 7.08 0A5.11 5.11 0 0 1 16.59 6 4 4 0 0 1 18 13.87V21H6Z" }
                line { x1: "6", y1: "17", x2: "18", y2: "17" }
            }
        },
        SidebarItem::PinBoards => rsx! {
            PinIcon { class: class.to_string(), filled: false }
        },
        SidebarItem::Trending => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                polyline { points: "23 6 13.5 15.5 8.5 10.5 1 18" }
                polyline { points: "17 6 23 6 23 12" }
            }
        },
        SidebarItem::Nips => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                polyline { points: "14 2 14 8 20 8" }
                line { x1: "16", y1: "13", x2: "8", y2: "13" }
                line { x1: "16", y1: "17", x2: "8", y2: "17" }
                polyline { points: "10 9 9 9 8 9" }
            }
        },
        SidebarItem::Badges => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                circle { cx: "12", cy: "8", r: "6" }
                path { d: "M15.477 12.89 17 22l-5-3-5 3 1.523-9.11" }
            }
        },
        SidebarItem::Citations => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                // Quote/citation icon
                path { d: "M3 21c3 0 7-1 7-8V5c0-1.25-.756-2.017-2-2H4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 .008-1 1.031V21" }
                path { d: "M15 21c3 0 7-1 7-8V5c0-1.25-.757-2.017-2-2h-4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2h.75c0 2.25.25 4-2.75 4v3" }
            }
        },
        SidebarItem::Code => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                polyline { points: "16 18 22 12 16 6" }
                polyline { points: "8 6 2 12 8 18" }
            }
        },
        SidebarItem::Lists => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                line { x1: "8", y1: "6", x2: "21", y2: "6" }
                line { x1: "8", y1: "12", x2: "21", y2: "12" }
                line { x1: "8", y1: "18", x2: "21", y2: "18" }
                line { x1: "3", y1: "6", x2: "3.01", y2: "6" }
                line { x1: "3", y1: "12", x2: "3.01", y2: "12" }
                line { x1: "3", y1: "18", x2: "3.01", y2: "18" }
            }
        },
        SidebarItem::Dvm => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "4", y: "4", width: "16", height: "16", rx: "2" }
                rect { x: "9", y: "9", width: "6", height: "6" }
                line { x1: "9", y1: "1", x2: "9", y2: "4" }
                line { x1: "15", y1: "1", x2: "15", y2: "4" }
                line { x1: "9", y1: "20", x2: "9", y2: "23" }
                line { x1: "15", y1: "20", x2: "15", y2: "23" }
                line { x1: "20", y1: "9", x2: "23", y2: "9" }
                line { x1: "20", y1: "14", x2: "23", y2: "14" }
                line { x1: "1", y1: "9", x2: "4", y2: "9" }
                line { x1: "1", y1: "14", x2: "4", y2: "14" }
            }
        },
        SidebarItem::Wiki => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20" }
                path { d: "M8 7h6" }
                path { d: "M8 11h8" }
            }
        },
        SidebarItem::Publications => rsx! {
            svg {
                class: "{class}",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" }
                path { d: "M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" }
            }
        },
        SidebarItem::Shop => rsx! {
            ShoppingBagIcon { class: class.to_string() }
        },
    }
}
