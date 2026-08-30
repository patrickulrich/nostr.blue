//! Workout composer (kind 1301, RUNSTR-canonical wire format).
//! Amethyst `NewWorkoutScreen` port.
use crate::components::ExerciseTypeIcon;
use crate::components::workout::units;
use crate::stores::{auth_store, nostr_client};
use crate::utils::nips::nip101e::ExerciseType;
use dioxus::prelude::*;

fn duration_seconds(h: &str, m: &str, s: &str) -> u64 {
    h.parse::<u64>().unwrap_or(0) * 3600
        + m.parse::<u64>().unwrap_or(0) * 60
        + s.parse::<u64>().unwrap_or(0)
}

#[component]
pub fn WorkoutNew() -> Element {
    let navigator = navigator();
    let nav_close = navigator;
    let nav_publish = navigator;
    let nav_effect = navigator;
    let mut exercise = use_signal(|| ExerciseType::Running);
    let mut title = use_signal(String::new);
    let mut hours = use_signal(String::new);
    let mut minutes = use_signal(String::new);
    let mut seconds = use_signal(String::new);
    let mut distance = use_signal(String::new);
    let mut distance_unit = use_signal(|| {
        match units::effective_units() {
            units::WorkoutUnits::Imperial => "mi".to_string(),
            units::WorkoutUnits::Metric => "km".to_string(),
        }
    });
    let mut calories = use_signal(String::new);
    let mut notes = use_signal(String::new);
    // Hidden carry-through fields: set by the Health Connect carousel on
    // mobile, published when present.
    let mut hc_source = use_signal(|| Option::<String>::None);
    let mut hc_avg_heart_rate = use_signal(|| 0u32);
    let mut hc_max_heart_rate = use_signal(|| 0u32);
    let mut hc_steps = use_signal(|| 0u32);
    let mut hc_elevation_gain = use_signal(|| 0.0f64);
    let mut hc_start_time = use_signal(|| 0u64);
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);
    let can_publish = use_memo(move || {
        duration_seconds(&hours.read(), &minutes.read(), &seconds.read()) > 0
            && !*is_publishing.read()
    });
    let handle_close = move |_| {
        nav_close.go_back();
    };
    let handle_publish = move |_| {
        if !*can_publish.read() {
            return;
        }
        let exercise_val = *exercise.read();
        let title_val = title.read().clone();
        let duration_val = duration_seconds(&hours.read(), &minutes.read(), &seconds.read());
        let distance_val = distance.read().parse::<f64>().ok().filter(|d| *d > 0.0);
        let unit_val = distance_unit.read().clone();
        let calories_val = calories.read().parse::<u32>().ok();
        let notes_val = notes.read().clone();
        let source_val = hc_source.read().clone();
        let avg_hr = *hc_avg_heart_rate.read();
        let max_hr = *hc_max_heart_rate.read();
        let steps = *hc_steps.read();
        let elevation = *hc_elevation_gain.read();
        let start_time = *hc_start_time.read();
        is_publishing.set(true);
        error_message.set(None);
        let nav_spawn = nav_publish;
        spawn(async move {
            let draft = crate::utils::nips::nip101e::WorkoutDraft {
                exercise: exercise_val,
                duration_seconds: duration_val,
                notes: notes_val,
                title: if title_val.trim().is_empty() {
                    None
                } else {
                    Some(title_val)
                },
                source: source_val,
                distance: distance_val.map(|d| (d, unit_val.clone())),
                calories: calories_val,
                avg_heart_rate: if avg_hr > 0 { Some(avg_hr) } else { None },
                max_heart_rate: if max_hr > 0 { Some(max_hr) } else { None },
                steps: if steps > 0 { Some(steps) } else { None },
                elevation_gain_meters: if elevation > 0.0 { Some(elevation) } else { None },
                workout_start_time: if start_time > 0 { Some(start_time) } else { None },
            };
            match nostr_client::publish_workout(draft).await {
                Ok(event_id) => {
                    log::info!("Workout published successfully: {}", event_id);
                    is_publishing.set(false);
                    nav_spawn.push(crate::routes::Route::Workouts {});
                }
                Err(e) => {
                    log::error!("Failed to publish workout: {}", e);
                    error_message.set(Some(format!("Failed to publish: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };
    use_effect(move || {
        if !*is_authenticated.read() {
            nav_effect.push(crate::routes::Route::Home {
                list: String::new(),
            });
        }
    });
    if !*is_authenticated.read() {
        return rsx! {
            div { class: "flex items-center justify-center h-screen", "Redirecting..." }
        };
    }
    let on_workout_detected = move |detected: crate::components::workout::health_connect::DetectedWorkoutPrefill| {
        if let Some(t) = &detected.title {
            title.set(t.clone());
        }
        let d = detected.duration_seconds;
        hours.set(if d >= 3600 { (d / 3600).to_string() } else { String::new() });
        minutes.set(if d % 3600 >= 60 || d >= 60 { ((d % 3600) / 60).to_string() } else { String::new() });
        seconds.set(if d % 60 > 0 { (d % 60).to_string() } else { String::new() });
        if let Some(meters) = detected.distance_meters.filter(|m| *m > 0.0) {
            match units::effective_units() {
                units::WorkoutUnits::Imperial => {
                    distance_unit.set("mi".to_string());
                    let miles = meters / crate::utils::nips::nip101e::METERS_PER_MILE;
                    distance.set(format!("{:.2}", miles));
                }
                units::WorkoutUnits::Metric => {
                    distance_unit.set("km".to_string());
                    distance.set(format!("{:.2}", meters / 1000.0));
                }
            }
        }
        if let Some(kcal) = detected.calories {
            calories.set(kcal.to_string());
        }
        hc_source.set(detected.source);
        hc_avg_heart_rate.set(detected.avg_heart_rate.unwrap_or(0));
        hc_max_heart_rate.set(detected.max_heart_rate.unwrap_or(0));
        hc_steps.set(detected.steps.unwrap_or(0));
        hc_elevation_gain.set(detected.elevation_gain_meters.unwrap_or(0.0));
        hc_start_time.set(detected.start_time);
    };
    rsx! {
        div { class: "min-h-screen bg-background",
            div { class: "border-b border-border bg-background sticky top-0 z-10",
                div { class: "max-w-4xl mx-auto px-4 py-4 flex items-center justify-between",
                    div { class: "flex items-center gap-4",
                        button {
                            class: "text-muted-foreground hover:text-foreground transition",
                            onclick: handle_close,
                            svg {
                                class: "w-6 h-6",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M15 19l-7-7 7-7",
                                }
                            }
                        }
                        h1 { class: "text-2xl font-bold", "\u{1F3C3} Log Workout" }
                    }
                    button {
                        class: if *can_publish.read() { "px-6 py-2 bg-primary text-primary-foreground font-semibold rounded-lg hover:bg-primary/90 transition" } else { "px-6 py-2 bg-muted text-muted-foreground font-semibold rounded-lg cursor-not-allowed" },
                        disabled: !*can_publish.read(),
                        onclick: handle_publish,
                        if *is_publishing.read() {
                            "Publishing..."
                        } else {
                            "Post Workout"
                        }
                    }
                }
            }
            div { class: "max-w-4xl mx-auto px-4 py-8",
                if let Some(err) = error_message.read().as_ref() {
                    div { class: "mb-4 p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive",
                        "{err}"
                    }
                }
                div { class: "space-y-6",
                    crate::components::workout::health_connect::HealthConnectCarousel {
                        on_pick: on_workout_detected,
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Activity" }
                        div { class: "flex flex-wrap gap-2",
                            for t in ExerciseType::ALL {
                                {
                                    let selected = *exercise.read() == t;
                                    rsx! {
                                        button {
                                            key: "{t.code()}",
                                            class: if selected {
                                                "px-3 py-1.5 rounded-full border-2 border-primary bg-primary/10 text-primary font-medium text-sm flex items-center gap-1.5 transition"
                                            } else {
                                                "px-3 py-1.5 rounded-full border border-border hover:border-primary/50 text-sm flex items-center gap-1.5 transition"
                                            },
                                            onclick: move |_| exercise.set(t),
                                            ExerciseTypeIcon { exercise_type: Some(t), class: "w-4 h-4".to_string() }
                                            "{t.hashtag()}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Title (optional)" }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                            placeholder: "Morning Run",
                            value: "{title}",
                            oninput: move |evt| title.set(evt.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Duration" }
                        div { class: "flex gap-2",
                            div { class: "flex-1",
                                input {
                                    r#type: "number",
                                    class: "w-full px-4 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    placeholder: "0",
                                    min: "0",
                                    value: "{hours}",
                                    oninput: move |evt| hours.set(evt.value()),
                                }
                                p { class: "mt-1 text-xs text-muted-foreground text-center", "Hours" }
                            }
                            div { class: "flex-1",
                                input {
                                    r#type: "number",
                                    class: "w-full px-4 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    placeholder: "0",
                                    min: "0",
                                    value: "{minutes}",
                                    oninput: move |evt| minutes.set(evt.value()),
                                }
                                p { class: "mt-1 text-xs text-muted-foreground text-center", "Minutes" }
                            }
                            div { class: "flex-1",
                                input {
                                    r#type: "number",
                                    class: "w-full px-4 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    placeholder: "0",
                                    min: "0",
                                    value: "{seconds}",
                                    oninput: move |evt| seconds.set(evt.value()),
                                }
                                p { class: "mt-1 text-xs text-muted-foreground text-center", "Seconds" }
                            }
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Distance (optional)" }
                        div { class: "flex gap-2",
                            input {
                                r#type: "number",
                                step: "0.01",
                                class: "flex-1 px-4 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                                placeholder: "5.20",
                                min: "0",
                                value: "{distance}",
                                oninput: move |evt| distance.set(evt.value()),
                            }
                            div { class: "flex rounded-lg border border-border overflow-hidden",
                                button {
                                    class: if *distance_unit.read() == "km" { "px-4 py-2 bg-primary text-primary-foreground text-sm font-medium transition" } else { "px-4 py-2 text-sm text-muted-foreground hover:bg-accent transition" },
                                    onclick: move |_| distance_unit.set("km".to_string()),
                                    "km"
                                }
                                button {
                                    class: if *distance_unit.read() == "mi" { "px-4 py-2 bg-primary text-primary-foreground text-sm font-medium transition" } else { "px-4 py-2 text-sm text-muted-foreground hover:bg-accent transition" },
                                    onclick: move |_| distance_unit.set("mi".to_string()),
                                    "mi"
                                }
                            }
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Calories (optional)" }
                        input {
                            r#type: "number",
                            class: "w-full px-4 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                            placeholder: "312",
                            min: "0",
                            value: "{calories}",
                            oninput: move |evt| calories.set(evt.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Notes" }
                        textarea {
                            class: "w-full px-4 py-3 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                            placeholder: "How did it go?",
                            rows: "3",
                            value: "{notes}",
                            oninput: move |evt| notes.set(evt.value()),
                        }
                    }
                    div { class: "p-4 bg-muted/30 rounded-lg",
                        h3 { class: "font-semibold mb-2", "\u{1F4A1} About workouts" }
                        ul { class: "space-y-1 text-sm text-muted-foreground list-disc list-inside",
                            li { "Workouts are published as NIP-101e kind 1301 events, interoperable with RUNSTR and POWR" }
                            li { "Duration is required; everything else is optional" }
                            li { "On Android, workouts recorded in Health Connect can pre-fill this form" }
                        }
                    }
                }
            }
        }
    }
}
