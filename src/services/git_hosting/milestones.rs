//! Milestones Service
//!
//! Manages repository milestones stored as custom tags on repository events.
//! Milestones are stored as `milestone` tags on the Kind 30617 repo announcement:
//! `["milestone", "<id>", "<name>", "<description>", "<due_date_unix>"]`
#![allow(dead_code)]

/// A repository milestone
#[derive(Debug, Clone, PartialEq)]
pub struct Milestone {
    /// Unique milestone ID (short slug)
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Optional description
    pub description: String,
    /// Optional due date (Unix timestamp)
    pub due_date: Option<u64>,
}

/// Parse milestones from repository event tags
pub fn parse_milestones(tags: &[Vec<String>]) -> Vec<Milestone> {
    tags.iter()
        .filter(|t| t.first().map(|s| s.as_str()) == Some("milestone") && t.len() >= 3)
        .map(|t| {
            let (description, due_date) = if t.len() == 4 {
                // Ambiguous: t[3] could be a description or a due_date
                if let Ok(ts) = t[3].parse::<u64>() {
                    (String::new(), Some(ts))
                } else {
                    (t[3].clone(), None)
                }
            } else {
                (t.get(3).cloned().unwrap_or_default(), t.get(4).and_then(|s| s.parse().ok()))
            };
            Milestone {
                id: t[1].clone(),
                name: t[2].clone(),
                description,
                due_date,
            }
        })
        .collect()
}

/// Serialize milestones to tag arrays for publishing
pub fn milestones_to_tags(milestones: &[Milestone]) -> Vec<Vec<String>> {
    milestones
        .iter()
        .map(|m| {
            let mut tag = vec!["milestone".to_string(), m.id.clone(), m.name.clone()];
            if !m.description.is_empty() || m.due_date.is_some() {
                tag.push(m.description.clone());
            }
            if let Some(due) = m.due_date {
                tag.push(due.to_string());
            }
            tag
        })
        .collect()
}

/// Generate a simple slug ID from a milestone name
pub fn generate_milestone_id(name: &str) -> String {
    let raw: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let mut result = String::with_capacity(raw.len());
    let mut prev_dash = false;
    for c in raw.chars() {
        if c == '-' {
            if !prev_dash {
                result.push(c);
            }
            prev_dash = true;
        } else {
            result.push(c);
            prev_dash = false;
        }
    }
    let slug = result.trim_matches('-').to_string();
    if slug.is_empty() {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        return format!("milestone-{:016x}", hasher.finish());
    }
    slug
}
