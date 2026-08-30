//! NIP-101e Fitness Workouts parsing and building
//!
//! Kind 1301 workout records, interoperable with the RUNSTR dialect
//! (docs/KIND_1301_SPEC.md in RUNSTR-LLC/RUNSTR) and the POWR / NIP-101e
//! strength dialect. Kind 33401 exercise templates as published by POWR.
//!
//! Parsing is intentionally lax: every tag is optional, units default to
//! metric (km/m) and pounds, and durations accept `HH:MM:SS` or raw
//! seconds. The content is plain text (user notes), never JSON.
//!
//! The two dialects share the `exercise` tag name and are disambiguated
//! structurally: RUNSTR publishes a plain verb (`["exercise","running"]`),
//! POWR publishes a kind-33401 coordinate plus per-set data
//! (`["exercise","33401:pubkey:d-tag",relay,weight,reps,rpe,set_type,set_number]`).
use nostr_sdk::prelude::*;
use std::collections::HashMap;

/// Workout record (regular kind)
pub const KIND_WORKOUT: u16 = 1301;
/// Exercise template (addressable kind, published by POWR)
pub const KIND_EXERCISE_TEMPLATE: u16 = 33401;

/// Known `source` tag values
#[allow(dead_code)]
pub const SOURCE_GPS: &str = "gps";
#[allow(dead_code)]
pub const SOURCE_MANUAL: &str = "manual";
#[allow(dead_code)]
pub const SOURCE_HEALTH_CONNECT: &str = "health_connect";

/// POWR `set_type` values on per-set `exercise` tags
#[allow(dead_code)]
pub const SET_TYPE_WARMUP: &str = "warmup";
#[allow(dead_code)]
pub const SET_TYPE_NORMAL: &str = "normal";
#[allow(dead_code)]
pub const SET_TYPE_DROP: &str = "drop";
#[allow(dead_code)]
pub const SET_TYPE_FAILURE: &str = "failure";

pub const METERS_PER_MILE: f64 = 1609.344;
pub const METERS_PER_FOOT: f64 = 0.3048;
pub const KILOGRAMS_PER_POUND: f64 = 0.45359237;

/// Activity / workout types understood across the kind-1301 dialects.
///
/// The first group are the RUNSTR activity verbs carried in the `exercise`
/// tag; [ExerciseType::Strength]/[Circuit]/[Emom]/[Amrap] double as the
/// POWR / NIP-101e `type` tag classifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ExerciseType {
    Running,
    Walking,
    Cycling,
    Hiking,
    Swimming,
    Rowing,
    Strength,
    Yoga,
    Meditation,
    Diet,
    Fasting,
    Circuit,
    Emom,
    Amrap,
}

impl ExerciseType {
    pub const ALL: [ExerciseType; 14] = [
        ExerciseType::Running,
        ExerciseType::Walking,
        ExerciseType::Cycling,
        ExerciseType::Hiking,
        ExerciseType::Swimming,
        ExerciseType::Rowing,
        ExerciseType::Strength,
        ExerciseType::Yoga,
        ExerciseType::Meditation,
        ExerciseType::Diet,
        ExerciseType::Fasting,
        ExerciseType::Circuit,
        ExerciseType::Emom,
        ExerciseType::Amrap,
    ];

    /// The wire code (`exercise` verb / POWR `type` value).
    pub fn code(&self) -> &'static str {
        match self {
            ExerciseType::Running => "running",
            ExerciseType::Walking => "walking",
            ExerciseType::Cycling => "cycling",
            ExerciseType::Hiking => "hiking",
            ExerciseType::Swimming => "swimming",
            ExerciseType::Rowing => "rowing",
            ExerciseType::Strength => "strength",
            ExerciseType::Yoga => "yoga",
            ExerciseType::Meditation => "meditation",
            ExerciseType::Diet => "diet",
            ExerciseType::Fasting => "fasting",
            ExerciseType::Circuit => "circuit",
            ExerciseType::Emom => "emom",
            ExerciseType::Amrap => "amrap",
        }
    }

    /// The capitalized hashtag published as a `t` tag so RUNSTR workouts
    /// stay discoverable in hashtag feeds.
    pub fn hashtag(&self) -> &'static str {
        match self {
            ExerciseType::Running => "Running",
            ExerciseType::Walking => "Walking",
            ExerciseType::Cycling => "Cycling",
            ExerciseType::Hiking => "Hiking",
            ExerciseType::Swimming => "Swimming",
            ExerciseType::Rowing => "Rowing",
            ExerciseType::Strength => "Strength",
            ExerciseType::Yoga => "Yoga",
            ExerciseType::Meditation => "Meditation",
            ExerciseType::Diet => "Diet",
            ExerciseType::Fasting => "Fasting",
            ExerciseType::Circuit => "Circuit",
            ExerciseType::Emom => "EMOM",
            ExerciseType::Amrap => "AMRAP",
        }
    }

    /// Case-insensitive parse; unknown verbs yield `None`.
    pub fn parse(code: &str) -> Option<ExerciseType> {
        let lowered = code.to_lowercase();
        ExerciseType::ALL
            .iter()
            .find(|t| t.code() == lowered)
            .copied()
    }
}

