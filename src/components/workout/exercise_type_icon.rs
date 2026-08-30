//! Icon mapping for NIP-101e exercise types.
use crate::components::icons::*;
use crate::utils::nips::nip101e::ExerciseType;
use dioxus::prelude::*;

#[component]
pub fn ExerciseTypeIcon(
    exercise_type: Option<ExerciseType>,
    #[props(default = "w-6 h-6".to_string())] class: String,
) -> Element {
    match exercise_type {
        Some(ExerciseType::Running) => rsx! { RunIcon { class } },
        Some(ExerciseType::Walking) => rsx! { WalkIcon { class } },
        Some(ExerciseType::Cycling) => rsx! { BikeIcon { class } },
        Some(ExerciseType::Hiking) => rsx! { MountainIcon { class } },
        Some(ExerciseType::Swimming) => rsx! { WavesIcon { class } },
        Some(ExerciseType::Rowing) => rsx! { SailboatIcon { class } },
        Some(ExerciseType::Strength)
        | Some(ExerciseType::Circuit)
        | Some(ExerciseType::Emom)
        | Some(ExerciseType::Amrap) => rsx! { DumbbellIcon { class } },
        Some(ExerciseType::Yoga) | Some(ExerciseType::Meditation) => rsx! { FlowerIcon { class } },
        Some(ExerciseType::Diet) => rsx! { UtensilsIcon { class } },
        Some(ExerciseType::Fasting) => rsx! { TimerIcon { class } },
        None => rsx! { RunIcon { class } },
    }
}
