//! Health Connect session merging (Amethyst `WorkoutMerger` port).
//!
//! Watches, Strava, and auto-pause split one long workout into several
//! back-to-back sessions of the same type; posting each break as its own
//! workout is noise. Sessions of the same type less than
//! [DEFAULT_MAX_GAP_SECONDS] apart chain-merge. Sessions of a different
//! type occurring between two same-type sessions do not break the chain
//! (gaps are tracked per exercise type).
// Consumed by the Android Health Connect bridge (mobile_platform) and
// the inline tests; unused elsewhere.
#![cfg_attr(not(any(test, feature = "mobile_platform")), allow(dead_code))]
use crate::utils::nips::nip101e::ExerciseType;
use serde::{Deserialize, Serialize};

/// Sessions less than one hour apart are treated as one workout.
pub const DEFAULT_MAX_GAP_SECONDS: u64 = 3600;

/// A workout detected on-device (platform-neutral; fed by the Health
/// Connect bridge on Android).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetectedWorkout {
    /// Health Connect record id; the members' ids joined with `+` when
    /// several close-by sessions were combined.
    pub id: String,
    pub exercise: ExerciseType,
    pub title: Option<String>,
    pub start_time_epoch_seconds: u64,
    pub duration_seconds: u64,
    pub distance_meters: Option<f64>,
    pub calories: Option<u32>,
    pub avg_heart_rate: Option<u32>,
    pub max_heart_rate: Option<u32>,
    pub steps: Option<u32>,
    pub elevation_gain_meters: Option<f64>,
    /// Human app name, e.g. "Samsung Health".
    pub source: String,
    /// 1 for a raw session; higher when [merge_close_workouts] combined
    /// several close-by same-type sessions.
    pub session_count: usize,
}

impl DetectedWorkout {
    pub fn new(id: String, exercise: ExerciseType, source: String) -> Self {
        DetectedWorkout {
            id,
            exercise,
            title: None,
            start_time_epoch_seconds: 0,
            duration_seconds: 0,
            distance_meters: None,
            calories: None,
            avg_heart_rate: None,
            max_heart_rate: None,
            steps: None,
            elevation_gain_meters: None,
            source,
            session_count: 1,
        }
    }
}

/// End of a raw session: its start plus its (contiguous) duration.
fn end_of(w: &DetectedWorkout) -> u64 {
    w.start_time_epoch_seconds
        .saturating_add(w.duration_seconds)
}

fn sum_f64(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let mut total = 0.0;
    let mut any = false;
    for x in values.flatten() {
        total += x;
        any = true;
    }
    if any {
        Some(total)
    } else {
        None
    }
}

fn sum_u32(values: impl Iterator<Item = Option<u32>>) -> Option<u32> {
    let mut total: u32 = 0;
    let mut any = false;
    for x in values.flatten() {
        total = total.saturating_add(x);
        any = true;
    }
    if any {
        Some(total)
    } else {
        None
    }
}

/// Duration-weighted average heart rate: a 4-hour leg dominates a
/// 5-minute leg. Falls back to a plain average when every contributing
/// leg has zero duration (shouldn't happen for real sessions).
fn weighted_avg_heart_rate(group: &[DetectedWorkout]) -> Option<u32> {
    let with_hr: Vec<(u32, u64)> = group
        .iter()
        .filter_map(|w| w.avg_heart_rate.map(|hr| (hr, w.duration_seconds)))
        .collect();
    if with_hr.is_empty() {
        return None;
    }
    let weight_sum: u64 = with_hr.iter().map(|(_, d)| *d).sum();
    let avg = if weight_sum > 0 {
        with_hr.iter().map(|(hr, d)| f64::from(*hr) * (*d as f64)).sum::<f64>()
            / weight_sum as f64
    } else {
        with_hr.iter().map(|(hr, _)| f64::from(*hr)).sum::<f64>() / with_hr.len() as f64
    };
    Some(avg.round() as u32)
}

fn combine(group: Vec<DetectedWorkout>) -> DetectedWorkout {
    if group.len() == 1 {
        return group.into_iter().next().expect("group has one element");
    }
    let earliest = &group[0];
    DetectedWorkout {
        id: group.iter().map(|w| w.id.as_str()).collect::<Vec<_>>().join("+"),
        exercise: earliest.exercise,
        title: group
            .iter()
            .find_map(|w| w.title.as_ref().filter(|t| !t.trim().is_empty()).cloned()),
        start_time_epoch_seconds: earliest.start_time_epoch_seconds,
        // Summed *active* duration; gaps between legs are excluded.
        duration_seconds: group.iter().map(|w| w.duration_seconds).sum(),
        distance_meters: sum_f64(group.iter().map(|w| w.distance_meters)),
        calories: sum_u32(group.iter().map(|w| w.calories)),
        avg_heart_rate: weighted_avg_heart_rate(&group),
        max_heart_rate: group.iter().filter_map(|w| w.max_heart_rate).max(),
        steps: sum_u32(group.iter().map(|w| w.steps)),
        elevation_gain_meters: sum_f64(group.iter().map(|w| w.elevation_gain_meters)),
        source: earliest.source.clone(),
        session_count: group.iter().map(|w| w.session_count).sum(),
    }
}

