use crate::components::blobbi::core::types::BlobbiStage;

#[derive(Clone, Debug, PartialEq)]
pub struct TagSchema {
    pub name: &'static str,
    pub category: TagCategory,
    pub required: bool,
    pub stages: &'static [BlobbiStage],
    pub persistent: bool,
    pub deprecated: Option<DeprecatedInfo>,
    pub default_value: Option<&'static str>,
    pub format: Option<TagFormat>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagCategory {
    Identity,
    State,
    Stats,
    Visual,
    Personality,
    Timestamp,
    Task,
    Social,
    Egg,
    Progression,
    Misc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeprecatedInfo {
    pub reason: &'static str,
    pub replaced_by: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagFormat {
    Float,
    Integer,
    Boolean,
    String,
    Identifier,
    Timestamp,
    CommaList,
}

use crate::utils::nip_bb::constants::*;

pub fn blobbi_tag_schema() -> &'static [TagSchema] {
    BLOBBI_TAG_SCHEMAS
}

static BLOBBI_TAG_SCHEMAS: &[TagSchema] = &[
    TagSchema {
        name: TAG_STAGE,
        category: TagCategory::State,
        required: true,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: Some(STAGE_EGG),
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_STATE,
        category: TagCategory::State,
        required: true,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: Some(STATE_ACTIVE),
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_NAME,
        category: TagCategory::Identity,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_SEED,
        category: TagCategory::Identity,
        required: true,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_GENERATION,
        category: TagCategory::Identity,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: Some("0"),
        format: Some(TagFormat::Integer),
    },
    TagSchema {
        name: TAG_BREEDING_READY,
        category: TagCategory::State,
        required: false,
        stages: &[BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: Some("false"),
        format: Some(TagFormat::Boolean),
    },
    TagSchema {
        name: TAG_IS_SLEEPING,
        category: TagCategory::State,
        required: false,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: Some("false"),
        format: Some(TagFormat::Boolean),
    },
    TagSchema {
        name: TAG_HUNGER,
        category: TagCategory::Stats,
        required: true,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: false,
        deprecated: None,
        default_value: Some("100"),
        format: Some(TagFormat::Float),
    },
    TagSchema {
        name: TAG_HAPPINESS,
        category: TagCategory::Stats,
        required: true,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: false,
        deprecated: None,
        default_value: Some("100"),
        format: Some(TagFormat::Float),
    },
    TagSchema {
        name: TAG_HEALTH,
        category: TagCategory::Stats,
        required: true,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: false,
        deprecated: None,
        default_value: Some("100"),
        format: Some(TagFormat::Float),
    },
    TagSchema {
        name: TAG_HYGIENE,
        category: TagCategory::Stats,
        required: true,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: false,
        deprecated: None,
        default_value: Some("100"),
        format: Some(TagFormat::Float),
    },
    TagSchema {
        name: TAG_ENERGY,
        category: TagCategory::Stats,
        required: true,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: false,
        deprecated: None,
        default_value: Some("100"),
        format: Some(TagFormat::Float),
    },
    TagSchema {
        name: TAG_EXPERIENCE,
        category: TagCategory::Progression,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: Some("0"),
        format: Some(TagFormat::Integer),
    },
    TagSchema {
        name: TAG_CARE_STREAK,
        category: TagCategory::Progression,
        required: false,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: Some("0"),
        format: Some(TagFormat::Integer),
    },
    TagSchema {
        name: TAG_BASE_COLOR,
        category: TagCategory::Visual,
        required: true,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_EYE_COLOR,
        category: TagCategory::Visual,
        required: false,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_PATTERN,
        category: TagCategory::Visual,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: Some("solid"),
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_SPECIAL_MARK,
        category: TagCategory::Visual,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: Some("none"),
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_SIZE,
        category: TagCategory::Visual,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: Some("medium"),
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_ADULT_TYPE,
        category: TagCategory::Visual,
        required: false,
        stages: &[BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_LAST_INTERACTION,
        category: TagCategory::Timestamp,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::Timestamp),
    },
    TagSchema {
        name: TAG_CARE_STREAK_LAST_AT,
        category: TagCategory::Timestamp,
        required: false,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::Timestamp),
    },
    TagSchema {
        name: TAG_TRAIT,
        category: TagCategory::Personality,
        required: false,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_MOOD,
        category: TagCategory::Personality,
        required: false,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_THEME,
        category: TagCategory::Misc,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_STATE_STARTED_AT,
        category: TagCategory::Timestamp,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::Timestamp),
    },
    TagSchema {
        name: TAG_LAST_DECAY_AT,
        category: TagCategory::Timestamp,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: false,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::Timestamp),
    },
    TagSchema {
        name: TAG_PERSONALITY,
        category: TagCategory::Personality,
        required: false,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_SECONDARY_COLOR,
        category: TagCategory::Visual,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_FAVORITE_FOOD,
        category: TagCategory::Personality,
        required: false,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_VOICE_TYPE,
        category: TagCategory::Personality,
        required: false,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_CARE_STREAK_LAST_DAY,
        category: TagCategory::Timestamp,
        required: false,
        stages: &[BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_CROSSOVER_APP,
        category: TagCategory::Misc,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::String),
    },
    TagSchema {
        name: TAG_D,
        category: TagCategory::Identity,
        required: true,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::Identifier),
    },
    TagSchema {
        name: TAG_B,
        category: TagCategory::Identity,
        required: false,
        stages: &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult],
        persistent: true,
        deprecated: None,
        default_value: None,
        format: Some(TagFormat::Identifier),
    },
];

