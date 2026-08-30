//! Health Connect workout suggestion carousel
//! `DetectedWorkoutCarousel` port). Renders only on the Android build;
//! other platforms compile it as a no-op so composers stay uniform.
use dioxus::prelude::*;

/// The data the carousel hands to the composer when a suggestion is
/// picked. Platform-neutral so the composer compiles without the
/// mobile feature.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectedWorkoutPrefill {
    pub title: Option<String>,
    pub start_time: u64,
    pub duration_seconds: u64,
    pub distance_meters: Option<f64>,
    pub calories: Option<u32>,
    pub avg_heart_rate: Option<u32>,
    pub max_heart_rate: Option<u32>,
    pub steps: Option<u32>,
    pub elevation_gain_meters: Option<f64>,
    pub source: Option<String>,
    pub session_count: usize,
    pub exercise_label: String,
}

#[cfg(feature = "mobile_platform")]
mod imp {
    use super::*;
    use crate::utils::format::format_relative_time_or;
    use crate::utils::workout_merger::DetectedWorkout;
    use crate::platform::android_health;

    #[derive(Clone)]
    enum CarouselState {
        Checking,
        Unavailable,
        NeedsPermission,
        Ready(Vec<DetectedWorkout>),
    }

    fn reload() -> CarouselState {
        if !android_health::is_health_connect_available() {
            return CarouselState::Unavailable;
        }
        if !android_health::has_all_health_permissions() {
            return CarouselState::NeedsPermission;
        }
        let since = android_health::now_secs()
            .saturating_sub(android_health::LOOKBACK_DAYS * 24 * 60 * 60);
        let mut workouts = android_health::read_health_workouts(since);
        workouts.sort_by_key(|w| std::cmp::Reverse(w.start_time_epoch_seconds));
        CarouselState::Ready(workouts)
    }

    fn to_prefill(w: &DetectedWorkout) -> DetectedWorkoutPrefill {
        DetectedWorkoutPrefill {
            title: w.title.clone(),
            start_time: w.start_time_epoch_seconds,
            duration_seconds: w.duration_seconds,
            distance_meters: w.distance_meters,
            calories: w.calories,
            avg_heart_rate: w.avg_heart_rate,
            max_heart_rate: w.max_heart_rate,
            steps: w.steps,
            elevation_gain_meters: w.elevation_gain_meters,
            source: Some(w.source.clone()),
            session_count: w.session_count,
            exercise_label: w.exercise.hashtag().to_string(),
        }
    }

    /// `M:SS` / `H:MM:SS` duration formatting.
    fn format_workout_duration(total_seconds: u64) -> String {
        let h = total_seconds / 3600;
        let m = (total_seconds % 3600) / 60;
        let s = total_seconds % 60;
        if h > 0 {
            format!("{}:{:02}:{:02}", h, m, s)
        } else {
            format!("{}:{:02}", m, s)
        }
    }

    fn summary_line(w: &DetectedWorkout) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(meters) = w.distance_meters.filter(|m| *m > 0.0) {
            parts.push(format!("{:.2} km", meters / 1000.0));
        }
        parts.push(format_workout_duration(w.duration_seconds));
        if w.session_count > 1 {
            parts.push(format!(
                "{} activit{}",
                w.session_count,
                if w.session_count == 1 { "y" } else { "ies" }
            ));
        }
        parts.join(" \u{b7} ")
    }

    #[component]
    pub fn HealthConnectCarousel(on_pick: EventHandler<DetectedWorkoutPrefill>) -> Element {
        let mut state = use_signal(|| CarouselState::Checking);
        use_effect(move || {
            spawn(async move {
                // JNI calls are blocking (runBlocking on the Kotlin side);
                // keep them off the render thread.
                let next = tokio::task::spawn_blocking(reload).await.unwrap_or(CarouselState::Unavailable);
                state.set(next);
            });
        });
        let handle_connect = move |_| {
            spawn(async move {
                let _ = tokio::task::spawn_blocking(|| {
                    crate::platform::android_health::request_health_permissions()
                })
                .await;
                // Poll for the grant result: the system sheet runs outside
                // the app, so there is no callback into Rust.
                for _ in 0..30 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let granted = tokio::task::spawn_blocking(|| {
                        crate::platform::android_health::has_all_health_permissions()
                    })
                    .await
                    .unwrap_or(false);
                    if granted {
                        let next = tokio::task::spawn_blocking(reload)
                            .await
                            .unwrap_or(CarouselState::Unavailable);
                        state.set(next);
                        return;
                    }
                }
            });
        };
        let current = state.read().clone();
        match &current {
            CarouselState::Checking | CarouselState::Unavailable => rsx! {},
            CarouselState::NeedsPermission => {
                rsx! {
                    div { class: "p-4 rounded-xl border border-border",
                        div { class: "flex items-start gap-3",
                            div { class: "w-10 h-10 rounded-full bg-primary/15 flex items-center justify-center shrink-0",
                                span { class: "text-xl", "\u{2764}\u{FE0F}" }
                            }
                            div { class: "flex-1",
                                p { class: "font-semibold text-sm", "Share your workouts" }
                                p { class: "text-xs text-muted-foreground mt-1",
                                    "Let nostr.blue read finished workouts from Health Connect (Samsung Health, Google Fit, Fitbit, Garmin\u{2026}) and suggest a post. Permission is requested only when you connect."
                                }
                            }
                        }
                        div { class: "flex justify-end mt-2",
                            button {
                                class: "px-4 py-1.5 text-sm rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition font-medium",
                                onclick: handle_connect,
                                "Connect"
                            }
                        }
                    }
                }
            }
            CarouselState::Ready(workouts) => {
                if workouts.is_empty() {
                    return rsx! {};
                }
                rsx! {
                    div { class: "space-y-2",
                        p { class: "text-sm font-semibold", "From Health Connect" }
                        div { class: "flex gap-2 overflow-x-auto scrollbar-hide pb-1",
                            for w in workouts.iter() {
                                {
                                    let prefill = to_prefill(w);
                                    let label = w.title.clone().unwrap_or_else(|| w.exercise.hashtag().to_string());
                                    let summary = summary_line(w);
                                    let time_ago = format_relative_time_or(w.start_time_epoch_seconds, "now");
                                    rsx! {
                                        button {
                                            key: "{w.id}",
                                            class: "w-[170px] shrink-0 text-left p-3 rounded-xl border border-border hover:border-primary/50 transition",
                                            onclick: move |_| {
                                                on_pick.call(prefill.clone());
                                            },
                                            p { class: "text-sm font-semibold truncate", "{label}" }
                                            p { class: "text-xs text-muted-foreground truncate mt-0.5", "{summary}" }
                                            p { class: "text-xs text-muted-foreground/70 truncate", "{time_ago}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "mobile_platform")]
pub use imp::HealthConnectCarousel;

#[cfg(not(feature = "mobile_platform"))]
mod imp {
    use super::*;

    #[component]
    pub fn HealthConnectCarousel(on_pick: EventHandler<DetectedWorkoutPrefill>) -> Element {
        // Health Connect only exists on Android; web/desktop composers
        // render nothing here.
        let _ = on_pick;
        rsx! {}
    }
}

#[cfg(not(feature = "mobile_platform"))]
pub use imp::HealthConnectCarousel;
