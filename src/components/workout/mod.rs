//! NIP-101e fitness workouts: kind-1301 workout records and kind-33401
//! exercise templates.
pub mod exercise_template_card;
pub mod exercise_type_icon;
pub mod health_connect;
pub mod units;
pub mod workout_card;

pub use exercise_template_card::ExerciseTemplateCard;
pub use exercise_type_icon::ExerciseTypeIcon;
#[allow(unused_imports)]
pub use units::{effective_units, WorkoutUnits};
pub use workout_card::WorkoutCard;