pub fn deprecated_tag_names() -> Vec<&'static str> {
    vec![
        "t",
        "client",
        "shell_integrity",
        "egg_temperature",
        "incubation_progress",
        "egg_status",
        "fees",
        "incubation_time",
        "start_incubation",
        "interact_6_progress",
    ]
}

pub fn transition_cleanup_tag_names() -> Vec<&'static str> {
    vec![
        "task",
        "task_completed",
        TAG_STATE_STARTED_AT,
    ]
}

pub const NEVER_INVENT_TAGS: &[&str] = &[
    "personality",
    "trait",
    "favorite_food",
    "voice_type",
    "mood",
    "adult_type",
    "theme",
    "crossover_app",
    "name",
    "seed",
    "d",
];

pub fn valid_states_for_stage(stage: BlobbiStage) -> Vec<&'static str> {
    match stage {
        BlobbiStage::Egg => vec![STATE_ACTIVE, STATE_SLEEPING, STATE_HIBERNATING, STATE_INCUBATING],
        BlobbiStage::Baby => vec![STATE_ACTIVE, STATE_SLEEPING, STATE_HIBERNATING, STATE_EVOLVING],
        BlobbiStage::Adult => vec![STATE_ACTIVE, STATE_SLEEPING, STATE_HIBERNATING],
    }
}

pub fn is_task_process_state(state: &str) -> bool {
    matches!(state, STATE_INCUBATING | STATE_EVOLVING)
}

pub fn required_tag_names(stage: BlobbiStage) -> Vec<&'static str> {
    BLOBBI_TAG_SCHEMAS
        .iter()
        .filter(|s| s.required && s.stages.contains(&stage))
        .map(|s| s.name)
        .collect()
}

pub fn persistent_tag_names() -> Vec<&'static str> {
    BLOBBI_TAG_SCHEMAS
        .iter()
        .filter(|s| s.persistent)
        .map(|s| s.name)
        .collect()
}

pub fn tags_for_stage(stage: BlobbiStage) -> Vec<&'static str> {
    BLOBBI_TAG_SCHEMAS
        .iter()
        .filter(|s| s.stages.contains(&stage))
        .map(|s| s.name)
        .collect()
}

pub fn get_default_value(tag_name: &str) -> Option<&'static str> {
    BLOBBI_TAG_SCHEMAS
        .iter()
        .find(|s| s.name == tag_name)
        .and_then(|s| s.default_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_non_identity_tags_have_defaults() {
        for schema in BLOBBI_TAG_SCHEMAS {
            if schema.required
                && !matches!(schema.category, TagCategory::Identity | TagCategory::Visual)
            {
                assert!(
                    schema.default_value.is_some(),
                    "Required non-identity tag '{}' missing default_value",
                    schema.name
                );
            }
        }
    }

    #[test]
    fn deprecated_tags_have_reason() {
        for schema in BLOBBI_TAG_SCHEMAS {
            if let Some(dep) = &schema.deprecated {
                assert!(!dep.reason.is_empty(), "Deprecated tag '{}' has empty reason", schema.name);
            }
        }
    }

    #[test]
    fn schema_covers_all_stages() {
        for stage in &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult] {
            let tags = tags_for_stage(*stage);
            assert!(!tags.is_empty(), "No tags for stage {:?}", stage);
        }
    }

    #[test]
    fn no_duplicate_tag_names() {
        let mut seen = std::collections::HashSet::new();
        for schema in BLOBBI_TAG_SCHEMAS {
            assert!(
                seen.insert(schema.name),
                "Duplicate tag name: {}",
                schema.name
            );
        }
    }

    #[test]
    fn required_tag_lookup_works() {
        let egg_required = required_tag_names(BlobbiStage::Egg);
        assert!(egg_required.contains(&TAG_STAGE));
        assert!(egg_required.contains(&TAG_STATE));
    }

    #[test]
    fn persistent_tags_present() {
        let persistent = persistent_tag_names();
        assert!(persistent.contains(&TAG_STAGE));
        assert!(!persistent.is_empty());
    }

    #[test]
    fn deprecated_tags_are_documented() {
        let dep = deprecated_tag_names();
        assert!(!dep.is_empty(), "Should have deprecated tags");
    }

    #[test]
    fn never_invent_tags_are_valid() {
        for tag in NEVER_INVENT_TAGS {
            assert!(!tag.is_empty(), "NEVER_INVENT tag should not be empty");
        }
    }

    #[test]
    fn valid_states_cover_all_stages() {
        for stage in &[BlobbiStage::Egg, BlobbiStage::Baby, BlobbiStage::Adult] {
            let states = valid_states_for_stage(*stage);
            assert!(!states.is_empty(), "No valid states for {:?}", stage);
            assert!(states.contains(&STATE_ACTIVE), "{:?} should allow active", stage);
        }
    }

    #[test]
    fn task_process_states_detected() {
        assert!(is_task_process_state(STATE_INCUBATING));
        assert!(is_task_process_state(STATE_EVOLVING));
        assert!(!is_task_process_state(STATE_ACTIVE));
        assert!(!is_task_process_state(STATE_SLEEPING));
    }
}
