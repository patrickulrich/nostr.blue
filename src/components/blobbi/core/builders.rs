use crate::components::blobbi::core::types::*;
use crate::utils::nip_bb::*;
use nostr_sdk::{EventBuilder, Tag, TagKind, Timestamp};

fn sorted_tags(tags: Vec<Tag>) -> Vec<Tag> {
    let mut tags = tags;
    tags.sort_by_key(|t| tag_priority(&t.kind().to_string()));
    tags
}

pub struct StateTags {
    tags: Vec<Tag>,
}

impl StateTags {
    pub fn new(blobbi: &BlobbiCompanion) -> Self {
        let mut tags = Vec::new();
        let now = Timestamp::now().as_secs();

        tags.push(Tag::identifier(&blobbi.d));
        tags.push(Tag::custom(TagKind::custom(TAG_B), vec![BLOBBI_ECOSYSTEM_TAG.to_string()]));
        tags.push(Tag::custom(TagKind::custom(TAG_T), vec![BLOBBI_TOPIC_TAG.to_string()]));
        tags.push(Tag::custom(TagKind::custom(TAG_CLIENT), vec![CLIENT_TAG.to_string()]));

        if let Some(ref source) = blobbi.source {
            tags.push(Tag::custom(TagKind::custom(TAG_SOURCE), vec![source.clone()]));
        }

        if !blobbi.name.is_empty() {
            tags.push(Tag::custom(TagKind::custom(TAG_NAME), vec![blobbi.name.clone()]));
        }

        if let Some(ref seed) = blobbi.seed {
            tags.push(Tag::custom(TagKind::custom(TAG_SEED), vec![seed.clone()]));
        }

        tags.push(Tag::custom(TagKind::custom(TAG_STAGE), vec![blobbi.stage.as_str().to_string()]));
        tags.push(Tag::custom(TagKind::custom(TAG_STATE), vec![blobbi.state.as_str().to_string()]));
        tags.push(Tag::custom(
            TagKind::custom(TAG_BREEDING_READY),
            vec![if blobbi.breeding_ready { "true" } else { "false" }.to_string()],
        ));
        tags.push(Tag::custom(TagKind::custom(TAG_GENERATION), vec![blobbi.generation.to_string()]));

        tags.push(Tag::custom(
            TagKind::custom(TAG_IS_SLEEPING),
            vec![if blobbi.is_sleeping { "true" } else { "false" }.to_string()],
        ));

        if blobbi.is_sleeping {
            if let Some(v) = blobbi.sleep_started_at {
                tags.push(Tag::custom(TagKind::custom(TAG_SLEEP_STARTED_AT), vec![v.to_string()]));
            }
            if let Some(v) = blobbi.last_sleep_update {
                tags.push(Tag::custom(TagKind::custom(TAG_LAST_SLEEP_UPDATE), vec![v.to_string()]));
            }
        }

        tags.push(Tag::custom(TagKind::custom(TAG_HUNGER), vec![blobbi.stats.hunger.to_string()]));
        tags.push(Tag::custom(TagKind::custom(TAG_HAPPINESS), vec![blobbi.stats.happiness.to_string()]));
        tags.push(Tag::custom(TagKind::custom(TAG_HEALTH), vec![blobbi.stats.health.to_string()]));
        tags.push(Tag::custom(TagKind::custom(TAG_HYGIENE), vec![blobbi.stats.hygiene.to_string()]));
        tags.push(Tag::custom(TagKind::custom(TAG_ENERGY), vec![blobbi.stats.energy.to_string()]));

        tags.push(Tag::custom(TagKind::custom(TAG_EXPERIENCE), vec![blobbi.experience.to_string()]));
        tags.push(Tag::custom(TagKind::custom(TAG_CARE_STREAK), vec![blobbi.care_streak.to_string()]));

        let last_interaction = blobbi.last_interaction.unwrap_or(now);
        tags.push(Tag::custom(TagKind::custom(TAG_LAST_INTERACTION), vec![last_interaction.to_string()]));

        tags.push(Tag::custom(TagKind::custom(TAG_BASE_COLOR), vec![blobbi.visual_traits.base_color.clone()]));
        if let Some(ref c) = blobbi.visual_traits.secondary_color {
            tags.push(Tag::custom(TagKind::custom(TAG_SECONDARY_COLOR), vec![c.clone()]));
        }
        if !blobbi.visual_traits.pattern.is_empty() {
            tags.push(Tag::custom(TagKind::custom(TAG_PATTERN), vec![blobbi.visual_traits.pattern.clone()]));
        }
        if !blobbi.visual_traits.eye_color.is_empty() {
            tags.push(Tag::custom(TagKind::custom(TAG_EYE_COLOR), vec![blobbi.visual_traits.eye_color.clone()]));
        }
        if !blobbi.visual_traits.special_mark.is_empty() {
            tags.push(Tag::custom(TagKind::custom(TAG_SPECIAL_MARK), vec![blobbi.visual_traits.special_mark.clone()]));
        }
        if !blobbi.visual_traits.size.is_empty() {
            tags.push(Tag::custom(TagKind::custom(TAG_SIZE), vec![blobbi.visual_traits.size.clone()]));
        }
        if let Some(ref t) = blobbi.adult_type {
            tags.push(Tag::custom(TagKind::custom(TAG_ADULT_TYPE), vec![t.clone()]));
        }
        if let Some(v) = blobbi.evolution_time {
            tags.push(Tag::custom(TagKind::custom(TAG_EVOLUTION_TIME), vec![v.to_string()]));
        }

        for trait_name in &blobbi.personality.traits {
            tags.push(Tag::custom(TagKind::custom(TAG_TRAIT), vec![trait_name.clone()]));
        }
        if !blobbi.personality.mood.is_empty() {
            tags.push(Tag::custom(TagKind::custom(TAG_MOOD), vec![blobbi.personality.mood.clone()]));
        }

        if blobbi.stage == BlobbiStage::Egg {
            if let Some(v) = blobbi.incubation_time {
                tags.push(Tag::custom(TagKind::custom(TAG_INCUBATION_TIME), vec![v.to_string()]));
            }
            if let Some(v) = blobbi.incubation_progress {
                tags.push(Tag::custom(TagKind::custom(TAG_INCUBATION_PROGRESS), vec![v.to_string()]));
            }
            if let Some(v) = blobbi.egg_temperature {
                tags.push(Tag::custom(TagKind::custom(TAG_EGG_TEMPERATURE), vec![v.to_string()]));
            }
            if let Some(ref v) = blobbi.egg_status {
                tags.push(Tag::custom(TagKind::custom(TAG_EGG_STATUS), vec![v.clone()]));
            }
            if let Some(v) = blobbi.shell_integrity {
                tags.push(Tag::custom(TagKind::custom(TAG_SHELL_INTEGRITY), vec![v.to_string()]));
            }
        }

        if let Some(v) = blobbi.start_incubation {
            tags.push(Tag::custom(TagKind::custom(TAG_START_INCUBATION), vec![v.to_string()]));
        }
        if let Some(v) = blobbi.start_evolution {
            tags.push(Tag::custom(TagKind::custom(TAG_START_EVOLUTION), vec![v.to_string()]));
        }

        for task in &blobbi.tasks {
            if task.progress > 0 {
                tags.push(Tag::custom(
                    TagKind::custom(format!("{}_progress", task.id)),
                    vec![task.progress.to_string()],
                ));
            }
            if task.completed {
                tags.push(Tag::custom(
                    TagKind::custom(format!("{}_confirmed", task.id)),
                    vec!["true".to_string()],
                ));
            }
        }

        if let Some(ref v) = blobbi.theme {
            tags.push(Tag::custom(TagKind::custom(TAG_THEME), vec![v.clone()]));
        }
        if let Some(ref v) = blobbi.crossover_app {
            tags.push(Tag::custom(TagKind::custom(TAG_CROSSOVER_APP), vec![v.clone()]));
        }
        if let Some(ref v) = blobbi.manifestation {
            tags.push(Tag::custom(TagKind::custom(TAG_MANIFESTATION), vec![v.clone()]));
        }
        if let Some(ref v) = blobbi.blessing {
            tags.push(Tag::custom(TagKind::custom(TAG_BLESSING), vec![v.clone()]));
        }
        if let Some(ref v) = blobbi.visual_effect {
            tags.push(Tag::custom(TagKind::custom(TAG_VISUAL_EFFECT), vec![v.clone()]));
        }

        if let Some(v) = blobbi.last_meal {
            tags.push(Tag::custom(TagKind::custom(TAG_LAST_MEAL), vec![v.to_string()]));
        }
        if let Some(v) = blobbi.last_clean {
            tags.push(Tag::custom(TagKind::custom(TAG_LAST_CLEAN), vec![v.to_string()]));
        }
        if let Some(v) = blobbi.last_warm {
            tags.push(Tag::custom(TagKind::custom(TAG_LAST_WARM), vec![v.to_string()]));
        }
        if let Some(v) = blobbi.last_talk {
            tags.push(Tag::custom(TagKind::custom(TAG_LAST_TALK), vec![v.to_string()]));
        }
        if let Some(v) = blobbi.last_check {
            tags.push(Tag::custom(TagKind::custom(TAG_LAST_CHECK), vec![v.to_string()]));
        }
        if let Some(v) = blobbi.last_sing {
            tags.push(Tag::custom(TagKind::custom(TAG_LAST_SING), vec![v.to_string()]));
        }
        if let Some(v) = blobbi.last_medicine {
            tags.push(Tag::custom(TagKind::custom(TAG_LAST_MEDICINE), vec![v.to_string()]));
        }

        Self { tags }
    }

