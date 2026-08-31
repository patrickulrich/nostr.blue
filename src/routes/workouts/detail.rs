//! Workout detail route: thin wrapper over [WorkoutViewer].
use crate::components::viewers::WorkoutViewer;
use dioxus::prelude::*;

#[component]
pub fn WorkoutDetail(note_id: String) -> Element {
    rsx! {
        WorkoutViewer { note_id }
    }
}