/// A `["distance", value, unit]` tag. Units: `km` (default), `mi`, `m`.
#[derive(Clone, Debug, PartialEq)]
pub struct Distance {
    pub value: f64,
    pub unit: Option<String>,
}

impl Distance {
    pub fn parse(values: &[String]) -> Option<Distance> {
        let value = values.get(1)?.parse().ok()?;
        let unit = values.get(2).filter(|u| !u.is_empty()).cloned();
        Some(Distance { value, unit })
    }

    /// Lax by design: a missing or unknown unit defaults to kilometers.
    pub fn to_meters(&self) -> f64 {
        match self.unit.as_deref() {
            Some("mi") => self.value * METERS_PER_MILE,
            Some("m") => self.value,
            _ => self.value * 1000.0,
        }
    }

    #[allow(dead_code)]
    pub fn to_kilometers(&self) -> f64 {
        self.to_meters() / 1000.0
    }
}

/// An `["elevation_gain"/"elevation_loss", value, unit]` tag.
/// Units: `m` (default), `ft`.
#[derive(Clone, Debug, PartialEq)]
pub struct Elevation {
    pub value: f64,
    pub unit: Option<String>,
}

impl Elevation {
    pub fn parse(values: &[String]) -> Option<Elevation> {
        let value = values.get(1)?.parse().ok()?;
        let unit = values.get(2).filter(|u| !u.is_empty()).cloned();
        Some(Elevation { value, unit })
    }

    /// Lax by design: a missing or unknown unit defaults to meters.
    pub fn to_meters(&self) -> f64 {
        match self.unit.as_deref() {
            Some("ft") => self.value * METERS_PER_FOOT,
            _ => self.value,
        }
    }
}

/// A `["weight", value, unit]` tag. Units: `lbs` (RUNSTR default), `kg`.
#[derive(Clone, Debug, PartialEq)]
pub struct Weight {
    pub value: f64,
    pub unit: Option<String>,
}

impl Weight {
    pub fn parse(values: &[String]) -> Option<Weight> {
        let value = values.get(1)?.parse().ok()?;
        let unit = values.get(2).filter(|u| !u.is_empty()).cloned();
        Some(Weight { value, unit })
    }

    /// Lax by design: a missing or unknown unit defaults to pounds
    /// (RUNSTR's default).
    pub fn to_kilograms(&self) -> f64 {
        match self.unit.as_deref() {
            Some("kg") => self.value,
            _ => self.value * KILOGRAMS_PER_POUND,
        }
    }
}

/// Parse a duration value: `HH:MM:SS` (or `MM:SS`) or raw seconds.
pub fn parse_duration_time(s: &str) -> Option<u64> {
    if !s.contains(':') {
        return s.parse().ok();
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let mut seconds: u64 = 0;
    for part in parts {
        let p: u64 = part.parse().ok()?;
        seconds = seconds * 60 + p;
    }
    Some(seconds)
}

/// Format a duration as zero-padded `HH:MM:SS` (canonical wire format).
pub fn format_duration_time(total_seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        total_seconds / 3600,
        (total_seconds % 3600) / 60,
        total_seconds % 60
    )
}

/// A per-distance split: 1-based index + cumulative elapsed time at its end.
#[derive(Clone, Debug, PartialEq)]
pub struct Split {
    pub number: u32,
    pub cumulative_seconds: u64,
}

impl Split {
    pub fn parse(values: &[String]) -> Option<Split> {
        let number = values.get(1)?.parse().ok()?;
        let cumulative_seconds = parse_duration_time(values.get(2)?)?;
        Some(Split {
            number,
            cumulative_seconds,
        })
    }
}

/// True when the value looks like a `kind:pubkey:d-tag` coordinate
/// (numeric kind, 64-hex pubkey). Used to structurally distinguish the
/// POWR per-set `exercise` tag from the RUNSTR verb form.
pub fn is_coordinate(value: &str) -> bool {
    let parts: Vec<&str> = value.split(':').collect();
    parts.len() == 3 && parts[0].parse::<u16>().is_ok() && parts[1].len() == 64
}

/// A POWR per-set `exercise` tag:
/// `["exercise", "33401:pubkey:d-tag", relay, weight, reps, rpe, set_type, set_number]`
///
/// Weight is in kilograms; an empty value means bodyweight (None) and a
/// negative value means assisted. `set_number` is a 1-based POWR extension.
#[derive(Clone, Debug, PartialEq)]
pub struct ExerciseSet {
    pub reference: String,
    pub relay_hint: Option<String>,
    pub weight_kg: Option<f64>,
    pub reps: Option<u32>,
    pub rpe: Option<f64>,
    pub set_type: Option<String>,
    pub set_number: Option<u32>,
}