    pub fn build(self) -> Vec<Tag> {
        sorted_tags(self.tags)
    }
}

fn blobbi_content(blobbi: &BlobbiCompanion) -> String {
    format!(
        "{} is a {} Blobbi.",
        blobbi.name,
        blobbi.stage.as_str()
    )
}

pub async fn publish_blobbi_state(blobbi: &BlobbiCompanion) -> Result<(), String> {
    let tags = StateTags::new(blobbi).build();
    let content = blobbi_content(blobbi);

    let builder = EventBuilder::new(blobbi_state_kind(), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("blobbi".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;

    Ok(())
}

#[allow(dead_code)]
pub async fn publish_blobbi_state_with_source(blobbi: &BlobbiCompanion, source: &str) -> Result<(), String> {
    let mut b = blobbi.clone();
    b.source = Some(source.to_string());
    publish_blobbi_state(&b).await
}

pub fn build_interaction_event(
    blobbi_d: &str,
    action: &str,
    category: &str,
    stat_changes: &[String],
    item_used: Option<&str>,
    experience_gained: u64,
) -> EventBuilder {
    let mut tags = vec![
        Tag::custom(TagKind::custom(TAG_BLOBBI_ID), vec![blobbi_d.to_string()]),
        Tag::custom(TagKind::custom(TAG_ACTION), vec![action.to_string()]),
        Tag::custom(TagKind::custom(TAG_ACTION_CATEGORY), vec![category.to_string()]),
        Tag::custom(TagKind::custom(TAG_B), vec![BLOBBI_ECOSYSTEM_TAG.to_string()]),
        Tag::custom(TagKind::custom(TAG_T), vec![BLOBBI_TOPIC_TAG.to_string()]),
        Tag::custom(TagKind::custom(TAG_CLIENT), vec![CLIENT_TAG.to_string()]),
    ];

    for change in stat_changes {
        tags.push(Tag::custom(TagKind::custom(TAG_STAT_CHANGE), vec![change.clone()]));
    }

    if let Some(item) = item_used {
        tags.push(Tag::custom(TagKind::custom(TAG_ITEM_USED), vec![item.to_string()]));
    }

    if experience_gained > 0 {
        tags.push(Tag::custom(TagKind::custom(TAG_EXPERIENCE_GAINED), vec![experience_gained.to_string()]));
    }

    let content = format!("Blobbi {} interaction", action);
    EventBuilder::new(blobbi_interaction_kind(), content).tags(tags)
}

#[allow(dead_code)]
#[allow(clippy::vec_init_then_push)]
pub async fn publish_profile(profile: &BlobbonautProfile) -> Result<(), String> {
    let mut tags = Vec::new();

    tags.push(Tag::identifier(&profile.d));
    tags.push(Tag::custom(TagKind::custom(TAG_B), vec![BLOBBI_ECOSYSTEM_TAG.to_string()]));
    tags.push(Tag::custom(TagKind::custom(TAG_T), vec![BLOBBI_TOPIC_TAG.to_string()]));
    tags.push(Tag::custom(TagKind::custom(TAG_CLIENT), vec![CLIENT_TAG.to_string()]));

    if !profile.name.is_empty() {
        tags.push(Tag::custom(TagKind::custom(TAG_NAME), vec![profile.name.clone()]));
    }
    tags.push(Tag::custom(TagKind::custom(TAG_COINS), vec![profile.coins.to_string()]));
    tags.push(Tag::custom(TagKind::custom(TAG_PETTING_LEVEL), vec![profile.petting_level.to_string()]));
    tags.push(Tag::custom(TagKind::custom("level"), vec![profile.level.to_string()]));

    if let Some(ref companion) = profile.current_companion {
        tags.push(Tag::custom(TagKind::custom(TAG_CURRENT_COMPANION), vec![companion.clone()]));
    }
    tags.push(Tag::custom(
        TagKind::custom(TAG_ONBOARDING_DONE),
        vec![if profile.onboarding_done { "true" } else { "false" }.to_string()],
    ));

    for pet_id in &profile.has {
        tags.push(Tag::custom(TagKind::custom(TAG_HAS), vec![pet_id.clone()]));
    }
    for item in &profile.storage {
        tags.push(Tag::custom(TagKind::custom(TAG_STORAGE), vec![item.to_string_value()]));
    }
    for achievement in &profile.achievements {
        tags.push(Tag::custom(TagKind::custom(TAG_ACHIEVEMENTS), vec![achievement.clone()]));
    }

    if profile.lifetime_blobbis > 0 {
        tags.push(Tag::custom(TagKind::custom(TAG_LIFETIME_BLOBBIS), vec![profile.lifetime_blobbis.to_string()]));
    }
    if let Some(ref starter) = profile.starter_blobbi {
        tags.push(Tag::custom(TagKind::custom(TAG_STARTER_BLOBBI), vec![starter.clone()]));
    }
    if let Some(ref fav) = profile.favorite_blobbi {
        tags.push(Tag::custom(TagKind::custom(TAG_FAVORITE_BLOBBI), vec![fav.clone()]));
    }
    if let Some(ref style) = profile.style {
        tags.push(Tag::custom(TagKind::custom(TAG_STYLE), vec![style.clone()]));
    }
    if let Some(ref bg) = profile.background {
        tags.push(Tag::custom(TagKind::custom(TAG_BACKGROUND), vec![bg.clone()]));
    }
    if let Some(ref title) = profile.title {
        tags.push(Tag::custom(TagKind::custom(TAG_TITLE), vec![title.clone()]));
    }

    let tags = sorted_tags(tags);

    let builder = EventBuilder::new(blobbonaut_profile_kind(), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("blobbi".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;

    Ok(())
}

pub fn build_breeding_event(
    parent_a: &str,
    parent_b: &str,
    owner_a: &str,
    owner_b: &str,
    success: bool,
    offspring_id: Option<&str>,
    content: String,
) -> EventBuilder {
    let mut tags = vec![
        Tag::custom(TagKind::custom(TAG_PARENT_A), vec![parent_a.to_string()]),
        Tag::custom(TagKind::custom(TAG_PARENT_B), vec![parent_b.to_string()]),
        Tag::custom(TagKind::custom(TAG_OWNER_A), vec![owner_a.to_string()]),
        Tag::custom(TagKind::custom(TAG_OWNER_B), vec![owner_b.to_string()]),
        Tag::custom(TagKind::custom(TAG_BREED_TIME), vec![Timestamp::now().to_human_datetime()]),
        Tag::custom(TagKind::custom(TAG_SUCCESS), vec![if success { "true" } else { "false" }.to_string()]),
        Tag::custom(TagKind::custom(TAG_B), vec![BLOBBI_ECOSYSTEM_TAG.to_string()]),
    ];

    if let Some(offspring) = offspring_id {
        tags.push(Tag::custom(TagKind::custom(TAG_OFFSPRING_ID), vec![offspring.to_string()]));
    }

    EventBuilder::new(blobbi_breeding_kind(), content).tags(tags)
}

pub fn build_record_event(
    blobbi_id: &str,
    record_type: &str,
    generation: u32,
    extra_tags: Vec<(&str, String)>,
    content: String,
) -> EventBuilder {
    let mut tags = vec![
        Tag::custom(TagKind::custom(TAG_BLOBBI_ID), vec![blobbi_id.to_string()]),
        Tag::custom(TagKind::custom(TAG_RECORD_TYPE), vec![record_type.to_string()]),
        Tag::custom(TagKind::custom(TAG_GENERATION), vec![generation.to_string()]),
        Tag::custom(TagKind::custom(TAG_B), vec![BLOBBI_ECOSYSTEM_TAG.to_string()]),
    ];

    for (tag_name, value) in extra_tags {
        tags.push(Tag::custom(TagKind::custom(tag_name), vec![value]));
    }

    EventBuilder::new(blobbi_record_kind(), content).tags(tags)
}
