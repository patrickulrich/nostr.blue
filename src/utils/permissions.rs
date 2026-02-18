//! Repository Permissions
//!
//! Role-based permission checks for NIP-34 Git repository operations.
//! Follows gittr's permission model: Owner > Maintainer > Contributor.
use crate::utils::nip34::Repository;

/// Check if user is the repository owner
pub fn is_owner(user_pk: &str, repo: &Repository) -> bool {
    user_pk == repo.pubkey
}

/// Check if user is a maintainer (listed in maintainers tag)
pub fn is_maintainer(user_pk: &str, repo: &Repository) -> bool {
    repo.maintainers.contains(&user_pk.to_string())
}

/// Check if user can merge PRs (owner or maintainer)
#[allow(dead_code)]
pub fn can_merge(user_pk: &str, repo: &Repository) -> bool {
    is_owner(user_pk, repo) || is_maintainer(user_pk, repo)
}

/// Check if user can manage repository settings (owner only)
#[allow(dead_code)]
pub fn can_manage_settings(user_pk: &str, repo: &Repository) -> bool {
    is_owner(user_pk, repo)
}

/// Check if user can change issue/PR status (owner, maintainer, or event author)
pub fn can_change_status(user_pk: &str, repo: &Repository, event_author_pk: &str) -> bool {
    user_pk == event_author_pk || is_owner(user_pk, repo) || is_maintainer(user_pk, repo)
}

/// Check if user can edit assignees (owner or maintainer)
#[allow(dead_code)]
pub fn can_edit_assignees(user_pk: &str, repo: &Repository) -> bool {
    is_owner(user_pk, repo) || is_maintainer(user_pk, repo)
}

/// Get a display role label for a user in a repository context
pub fn user_role_label(user_pk: &str, repo: &Repository) -> Option<&'static str> {
    if is_owner(user_pk, repo) {
        Some("Owner")
    } else if is_maintainer(user_pk, repo) {
        Some("Maintainer")
    } else {
        None
    }
}