impl ExerciseSet {
    pub fn parse(values: &[String]) -> Option<ExerciseSet> {
        let reference = values.get(1)?;
        if !is_coordinate(reference) {
            return None;
        }
        let relay_hint = values
            .get(2)
            .and_then(|r| if r.is_empty() { None } else { Some(r.clone()) });
        let weight_kg = values
            .get(3)
            .and_then(|w| if w.is_empty() { None } else { w.parse().ok() });
        let reps = values
            .get(4)
            .and_then(|r| if r.is_empty() { None } else { r.parse().ok() });
        let rpe = values
            .get(5)
            .and_then(|r| if r.is_empty() { None } else { r.parse().ok() });
        let set_type = values
            .get(6)
            .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
        let set_number = values.get(7).and_then(|n| n.parse().ok());
        Some(ExerciseSet {
            reference: reference.clone(),
            relay_hint,
            weight_kg,
            reps,
            rpe,
            set_type,
            set_number,
        })
    }

    /// The template d-tag slug extracted from the coordinate.
    pub fn d_tag(&self) -> &str {
        self.reference.rsplit(':').next().unwrap_or(&self.reference)
    }

    /// Weight × reps when both are known and positive.
    pub fn volume_kg(&self) -> Option<f64> {
        match (self.weight_kg, self.reps) {
            (Some(w), Some(r)) if w > 0.0 && r > 0 => Some(w * f64::from(r)),
            _ => None,
        }
    }
}

/// The sets logged for one exercise within a POWR workout, grouped by the
/// referenced exercise-template coordinate and ordered by set number.
#[derive(Clone, Debug, PartialEq)]
pub struct ExerciseGroup {
    pub reference: String,
    pub sets: Vec<ExerciseSet>,
}

impl ExerciseGroup {
    /// Best-effort human label derived from the template d-tag
    /// (`back-squat-bb` → "Back Squat Bb").
    pub fn display_name(&self) -> Option<String> {
        let d = self.sets.first().map(|s| s.d_tag().to_string())?;
        if d.is_empty() {
            None
        } else {
            Some(slug_to_title(&d))
        }
    }

    /// Sum of per-set volume; `None` when no set has both weight and reps.
    pub fn total_volume_kg(&self) -> Option<f64> {
        let mut total = 0.0;
        let mut any = false;
        for set in &self.sets {
            if let Some(v) = set.volume_kg() {
                total += v;
                any = true;
            }
        }
        if any {
            Some(total)
        } else {
            None
        }
    }

    /// Largest positive per-set weight.
    #[allow(dead_code)]
    pub fn top_weight_kg(&self) -> Option<f64> {
        self.sets
            .iter()
            .filter_map(|s| s.weight_kg)
            .filter(|w| *w > 0.0)
            .fold(None::<f64>, |acc, w| Some(acc.map_or(w, |a| a.max(w))))
    }
}

/// Group per-set tags by template coordinate in first-seen order, sorting
/// each group by set number (missing numbers sort last).
pub fn group_exercise_sets(sets: Vec<ExerciseSet>) -> Vec<ExerciseGroup> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Vec<ExerciseSet>> = HashMap::new();
    for set in sets {
        if !map.contains_key(&set.reference) {
            order.push(set.reference.clone());
        }
        map.entry(set.reference.clone()).or_default().push(set);
    }
    order
        .into_iter()
        .map(|reference| {
            let mut group_sets = map.remove(&reference).unwrap_or_default();
            group_sets.sort_by_key(|s| s.set_number.unwrap_or(u32::MAX));
            ExerciseGroup {
                reference,
                sets: group_sets,
            }
        })
        .collect()
}

/// Turn a NIP-101e d-tag slug (`seated-calf-raise-machine`) into a title
/// (`Seated Calf Raise Machine`).
pub fn slug_to_title(slug: &str) -> String {
    slug.split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A parsed kind-1301 workout record. Fields from both dialects coexist;
/// see [WorkoutRecord::activity_type] and
/// [WorkoutRecord::effective_duration_seconds] for the bridging rules.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct WorkoutRecord {
    pub event_id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub content: String,
    pub title: Option<String>,
    /// RUNSTR activity verb (plain, non-coordinate `exercise` tag).
    pub exercise: Option<String>,
    pub duration_seconds: Option<u64>,
    pub distance: Option<Distance>,
    pub elevation_gain: Option<Elevation>,
    pub elevation_loss: Option<Elevation>,
    pub calories: Option<u32>,
    pub steps: Option<u32>,
    pub avg_heart_rate: Option<u32>,
    pub max_heart_rate: Option<u32>,
    pub splits: Vec<Split>,
    /// RUNSTR strength summary totals.
    pub sets: Option<u32>,
    pub reps: Option<u32>,
    pub weight: Option<Weight>,
    pub source: Option<String>,
    pub workout_start_time: Option<u64>,
    // --- POWR / NIP-101e strength dialect ---
    pub workout_type_code: Option<String>,
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub completed: Option<bool>,
    pub exercise_sets: Vec<ExerciseSet>,
    /// `["template", "33402:pubkey:d-tag", relay]` reference.
    pub template: Option<(String, Option<String>)>,
    pub client: Option<String>,
}