/// Merge same-type sessions whose gap is strictly less than
/// `max_gap_seconds`. Results are ordered by combined start time
/// (first-member encounter order in the start-sorted walk).
pub fn merge_close_workouts(
    workouts: Vec<DetectedWorkout>,
    max_gap_seconds: u64,
) -> Vec<DetectedWorkout> {
    if workouts.len() < 2 {
        return workouts;
    }
    let mut sorted = workouts;
    sorted.sort_by_key(|w| w.start_time_epoch_seconds);
    let mut groups: Vec<Vec<DetectedWorkout>> = Vec::new();
    // Most recent still-open group per exercise type.
    let mut open_group_by_type: std::collections::HashMap<ExerciseType, usize> =
        std::collections::HashMap::new();
    let mut last_end_by_type: std::collections::HashMap<ExerciseType, u64> =
        std::collections::HashMap::new();
    for workout in sorted {
        let exercise = workout.exercise;
        let end = end_of(&workout);
        let start = workout.start_time_epoch_seconds;
        let close_enough = last_end_by_type
            .get(&exercise)
            .is_some_and(|last_end| start.saturating_sub(*last_end) < max_gap_seconds);
        match (open_group_by_type.get(&exercise).copied(), close_enough) {
            (Some(idx), true) => groups[idx].push(workout),
            _ => {
                groups.push(vec![workout]);
                open_group_by_type.insert(exercise, groups.len() - 1);
            }
        }
        let last_end = last_end_by_type.get(&exercise).copied().unwrap_or(u64::MIN);
        last_end_by_type.insert(exercise, last_end.max(end));
    }
    groups.into_iter().map(combine).collect()
}

