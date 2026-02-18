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
        .map(|t| Milestone {
            id: t[1].clone(),
            name: t[2].clone(),
            description: t.get(3).cloned().unwrap_or_default(),
            due_date: t.get(4).and_then(|s| s.parse().ok()),
        })
        .collect()
}

/// Serialize milestones to tag arrays for publishing
pub fn milestones_to_tags(milestones: &[Milestone]) -> Vec<Vec<String>> {
    milestones
        .iter()
        .map(|m| {
            let mut tag = vec!["milestone".to_string(), m.id.clone(), m.name.clone()];
            tag.push(m.description.clone());
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
        // Deterministic fallback using byte sum of the original name
        let hash: u16 = name.bytes().fold(0u16, |acc, b| acc.wrapping_add(b as u16));
        return format!("milestone-{:04x}", hash);
    }
    slug
}