impl WorkoutRecord {
    #[allow(dead_code)]
    pub fn is_workout_event(event: &Event) -> bool {
        event.kind == Kind::from(KIND_WORKOUT)
    }

    /// Activity type, preferring the POWR `type` tag and falling back to
    /// the RUNSTR `exercise` verb.
    pub fn activity_type(&self) -> Option<ExerciseType> {
        self.workout_type_code
            .as_deref()
            .and_then(ExerciseType::parse)
            .or_else(|| self.exercise.as_deref().and_then(ExerciseType::parse))
    }

    /// Duration in seconds: the explicit `duration` tag if present
    /// (RUNSTR), otherwise derived from the POWR `start`/`end` session
    /// timestamps.
    pub fn effective_duration_seconds(&self) -> Option<u64> {
        if let Some(d) = self.duration_seconds {
            return Some(d);
        }
        match (self.start, self.end) {
            (Some(s), Some(e)) if e > s => Some(e - s),
            _ => None,
        }
    }

    pub fn exercise_groups(&self) -> Vec<ExerciseGroup> {
        group_exercise_sets(self.exercise_sets.clone())
    }

    /// The `source` tag, else the `client` tag (e.g. "RUNSTR", "POWR").
    pub fn source_or_client(&self) -> Option<&str> {
        self.source.as_deref().or(self.client.as_deref())
    }

    /// Kind-33401/33402 address coordinates referenced by this workout
    /// (per-set templates + the workout template), with relay hints.
    #[allow(dead_code)]
    pub fn template_hints(&self) -> Vec<(String, Option<String>)> {
        let mut hints: Vec<(String, Option<String>)> = self
            .exercise_sets
            .iter()
            .map(|s| (s.reference.clone(), s.relay_hint.clone()))
            .collect();
        if let Some((reference, relay)) = &self.template {
            hints.push((reference.clone(), relay.clone()));
        }
        hints
    }
}

/// Parse a kind-1301 workout record. Every tag is optional.
pub fn parse_workout(event: &Event) -> Result<WorkoutRecord, String> {
    if event.kind != Kind::from(KIND_WORKOUT) {
        return Err(format!(
            "Expected kind {} workout, got {}",
            KIND_WORKOUT,
            event.kind.as_u16()
        ));
    }
    let mut record = WorkoutRecord {
        event_id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs(),
        content: event.content.clone(),
        title: None,
        exercise: None,
        duration_seconds: None,
        distance: None,
        elevation_gain: None,
        elevation_loss: None,
        calories: None,
        steps: None,
        avg_heart_rate: None,
        max_heart_rate: None,
        splits: Vec::new(),
        sets: None,
        reps: None,
        weight: None,
        source: None,
        workout_start_time: None,
        workout_type_code: None,
        start: None,
        end: None,
        completed: None,
        exercise_sets: Vec::new(),
        template: None,
        client: None,
    };
    for tag in event.tags.iter() {
        let values = tag.as_slice();
        let name = values.first().map(|s| s.as_str()).unwrap_or("");
        match name {
            "title" => {
                record.title = values.get(1).filter(|s| !s.is_empty()).cloned();
            }
            "exercise" => {
                if let Some(value) = values.get(1) {
                    if is_coordinate(value) {
                        if let Some(set) = ExerciseSet::parse(values) {
                            record.exercise_sets.push(set);
                        }
                    } else if !value.is_empty() {
                        record.exercise = Some(value.clone());
                    }
                }
            }
            "duration" => {
                record.duration_seconds = values.get(1).and_then(|s| parse_duration_time(s));
            }
            "distance" => {
                record.distance = Distance::parse(values);
            }
            "elevation_gain" => {
                record.elevation_gain = Elevation::parse(values);
            }
            "elevation_loss" => {
                record.elevation_loss = Elevation::parse(values);
            }
            "calories" => {
                record.calories = values.get(1).and_then(|s| s.parse().ok());
            }
            "steps" => {
                record.steps = values.get(1).and_then(|s| s.parse().ok());
            }
            "avg_heart_rate" => {
                record.avg_heart_rate = values.get(1).and_then(|s| s.parse().ok());
            }
            "max_heart_rate" => {
                record.max_heart_rate = values.get(1).and_then(|s| s.parse().ok());
            }
            "split" => {
                if let Some(split) = Split::parse(values) {
                    record.splits.push(split);
                }
            }
            "sets" => {
                record.sets = values.get(1).and_then(|s| s.parse().ok());
            }
            "reps" => {
                record.reps = values.get(1).and_then(|s| s.parse().ok());
            }
            "weight" => {
                record.weight = Weight::parse(values);
            }
            "source" => {
                record.source = values.get(1).filter(|s| !s.is_empty()).cloned();
            }
            "workout_start_time" => {
                record.workout_start_time = values.get(1).and_then(|s| s.parse().ok());
            }
            "type" => {
                record.workout_type_code = values.get(1).filter(|s| !s.is_empty()).cloned();
            }
            "start" => {
                record.start = values.get(1).and_then(|s| s.parse().ok());
            }
            "end" => {
                record.end = values.get(1).and_then(|s| s.parse().ok());
            }
            "completed" => {
                record.completed = values.get(1).and_then(|s| s.parse::<bool>().ok());
            }
            "template" => {
                if let Some(reference) = values.get(1) {
                    if is_coordinate(reference) {
                        let relay = values
                            .get(2)
                            .and_then(|r| if r.is_empty() { None } else { Some(r.clone()) });
                        record.template = Some((reference.clone(), relay));
                    }
                }
            }
            "client" => {
                record.client = values.get(1).filter(|s| !s.is_empty()).cloned();
            }
            _ => {}
        }
    }
    Ok(record)
}