/// [merge_close_workouts] with [DEFAULT_MAX_GAP_SECONDS].
pub fn merge_close_workouts_default(workouts: Vec<DetectedWorkout>) -> Vec<DetectedWorkout> {
    merge_close_workouts(workouts, DEFAULT_MAX_GAP_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workout(
        id: &str,
        start: u64,
        duration: u64,
    ) -> DetectedWorkout {
        DetectedWorkout {
            id: id.to_string(),
            exercise: ExerciseType::Running,
            title: None,
            start_time_epoch_seconds: start,
            duration_seconds: duration,
            distance_meters: None,
            calories: None,
            avg_heart_rate: None,
            max_heart_rate: None,
            steps: None,
            elevation_gain_meters: None,
            source: "Samsung Health".to_string(),
            session_count: 1,
        }
    }

    #[test]
    fn empty_list_passes_through() {
        assert!(merge_close_workouts_default(Vec::new()).is_empty());
    }

    #[test]
    fn single_workout_is_returned_unchanged() {
        let input = vec![workout("only", 0, 1800)];
        let output = merge_close_workouts_default(input.clone());
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], input[0]);
        assert_eq!(output[0].session_count, 1);
    }

    #[test]
    fn two_close_same_type_sessions_merge_and_sum_metrics() {
        let mut a = workout("first", 0, 3600);
        a.distance_meters = Some(10_000.0);
        a.calories = Some(500);
        a.steps = Some(10_000);
        a.elevation_gain_meters = Some(50.0);
        let mut b = workout("second", 3600 + 1800, 1800);
        b.distance_meters = Some(5000.0);
        b.calories = Some(400);
        b.steps = Some(8000);
        b.elevation_gain_meters = Some(25.0);
        let output = merge_close_workouts_default(vec![a, b]);
        assert_eq!(output.len(), 1);
        let merged = &output[0];
        assert_eq!(merged.id, "first+second");
        assert_eq!(merged.session_count, 2);
        assert_eq!(merged.start_time_epoch_seconds, 0);
        assert_eq!(merged.duration_seconds, 3600 + 1800);
        assert_eq!(merged.distance_meters, Some(15_000.0));
        assert_eq!(merged.calories, Some(900));
        assert_eq!(merged.steps, Some(18_000));
        assert_eq!(merged.elevation_gain_meters, Some(75.0));
    }

    #[test]
    fn same_type_but_far_apart_does_not_merge() {
        // 90-minute gap > 1 hour.
        let a = workout("first", 0, 3600);
        let b = workout("second", 3600 + 5400, 1800);
        let output = merge_close_workouts_default(vec![a, b]);
        assert_eq!(output.len(), 2);
        assert_eq!(output[0].id, "first");
        assert_eq!(output[1].id, "second");
    }

    #[test]
    fn gap_of_exactly_one_hour_does_not_merge() {
        let a = workout("first", 0, 3600);
        let b = workout("second", 7200, 1800);
        let output = merge_close_workouts_default(vec![a, b]);
        assert_eq!(output.len(), 2, "boundary is exclusive");
    }

    #[test]
    fn different_types_close_together_do_not_merge() {
        let mut a = workout("run", 0, 3600);
        let mut b = workout("ride", 60, 3600);
        b.exercise = ExerciseType::Cycling;
        a.title = None;
        let output = merge_close_workouts_default(vec![a, b]);
        assert_eq!(output.len(), 2);
    }

    #[test]
    fn interleaved_other_type_does_not_break_same_type_chain() {
        let run1 = workout("run1", 0, 3600);
        let mut walk = workout("walk", 3700, 600);
        walk.exercise = ExerciseType::Walking;
        let run2 = workout("run2", 4400, 3600);
        let output = merge_close_workouts_default(vec![run1, walk, run2]);
        assert_eq!(output.len(), 2);
        let runs = output.iter().find(|w| w.exercise == ExerciseType::Running).unwrap();
        assert_eq!(runs.id, "run1+run2");
        assert_eq!(runs.session_count, 2);
        assert_eq!(runs.duration_seconds, 7200);
        let walk_result = output.iter().find(|w| w.exercise == ExerciseType::Walking).unwrap();
        assert_eq!(walk_result.session_count, 1);
    }

    #[test]
    fn heart_rate_is_duration_weighted() {
        let mut a = workout("a", 0, 3600);
        a.avg_heart_rate = Some(120);
        a.max_heart_rate = Some(160);
        let mut b = workout("b", 3700, 1800);
        b.avg_heart_rate = Some(150);
        b.max_heart_rate = Some(175);
        let output = merge_close_workouts_default(vec![a, b]);
        // (120*3600 + 150*1800) / 5400 = 130
        assert_eq!(output[0].avg_heart_rate, Some(130));
        assert_eq!(output[0].max_heart_rate, Some(175));
    }

    #[test]
    fn null_metrics_are_summed_only_when_present() {
        let mut a = workout("a", 0, 3600);
        a.distance_meters = Some(5000.0);
        let mut b = workout("b", 3700, 1800);
        b.calories = Some(300);
        let output = merge_close_workouts_default(vec![a, b]);
        assert_eq!(output[0].distance_meters, Some(5000.0));
        assert_eq!(output[0].calories, Some(300));
    }

    #[test]
    fn all_null_metric_stays_null() {
        let a = workout("a", 0, 3600);
        let b = workout("b", 3700, 1800);
        let output = merge_close_workouts_default(vec![a, b]);
        let merged = &output[0];
        assert_eq!(merged.distance_meters, None);
        assert_eq!(merged.calories, None);
        assert_eq!(merged.avg_heart_rate, None);
        assert_eq!(merged.max_heart_rate, None);
        assert_eq!(merged.steps, None);
        assert_eq!(merged.elevation_gain_meters, None);
    }

    #[test]
    fn title_takes_first_non_blank() {
        let mut a = workout("a", 0, 3600);
        a.title = Some("   ".to_string());
        let mut b = workout("b", 3700, 1800);
        b.title = Some("Morning long run".to_string());
        let output = merge_close_workouts_default(vec![a, b]);
        assert_eq!(output[0].title.as_deref(), Some("Morning long run"));
    }

    #[test]
    fn unsorted_input_produces_results_ordered_by_start() {
        let a = workout("a", 0, 600);
        let b = workout("b", 700, 600);
        let c = workout("c", 1400, 600);
        let output = merge_close_workouts_default(vec![c.clone(), a, b]);
        assert_eq!(output.len(), 1);
        let merged = &output[0];
        assert_eq!(merged.id, "a+b+c");
        assert_eq!(merged.session_count, 3);
        assert_eq!(merged.start_time_epoch_seconds, 0);
        assert_eq!(merged.duration_seconds, 1800);
    }

    #[test]
    fn chain_merges_even_when_adjacent_gaps_are_short_but_ends_are_far_apart() {
        // Five 45-minute sessions each starting 50 minutes apart: the
        // consecutive gaps are short, so they chain transitively even
        // though the first and last are hours apart.
        let sessions: Vec<DetectedWorkout> = (0..5)
            .map(|i| workout(&format!("w{}", i), i * 3000, 2700))
            .collect();
        let output = merge_close_workouts_default(sessions);
        assert_eq!(output.len(), 1, "transitive chaining");
        assert_eq!(output[0].session_count, 5);
    }
}
