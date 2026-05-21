use crate::components::blobbi::core::tag_schema::{
    deprecated_tag_names, get_default_value, is_task_process_state,
    required_tag_names, tags_for_stage, transition_cleanup_tag_names, valid_states_for_stage,
    NEVER_INVENT_TAGS,
};
use crate::components::blobbi::core::types::{BlobbiStage, BlobbiState};
use crate::utils::nip_bb::constants::*;

use std::collections::HashMap;

pub struct TagRepairResult {
    pub tags: HashMap<String, String>,
    pub final_stage: BlobbiStage,
    pub removed_deprecated: Vec<String>,
    pub added_defaults: Vec<String>,
    pub state_corrected: bool,
}

pub fn validate_and_repair_tags(
    tags: &HashMap<String, String>,
    stage: BlobbiStage,
) -> HashMap<String, String> {
    repair_tags_full(tags, stage, false).tags
}

pub fn repair_tags_full(
    tags: &HashMap<String, String>,
    stage: BlobbiStage,
    cleanup_tasks: bool,
) -> TagRepairResult {
    let mut repaired = tags.clone();
    let mut removed_deprecated = Vec::new();
    let mut added_defaults = Vec::new();

    // Step 1: Filter deprecated tags
    let deprecated = deprecated_tag_names();
    for dep_tag in &deprecated {
        if repaired.remove(*dep_tag).is_some() {
            removed_deprecated.push(dep_tag.to_string());
        }
    }

    // Step 2: Optionally filter task/transition tags
    if cleanup_tasks {
        for cleanup_tag in transition_cleanup_tag_names() {
            repaired.remove(cleanup_tag);
        }
        // Remove per-task progress/confirmed tags
        let task_prefixes: Vec<String> = repaired
            .keys()
            .filter(|k| k.ends_with("_progress") || k.ends_with("_confirmed"))
            .cloned()
            .collect();
        for key in task_prefixes {
            repaired.remove(&key);
        }
    }

    // Step 3: Detect final stage from tags
    let final_stage = repaired
        .get(TAG_STAGE)
        .map(|s| BlobbiStage::from_str(s))
        .unwrap_or(stage);

    // Step 4: Stage-aware tag filtering - remove tags not valid for current stage
    let valid_for_stage: Vec<&str> = tags_for_stage(final_stage);
    let keys_to_remove: Vec<String> = repaired
        .keys()
        .filter(|k| {
            // Keep non-schema tags (extension tags)
            let is_schema_tag = valid_for_stage.contains(&k.as_str())
                || deprecated.contains(&k.as_str());
            if !is_schema_tag {
                return false;
            }
            // Remove schema tags that aren't valid for this stage
            !valid_for_stage.contains(&k.as_str())
        })
        .cloned()
        .collect();
    for key in keys_to_remove {
        repaired.remove(&key);
    }

    // Step 5: Semantic state cleanup
    let mut state_corrected = false;
    if let Some(state_val) = repaired.get(TAG_STATE).cloned() {
        let valid = valid_states_for_stage(final_stage);
        if !valid.contains(&state_val.as_str()) {
            // Invalid state for stage; reset to active
            if is_task_process_state(&state_val) {
                // Task process states should not persist after stage transitions
                repaired.insert(TAG_STATE.to_string(), STATE_ACTIVE.to_string());
                repaired.remove(TAG_STATE_STARTED_AT);
                state_corrected = true;
            } else {
                repaired.insert(TAG_STATE.to_string(), STATE_ACTIVE.to_string());
                state_corrected = true;
            }
        }

        // Fix sleeping state: ensure is_sleeping tag matches
        let is_sleeping_state = state_val == STATE_SLEEPING;
        if is_sleeping_state && repaired.get(TAG_IS_SLEEPING).map(|v| v.as_str()) != Some("true") {
            repaired.insert(TAG_IS_SLEEPING.to_string(), "true".to_string());
        }
    }

    // Fix blobbi_state consistency: if state is Sleeping but is_sleeping is false
    if repaired.get(TAG_STATE).map(|v| v.as_str()) == Some(STATE_SLEEPING)
        && repaired.get(TAG_IS_SLEEPING).map(|v| v.as_str()) != Some("true")
    {
        repaired.insert(TAG_IS_SLEEPING.to_string(), "true".to_string());
    }

    // Step 6: Required tag recovery with NEVER_INVENT check
    let required = required_tag_names(final_stage);
    for req_tag in &required {
        if !repaired.contains_key(*req_tag) {
            // Never auto-generate identity/personality tags
            if NEVER_INVENT_TAGS.contains(req_tag) {
                continue;
            }
            if let Some(default) = get_default_value(req_tag) {
                repaired.insert(req_tag.to_string(), default.to_string());
                added_defaults.push(req_tag.to_string());
            }
        }
    }

    // Step 7: Persistent tag recovery from previous event (if available)
    // This step is handled by the caller (ensure_canonical_before_action)
    // since it needs access to the original event

    TagRepairResult {
        tags: repaired,
        final_stage,
        removed_deprecated,
        added_defaults,
        state_corrected,
    }
}