/// A parsed kind-33401 exercise template.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ExerciseTemplate {
    pub event_id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub d_tag: String,
    pub content: String,
    pub title: Option<String>,
    /// Ordered parameter names per set, e.g. `["weight","reps","rpe","set_type"]`.
    pub format: Vec<String>,
    /// Units for each format parameter, e.g. `["kg","count","0-10","enum"]`.
    pub format_units: Vec<String>,
    /// `barbell` | `dumbbell` | `bodyweight` | `machine` | `cardio`.
    pub equipment: Option<String>,
    /// `beginner` | `intermediate` | `advanced`.
    pub difficulty: Option<String>,
}

impl ExerciseTemplate {
    #[allow(dead_code)]
    pub fn is_template_event(event: &Event) -> bool {
        event.kind == Kind::from(KIND_EXERCISE_TEMPLATE)
    }
}

/// Parse a kind-33401 exercise template. Every tag is optional.
pub fn parse_exercise_template(event: &Event) -> Result<ExerciseTemplate, String> {
    if event.kind != Kind::from(KIND_EXERCISE_TEMPLATE) {
        return Err(format!(
            "Expected kind {} exercise template, got {}",
            KIND_EXERCISE_TEMPLATE,
            event.kind.as_u16()
        ));
    }
    let mut template = ExerciseTemplate {
        event_id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs(),
        d_tag: event
            .tags
            .identifier()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        content: event.content.clone(),
        title: None,
        format: Vec::new(),
        format_units: Vec::new(),
        equipment: None,
        difficulty: None,
    };
    for tag in event.tags.iter() {
        let values = tag.as_slice();
        let name = values.first().map(|s| s.as_str()).unwrap_or("");
        match name {
            "title" => {
                template.title = values.get(1).filter(|s| !s.is_empty()).cloned();
            }
            "format" => {
                template.format = values[1..]
                    .iter()
                    .filter(|v| !v.is_empty())
                    .cloned()
                    .collect();
            }
            "format_units" => {
                template.format_units = values[1..]
                    .iter()
                    .filter(|v| !v.is_empty())
                    .cloned()
                    .collect();
            }
            "equipment" => {
                template.equipment = values.get(1).filter(|s| !s.is_empty()).cloned();
            }
            "difficulty" => {
                template.difficulty = values.get(1).filter(|s| !s.is_empty()).cloned();
            }
            _ => {}
        }
    }
    Ok(template)
}

/// Input for building a kind-1301 workout event (RUNSTR-canonical wire
/// format). All optional fields are emitted as tags only when present.
pub struct WorkoutDraft {
    pub exercise: ExerciseType,
    pub duration_seconds: u64,
    pub notes: String,
    pub title: Option<String>,
    pub source: Option<String>,
    /// (value, unit) with unit `km` or `mi`.
    pub distance: Option<(f64, String)>,
    pub calories: Option<u32>,
    pub avg_heart_rate: Option<u32>,
    pub max_heart_rate: Option<u32>,
    pub steps: Option<u32>,
    /// Meters; emitted as an `elevation_gain` tag with unit `m`.
    pub elevation_gain_meters: Option<f64>,
    pub workout_start_time: Option<u64>,
}

fn custom_tag(name: &str, values: &[&str]) -> Tag {
    Tag::custom(TagKind::custom(name), values.iter().map(|v| v.to_string()))
}

