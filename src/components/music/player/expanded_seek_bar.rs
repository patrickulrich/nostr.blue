use crate::stores::music_player;
use dioxus::prelude::*;

use super::format_time;

#[component]
pub fn ExpandedSeekBar(current_time: f64, duration: f64) -> Element {
    let mut is_scrubbing = use_signal(|| false);
    let mut scrub_position = use_signal(|| None::<f64>);
    let mut seek_bar_left = use_signal(|| 0.0f64);
    let mut seek_bar_width = use_signal(|| 1.0f64);
    let mut gesture_id = use_signal(|| 0u32);

    let progress = if duration > 0.0 {
        (current_time / duration * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    let display_progress = if let Some(pos) = scrub_position() {
        pos
    } else {
        progress
    };
    let display_time = if let Some(pos) = scrub_position() {
        if duration > 0.0 {
            pos / 100.0 * duration
        } else {
            0.0
        }
    } else {
        current_time
    };

    let duration_for_seek = duration;

    rsx! {
        div { class: "w-full mb-4",
            div {
                id: "expanded-seek-bar",
                class: "relative h-6 flex items-center cursor-pointer touch-none",
                onpointerdown: move |evt: Event<PointerData>| {
                    let client_x = evt.client_coordinates().x;
                    gesture_id.set(gesture_id().wrapping_add(1));
                    let current_gesture = gesture_id();
                    spawn(async move {
                        let result = document::eval(&format!(
                            r#"
                            let el = document.getElementById('expanded-seek-bar');
                            if (!el) return [0, 1, 0];
                            let r = el.getBoundingClientRect();
                            let w = r.width || 1;
                            let p = Math.max(0, Math.min(100, (({client_x} - r.left) / w) * 100));
                            return [r.left, w, p];
                            "#,
                        ))
                        .await;
                        if gesture_id() != current_gesture {
                            return;
                        }
                        if let Ok(val) = result {
                            if let Some(arr) = val.as_array() {
                                let left = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                                let width = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0).max(1.0);
                                let percent = arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
                                seek_bar_left.set(left);
                                seek_bar_width.set(width);
                                scrub_position.set(Some(percent));
                                is_scrubbing.set(true);
                            }
                        }
                    });
                },
                onpointermove: move |evt: Event<PointerData>| {
                    if *is_scrubbing.read() && *seek_bar_width.read() > 1.0 {
                        let client_x = evt.client_coordinates().x;
                        let left = *seek_bar_left.read();
                        let width = *seek_bar_width.read();
                        let percent = ((client_x - left) / width * 100.0).clamp(0.0, 100.0);
                        scrub_position.set(Some(percent));
                    }
                },
                onpointerup: move |_| {
                    gesture_id.set(gesture_id().wrapping_add(1));
                    if let Some(pos) = scrub_position() {
                        let new_time = pos / 100.0 * duration_for_seek;
                        if new_time.is_finite() && new_time >= 0.0 {
                            music_player::seek_to(new_time);
                        }
                    }
                    is_scrubbing.set(false);
                    scrub_position.set(None);
                },
                onpointerleave: move |_| {
                    if *is_scrubbing.read() {
                        gesture_id.set(gesture_id().wrapping_add(1));
                        if let Some(pos) = scrub_position() {
                            let new_time = pos / 100.0 * duration_for_seek;
                            if new_time.is_finite() && new_time >= 0.0 {
                                music_player::seek_to(new_time);
                            }
                        }
                        is_scrubbing.set(false);
                        scrub_position.set(None);
                    }
                },
                div { class: "absolute inset-x-0 top-1/2 -translate-y-1/2 h-1.5 bg-secondary rounded-full",
                    div {
                        class: "absolute h-full bg-primary rounded-full transition-[width] duration-75",
                        style: "width: {display_progress}%",
                    }
                }
                div {
                    class: "absolute top-1/2 -translate-y-1/2 w-4 h-4 bg-primary rounded-full shadow-md transition-[left] duration-75",
                    style: "left: calc({display_progress}% - 8px);",
                }
            }
            div { class: "flex justify-between mt-1",
                span { class: "text-xs text-muted-foreground",
                    "{format_time(display_time)}"
                }
                span { class: "text-xs text-muted-foreground",
                    "{format_time(duration)}"
                }
            }
        }
    }
}