pub fn is_canonical_blobbi_d(d: &str) -> bool {
    if !d.starts_with("blobbi-") {
        return false;
    }
    let parts: Vec<&str> = d.splitn(3, '-').collect();
    if parts.len() != 3 {
        return false;
    }
    let hex1 = parts[1];
    let hex2 = parts[2];
    hex1.len() == 12 && hex2.len() == 10 && hex1.chars().all(|c| c.is_ascii_hexdigit()) && hex2.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn is_legacy_blobbi_d(d: &str) -> bool {
    d.starts_with("blobbi-") && !is_canonical_blobbi_d(d)
}

pub fn is_canonical_blobbonaut_d(d: &str) -> bool {
    if !d.starts_with("blobbonaut-") {
        return false;
    }
    let hex_part = &d[11..];
    hex_part.len() == 12 && hex_part.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn is_legacy_blobbonaut_d(d: &str) -> bool {
    if !d.starts_with("blobbonaut") && !d.starts_with("Blobbonaut") {
        return false;
    }
    !is_canonical_blobbonaut_d(d)
}

pub fn derive_name_from_legacy_d(d: &str) -> Option<String> {
    if !is_legacy_blobbi_d(d) {
        return None;
    }
    let name_part = d.strip_prefix("blobbi-")?;
    if name_part.starts_with(|c: char| c.is_ascii_hexdigit()) && name_part.len() > 20 {
        return None;
    }
    let name = name_part
        .replace(['-', '_'], " ");
    let mut chars = name.chars();
    let first = chars.next()?.to_uppercase().collect::<String>();
    Some(format!("{}{}", first, chars.as_str()))
}

pub fn needs_blobbi_migration(blobbi: &crate::components::blobbi::core::types::BlobbiCompanion) -> bool {
    if is_legacy_blobbi_d(&blobbi.d) {
        return true;
    }
    if blobbi.seed.is_none() && blobbi.stage != BlobbiStage::Egg {
        return true;
    }
    if blobbi.name.is_empty() {
        return true;
    }
    let state_val = blobbi.state.as_str();
    let valid = valid_states_for_stage(blobbi.stage);
    if !valid.contains(&state_val) {
        return true;
    }
    false
}

pub fn needs_profile_migration(profile: &crate::components::blobbi::core::types::BlobbonautProfile) -> bool {
    if is_legacy_blobbonaut_d(&profile.d) {
        return true;
    }
    // Check for old onboarding tag
    if profile.onboarding_done {
        // fine, modern tag
    }
    false
}

pub fn migrate_blobbi_tags(
    blobbi: &mut crate::components::blobbi::core::types::BlobbiCompanion,
    pubkey: &str,
) -> bool {
    let mut changed = false;

    // Generate canonical d-tag if legacy
    if is_legacy_blobbi_d(&blobbi.d) {
        let pet_id = crate::utils::nip_bb::constants::generate_blobbi_pet_id();
        blobbi.d = crate::utils::nip_bb::constants::blobbi_d_tag(pubkey, &pet_id);
        changed = true;
    }

    // Derive name from legacy d-tag if missing
    if blobbi.name.is_empty() {
        if let Some(name) = derive_name_from_legacy_d(&blobbi.d) {
            blobbi.name = name;
            changed = true;
        }
    }

    // Derive seed if missing (for non-egg stages)
    if blobbi.seed.is_none() && blobbi.stage != BlobbiStage::Egg {
        let created_at = blobbi.last_interaction.unwrap_or(
            crate::platform::timestamp::now_secs()
        );
        blobbi.seed = Some(crate::components::blobbi::core::seed::derive_seed(
            pubkey,
            &blobbi.d,
            created_at,
        ));
        changed = true;
    }

    // Derive visual traits from seed if incomplete
    if let Some(ref seed) = blobbi.seed {
        let seed_traits = crate::components::blobbi::core::seed::derive_visual_traits_from_seed(seed);
        if blobbi.visual_traits.base_color.is_empty()
            || blobbi.visual_traits.base_color == DEFAULT_BASE_COLORS[0]
        {
            blobbi.visual_traits.base_color = seed_traits.base_color;
            changed = true;
        }
        if blobbi.visual_traits.secondary_color.is_none() {
            blobbi.visual_traits.secondary_color = seed_traits.secondary_color;
            changed = true;
        }
        if blobbi.visual_traits.eye_color.is_empty() {
            blobbi.visual_traits.eye_color = seed_traits.eye_color;
            changed = true;
        }
    }

    // Run full tag repair
    let mut tag_map: HashMap<String, String> = HashMap::new();
    tag_map.insert(TAG_STAGE.to_string(), blobbi.stage.as_str().to_string());
    tag_map.insert(TAG_STATE.to_string(), blobbi.state.as_str().to_string());
    if let Some(ref seed) = blobbi.seed {
        tag_map.insert(TAG_SEED.to_string(), seed.clone());
    }

    let result = repair_tags_full(&tag_map, blobbi.stage, false);
    if result.state_corrected {
        blobbi.state = BlobbiState::from_str(
            result.tags.get(TAG_STATE).map(|s| s.as_str()).unwrap_or(STATE_ACTIVE),
        );
        changed = true;
    }

    changed
}

pub fn ensure_canonical_before_action(blobbi: &mut crate::components::blobbi::core::types::BlobbiCompanion) -> bool {
    if !needs_blobbi_migration(blobbi) {
        return false;
    }

    let pubkey = crate::stores::auth_store::get_pubkey()
        .unwrap_or_default();

    let changed = migrate_blobbi_tags(blobbi, &pubkey);

    if changed {
        let blobbi_clone = blobbi.clone();
        dioxus::prelude::spawn(async move {
            if let Err(e) = crate::components::blobbi::core::builders::publish_blobbi_state(&blobbi_clone).await {
                log::error!("Failed to publish migrated blobbi: {}", e);
            }
        });
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_deprecated_tags() {
        let deprecated = deprecated_tag_names();
        if deprecated.is_empty() {
            return;
        }
        let mut tags = HashMap::new();
        tags.insert(deprecated[0].to_string(), "value".to_string());
        tags.insert(TAG_STAGE.to_string(), "egg".to_string());

        let repaired = validate_and_repair_tags(&tags, BlobbiStage::Egg);
        assert!(!repaired.contains_key(deprecated[0]));
        assert!(repaired.contains_key(TAG_STAGE));
    }

    #[test]
    fn adds_missing_required_defaults() {
        let tags = HashMap::new();
        let repaired = validate_and_repair_tags(&tags, BlobbiStage::Egg);

        let required = required_tag_names(BlobbiStage::Egg);
        for req in &required {
            if NEVER_INVENT_TAGS.contains(req) {
                continue;
            }
            if let Some(default) = get_default_value(req) {
                assert_eq!(repaired.get(*req).map(|s| s.as_str()), Some(default));
            }
        }
    }

    #[test]
    fn preserves_existing_tags() {
        let mut tags = HashMap::new();
        tags.insert(TAG_STAGE.to_string(), "baby".to_string());
        tags.insert(TAG_STATE.to_string(), "active".to_string());

        let repaired = validate_and_repair_tags(&tags, BlobbiStage::Baby);
        assert_eq!(repaired.get(TAG_STAGE).map(|s| s.as_str()), Some("baby"));
        assert_eq!(repaired.get(TAG_STATE).map(|s| s.as_str()), Some("active"));
    }

    #[test]
    fn idempotent_repair() {
        let mut tags = HashMap::new();
        tags.insert(TAG_STAGE.to_string(), "egg".to_string());

        let first = validate_and_repair_tags(&tags, BlobbiStage::Egg);
        let second = validate_and_repair_tags(&first, BlobbiStage::Egg);
        assert_eq!(first, second);
    }

    #[test]
    fn canonical_d_detection() {
        assert!(is_canonical_blobbi_d("blobbi-0123456789ab-0123456789"));
        assert!(!is_canonical_blobbi_d("blobbi-fluffy"));
        assert!(!is_canonical_blobbi_d("blobbi-"));
    }

    #[test]
    fn legacy_d_detection() {
        assert!(is_legacy_blobbi_d("blobbi-fluffy"));
        assert!(is_legacy_blobbi_d("blobbi-mr-cool"));
        assert!(!is_legacy_blobbi_d("blobbi-0123456789ab-0123456789"));
    }

    #[test]
    fn derive_name_from_legacy() {
        assert_eq!(derive_name_from_legacy_d("blobbi-fluffy"), Some("Fluffy".to_string()));
        assert_eq!(derive_name_from_legacy_d("blobbi-mr-cool"), Some("Mr cool".to_string()));
        assert_eq!(derive_name_from_legacy_d("blobbi-0123456789ab-0123456789"), None);
    }

    #[test]
    fn state_cleanup_resets_invalid() {
        let mut tags = HashMap::new();
        tags.insert(TAG_STAGE.to_string(), "adult".to_string());
        tags.insert(TAG_STATE.to_string(), "incubating".to_string());

        let result = repair_tags_full(&tags, BlobbiStage::Adult, false);
        assert!(result.state_corrected);
        assert_eq!(result.tags.get(TAG_STATE).map(|s| s.as_str()), Some("active"));
    }

    #[test]
    fn task_cleanup_removes_progress_tags() {
        let mut tags = HashMap::new();
        tags.insert(TAG_STAGE.to_string(), "baby".to_string());
        tags.insert(TAG_STATE.to_string(), "active".to_string());
        tags.insert("interact_6_progress".to_string(), "3".to_string());
        tags.insert("first_post_confirmed".to_string(), "true".to_string());

        let result = repair_tags_full(&tags, BlobbiStage::Baby, true);
        assert!(!result.tags.contains_key("interact_6_progress"));
        assert!(!result.tags.contains_key("first_post_confirmed"));
    }

    #[test]
    fn never_invent_tags_not_generated() {
        let mut tags = HashMap::new();
        tags.insert(TAG_STAGE.to_string(), "baby".to_string());
        // name is in NEVER_INVENT_TAGS and required but has no default
        // seed is in NEVER_INVENT_TAGS
        let repaired = validate_and_repair_tags(&tags, BlobbiStage::Baby);
        // Should NOT auto-generate name or seed
        assert!(!repaired.contains_key("name"));
    }
}