/// Print a float without a trailing `.0` when it is a whole number.
fn trim_float(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Build a kind-1301 workout [EventBuilder] from a draft. Emits the
/// canonical RUNSTR layout: `d` (UUID), `exercise` verb, capitalized `t`
/// hashtag, `duration` (HH:MM:SS), plus any optional tags. The content is
/// the plain-text notes.
pub fn build_workout_event(draft: &WorkoutDraft, workout_id: String) -> EventBuilder {
    let mut tags = vec![
        Tag::identifier(workout_id),
        custom_tag("exercise", &[draft.exercise.code()]),
        custom_tag("t", &[draft.exercise.hashtag()]),
        custom_tag("duration", &[&format_duration_time(draft.duration_seconds)]),
    ];
    if let Some(title) = &draft.title {
        let t = title.as_str();
        tags.push(custom_tag("title", &[t]));
    }
    if let Some(source) = &draft.source {
        let s = source.as_str();
        tags.push(custom_tag("source", &[s]));
    }
    if let Some((value, unit)) = &draft.distance {
        let v = trim_float(*value);
        let u = unit.as_str();
        tags.push(custom_tag("distance", &[&v, u]));
    }
    if let Some(calories) = draft.calories {
        tags.push(custom_tag("calories", &[&calories.to_string()]));
    }
    if let Some(bpm) = draft.avg_heart_rate {
        tags.push(custom_tag("avg_heart_rate", &[&bpm.to_string()]));
    }
    if let Some(bpm) = draft.max_heart_rate {
        tags.push(custom_tag("max_heart_rate", &[&bpm.to_string()]));
    }
    if let Some(steps) = draft.steps {
        tags.push(custom_tag("steps", &[&steps.to_string()]));
    }
    if let Some(gain) = draft.elevation_gain_meters {
        let g = trim_float(gain);
        tags.push(custom_tag("elevation_gain", &[&g, "m"]));
    }
    if let Some(ts) = draft.workout_start_time {
        tags.push(custom_tag("workout_start_time", &[&ts.to_string()]));
    }
    EventBuilder::new(Kind::from(KIND_WORKOUT), draft.notes.clone()).tags(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(kind: u16, content: &str, tags: Vec<Tag>) -> Event {
        let keys = Keys::generate();
        EventBuilder::new(Kind::from(kind), content)
            .tags(tags)
            .sign_with_keys(&keys)
            .unwrap()
    }

    fn t(name: &str, values: &[&str]) -> Tag {
        Tag::parse(std::iter::once(name).chain(values.iter().copied()))
            .unwrap()
    }

    #[test]
    fn parses_runstr_dialect() {
        let event = signed(
            KIND_WORKOUT,
            "Felt great today",
            vec![
                t("d", &["b3f5c0e2-1d4a-4f6b-9c8e-7a2d5b1c4e9f"]),
                t("title", &["Morning Run"]),
                t("exercise", &["running"]),
                t("distance", &["5.20", "km"]),
                t("duration", &["00:31:30"]),
                t("elevation_gain", &["75", "m"]),
                t("elevation_loss", &["20", "m"]),
                t("calories", &["312"]),
                t("steps", &["8421"]),
                t("source", &["gps"]),
                t("client", &["RUNSTR", "1.0.5"]),
                t("t", &["Running"]),
                t("split", &["1", "00:06:01"]),
                t("split", &["2", "00:12:10"]),
            ],
        );
        assert!(WorkoutRecord::is_workout_event(&event));
        let workout = parse_workout(&event).unwrap();
        assert_eq!(
            workout.title.as_deref(),
            Some("Morning Run"),
            "title tag should parse"
        );
        assert_eq!(workout.exercise.as_deref(), Some("running"));
        assert_eq!(workout.activity_type(), Some(ExerciseType::Running));
        assert_eq!(workout.duration_seconds, Some(31 * 60 + 30));
        let distance = workout.distance.as_ref().unwrap();
        assert!((distance.to_meters() - 5200.0).abs() < 1e-9);
        assert!((workout.elevation_gain.as_ref().unwrap().to_meters() - 75.0).abs() < 1e-9);
        assert!((workout.elevation_loss.as_ref().unwrap().to_meters() - 20.0).abs() < 1e-9);
        assert_eq!(workout.calories, Some(312));
        assert_eq!(workout.steps, Some(8421));
        assert_eq!(workout.source.as_deref(), Some("gps"));
        assert_eq!(
            workout.source_or_client(),
            Some("gps"),
            "source tag wins over client"
        );
        assert_eq!(workout.content, "Felt great today");
        assert_eq!(workout.splits.len(), 2);
        assert_eq!(workout.splits[0].cumulative_seconds, 361);
        assert_eq!(workout.splits[1].cumulative_seconds, 730);
    }

    #[test]
    fn parses_lax_units_and_raw_seconds() {
        let event = signed(
            KIND_WORKOUT,
            "",
            vec![
                t("d", &["id-2"]),
                t("exercise", &["Running"]),
                t("distance", &["3.1", "mi"]),
                t("duration", &["1800"]),
                t("elevation_gain", &["100", "ft"]),
            ],
        );
        let workout = parse_workout(&event).unwrap();
        // Capitalized verb is tolerated (case-insensitive parse).
        assert_eq!(workout.activity_type(), Some(ExerciseType::Running));
        // Raw-seconds duration.
        assert_eq!(workout.duration_seconds, Some(1800));
        assert_eq!(workout.effective_duration_seconds(), Some(1800));
        // Miles convert.
        assert!(
            (workout.distance.unwrap().to_meters() - 3.1 * METERS_PER_MILE).abs() < 1e-9,
            "mi should convert to meters"
        );
        // Feet convert.
        assert!(
            (workout.elevation_gain.unwrap().to_meters() - 30.48).abs() < 1e-9,
            "ft should convert to meters"
        );
        // Absent title -> None.
        assert_eq!(workout.title, None);
    }

    #[test]
    fn parses_strength_totals() {
        let event = signed(
            KIND_WORKOUT,
            "",
            vec![
                t("d", &["id-3"]),
                t("exercise", &["strength"]),
                t("duration", &["01:00:00"]),
                t("sets", &["5"]),
                t("reps", &["10"]),
                t("weight", &["165", "lbs"]),
            ],
        );
        let workout = parse_workout(&event).unwrap();
        assert_eq!(workout.activity_type(), Some(ExerciseType::Strength));
        assert_eq!(workout.sets, Some(5));
        assert_eq!(workout.reps, Some(10));
        let kg = workout.weight.unwrap().to_kilograms();
        assert!((kg - 165.0 * KILOGRAMS_PER_POUND).abs() < 1e-9);
        assert!(workout.distance.is_none());
    }

    #[test]
    fn parses_powr_dialect() {
        let keys = Keys::generate();
        let pk = keys.public_key().to_hex();
        let back_squat = format!("33401:{}:back-squat-bb", pk);
        let calf_raise = format!("33401:{}:seated-calf-raise-machine", pk);
        let template = format!("33402:{}:full-body-a", pk);
        let event = EventBuilder::new(Kind::from(KIND_WORKOUT), "")
            .tags(vec![
                t("d", &["powr-1"]),
                t("type", &["strength"]),
                t("start", &["1781969106"]),
                t("end", &["1781972319"]),
                t("completed", &["true"]),
                t(
                    "template",
                    &[template.as_str(), "wss://relay.powr.build/"],
                ),
                t("exercise", &[back_squat.as_str(), "", "84", "8", "8", "normal", "1"]),
                t("exercise", &[back_squat.as_str(), "", "84", "8", "8", "normal", "2"]),
                t("exercise", &[back_squat.as_str(), "", "84", "8", "8", "normal", "3"]),
                t("exercise", &[calf_raise.as_str(), "", "", "15", "6", "normal", "4"]),
                t("client", &["POWR"]),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let workout = parse_workout(&event).unwrap();
        // The POWR `type` tag classifies the workout; the coordinate form
        // must not surface as a verb.
        assert_eq!(workout.workout_type_code.as_deref(), Some("strength"));
        assert_eq!(workout.activity_type(), Some(ExerciseType::Strength));
        assert_eq!(workout.exercise, None, "coordinate must not surface as verb");
        assert_eq!(workout.start, Some(1_781_969_106));
        assert_eq!(workout.end, Some(1_781_972_319));
        assert_eq!(workout.completed, Some(true));
        assert_eq!(workout.client.as_deref(), Some("POWR"));
        assert_eq!(
            workout.effective_duration_seconds(),
            Some(1_781_972_319 - 1_781_969_106),
            "duration derived from start/end"
        );
        assert_eq!(workout.exercise_sets.len(), 4);
        let first = &workout.exercise_sets[0];
        assert!((first.weight_kg.unwrap() - 84.0).abs() < 1e-9);
        assert_eq!(first.reps, Some(8));
        assert!((first.rpe.unwrap() - 8.0).abs() < 1e-9);
        assert_eq!(first.set_type.as_deref(), Some("normal"));
        assert_eq!(first.set_number, Some(1));
        assert_eq!(first.d_tag(), "back-squat-bb");
        let bodyweight = &workout.exercise_sets[3];
        assert_eq!(bodyweight.weight_kg, None, "empty weight = bodyweight");
        assert_eq!(bodyweight.reps, Some(15));
        // Grouping.
        let groups = workout.exercise_groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].display_name().as_deref(), Some("Back Squat Bb"));
        assert_eq!(groups[0].sets.len(), 3);
        let expected_volume = 84.0 * 8.0 * 3.0;
        assert!((groups[0].total_volume_kg().unwrap() - expected_volume).abs() < 1e-9);
        assert!((groups[0].top_weight_kg().unwrap() - 84.0).abs() < 1e-9);
        assert_eq!(
            groups[1].display_name().as_deref(),
            Some("Seated Calf Raise Machine")
        );
        assert_eq!(
            groups[1].total_volume_kg(),
            None,
            "bodyweight-only group has no volume"
        );
    }

    #[test]
    fn exposes_powr_template_references() {
        let keys = Keys::generate();
        let pk = keys.public_key().to_hex();
        let back_squat = format!("33401:{}:back-squat-bb", pk);
        let deadlift = format!("33401:{}:deadlift-bb", pk);
        let template = format!("33402:{}:full-body-a", pk);
        let event = EventBuilder::new(Kind::from(KIND_WORKOUT), "")
            .tags(vec![
                t("d", &["powr-2"]),
                t("type", &["strength"]),
                t("template", &[template.as_str(), "wss://relay.powr.build/"]),
                t("exercise", &[back_squat.as_str(), "wss://relay.powr.build/"]),
                t("exercise", &[deadlift.as_str(), "wss://relay.powr.build/"]),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let workout = parse_workout(&event).unwrap();
        let hints = workout.template_hints();
        let references: Vec<&str> = hints.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(references.len(), 3, "references should dedupe");
        assert!(references.contains(&back_squat.as_str()));
        assert!(references.contains(&deadlift.as_str()));
        assert!(references.contains(&template.as_str()));
        assert!(
            hints.iter().all(|(_, relay)| relay.as_deref() == Some("wss://relay.powr.build/")),
            "relay hints preserved"
        );
    }

    #[test]
    fn parses_exercise_template() {
        let event = signed(
            KIND_EXERCISE_TEMPLATE,
            "Barbell back squat",
            vec![
                t("d", &["back-squat-bb"]),
                t("title", &["Back Squat (Barbell)"]),
                t("format", &["weight", "reps", "rpe", "set_type"]),
                t("format_units", &["kg", "count", "0-10", "enum"]),
                t("equipment", &["barbell"]),
                t("difficulty", &["intermediate"]),
            ],
        );
        assert!(ExerciseTemplate::is_template_event(&event));
        let template = parse_exercise_template(&event).unwrap();
        assert_eq!(template.d_tag, "back-squat-bb");
        assert_eq!(template.title.as_deref(), Some("Back Squat (Barbell)"));
        assert_eq!(
            template.format,
            vec!["weight", "reps", "rpe", "set_type"]
        );
        assert_eq!(template.format_units, vec!["kg", "count", "0-10", "enum"]);
        assert_eq!(template.equipment.as_deref(), Some("barbell"));
        assert_eq!(template.difficulty.as_deref(), Some("intermediate"));
    }

    #[test]
    fn slug_to_title_prettifies_d_tags() {
        assert_eq!(slug_to_title("back-squat-bb"), "Back Squat Bb");
        assert_eq!(slug_to_title("seated-calf-raise-machine"), "Seated Calf Raise Machine");
        assert_eq!(slug_to_title("bench_press db"), "Bench Press Db");
        assert_eq!(slug_to_title(""), "");
    }

    #[test]
    fn duration_formats_as_padded_time() {
        assert_eq!(format_duration_time(1890), "00:31:30");
        assert_eq!(parse_duration_time("02:05"), Some(125));
        assert_eq!(parse_duration_time("1:01:01"), Some(3661));
        assert_eq!(parse_duration_time("1800"), Some(1800));
        assert_eq!(parse_duration_time(""), None);
        assert_eq!(parse_duration_time("a:b"), None);
        assert_eq!(parse_duration_time("1:2:3:4"), None);
    }

    #[test]
    fn build_emits_canonical_tags() {
        let draft = WorkoutDraft {
            exercise: ExerciseType::Running,
            duration_seconds: 31 * 60 + 30,
            notes: "Easy pace".to_string(),
            title: Some("Morning Run".to_string()),
            source: Some(SOURCE_MANUAL.to_string()),
            distance: Some((5.2, "km".to_string())),
            calories: Some(312),
            avg_heart_rate: None,
            max_heart_rate: None,
            steps: None,
            elevation_gain_meters: None,
            workout_start_time: None,
        };
        let keys = Keys::generate();
        let event = build_workout_event(&draft, "fixed-id".to_string())
            .sign_with_keys(&keys)
            .unwrap();
        assert_eq!(event.kind, Kind::from(KIND_WORKOUT));
        assert_eq!(event.content, "Easy pace");
        let expected: Vec<Vec<String>> = vec![
            vec!["d".into(), "fixed-id".into()],
            vec!["exercise".into(), "running".into()],
            // Hashtag keeps RUNSTR's capitalized casing (Tag::hashtag
            // would lowercase it).
            vec!["t".into(), "Running".into()],
            vec!["duration".into(), "00:31:30".into()],
            vec!["title".into(), "Morning Run".into()],
            vec!["source".into(), "manual".into()],
            vec!["distance".into(), "5.2".into(), "km".into()],
            vec!["calories".into(), "312".into()],
        ];
        let actual: Vec<Vec<String>> = event
            .tags
            .iter()
            .map(|tag| tag.as_slice().to_vec())
            .collect();
        assert_eq!(actual, expected, "exact canonical tag layout");
        // Round-trip: the built event parses back.
        let parsed = parse_workout(&event).unwrap();
        assert_eq!(parsed.title.as_deref(), Some("Morning Run"));
        assert_eq!(parsed.duration_seconds, Some(1890));
        assert_eq!(
            (parsed.distance.unwrap().to_kilometers() - 5.2).abs(),
            0.0,
            "distance round-trips losslessly"
        );
    }

    #[test]
    fn coordinate_detection_rejects_non_coordinates() {
        assert!(is_coordinate("33401:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:squat"));
        assert!(!is_coordinate("running"));
        assert!(!is_coordinate("33401:shortpk:squat"));
        assert!(!is_coordinate("notakind:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:squat"));
    }
}
