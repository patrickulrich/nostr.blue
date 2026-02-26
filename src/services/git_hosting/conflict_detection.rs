//! Heuristic Merge Conflict Detection
//!
//! Parses a PR's unified diff to extract changed file paths and context,
//! then compares against the current HEAD content to detect potential conflicts.

use crate::services::git_hosting::git_service;
use crate::utils::nip34::Repository;

/// Type of conflict detected
#[derive(Clone, Debug, PartialEq)]
pub enum ConflictType {
    /// Lines the patch expects to exist have changed or been removed
    ContextMismatch,
    /// The file was deleted from the target branch
    FileDeleted,
    /// The patch creates a file that already exists (for new-file patches)
    FileAlreadyExists,
}

/// Information about a detected conflict
#[derive(Clone, Debug)]
pub struct ConflictInfo {
    pub file_path: String,
    pub conflict_type: ConflictType,
    pub details: String,
}

/// A parsed hunk from a unified diff
struct DiffHunk {
    /// Lines expected to exist in the base file (context + removed lines)
    expected_lines: Vec<String>,
}

/// A parsed file entry from a unified diff
struct DiffFile {
    path: String,
    is_new_file: bool,
    is_deleted: bool,
    hunks: Vec<DiffHunk>,
}

/// Parse a unified diff into file entries
fn parse_diff(diff: &str) -> Vec<DiffFile> {
    let mut files = Vec::new();
    let mut current_path = String::new();
    let mut is_new = false;
    let mut is_deleted = false;
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut current_hunk_lines: Vec<String> = Vec::new();
    let mut in_hunk = false;

    for line in diff.lines() {
        if line.starts_with("diff --git") || line.starts_with("--- a/") && !in_hunk {
            // Flush previous file
            if !current_path.is_empty() {
                if in_hunk && !current_hunk_lines.is_empty() {
                    hunks.push(DiffHunk {
                        expected_lines: current_hunk_lines.clone(),
                    });
                    current_hunk_lines.clear();
                }
                files.push(DiffFile {
                    path: current_path.clone(),
                    is_new_file: is_new,
                    is_deleted,
                    hunks: std::mem::take(&mut hunks),
                });
            }
            in_hunk = false;
            is_new = false;
            is_deleted = false;
        }

        if let Some(rest) = line.strip_prefix("+++ b/") {
            current_path = rest.to_string();
        } else if line.starts_with("+++ /dev/null") {
            is_deleted = true;
        } else if line.starts_with("--- /dev/null") {
            is_new = true;
        } else if line.starts_with("@@ ") {
            // Start of a new hunk
            if in_hunk && !current_hunk_lines.is_empty() {
                hunks.push(DiffHunk {
                    expected_lines: current_hunk_lines.clone(),
                });
                current_hunk_lines.clear();
            }
            in_hunk = true;
        } else if in_hunk
            && (line.starts_with(' ') || line.starts_with('-'))
        {
            // Context line or removed line — these are what we expect in the base
            current_hunk_lines.push(line[1..].to_string());
            // Lines starting with '+' are additions — skip for base comparison
        }
    }

    // Flush last file
    if !current_path.is_empty() {
        if in_hunk && !current_hunk_lines.is_empty() {
            hunks.push(DiffHunk {
                expected_lines: current_hunk_lines,
            });
        }
        files.push(DiffFile {
            path: current_path,
            is_new_file: is_new,
            is_deleted,
            hunks,
        });
    }

    files
}

/// Detect potential merge conflicts by comparing the PR's diff against
/// the current state of the repository at the given base ref.
pub async fn detect_conflicts(
    repo: &Repository,
    diff: &str,
    base_ref: &str,
) -> Vec<ConflictInfo> {
    let diff_files = parse_diff(diff);
    let mut conflicts = Vec::new();

    for file in &diff_files {
        if file.path.is_empty() || file.path == "/dev/null" {
            continue;
        }

        // Check new-file conflicts
        if file.is_new_file {
            if git_service().read_file(repo, &file.path, Some(base_ref)).await.is_ok() {
                conflicts.push(ConflictInfo {
                    file_path: file.path.clone(),
                    conflict_type: ConflictType::FileAlreadyExists,
                    details: "Patch creates this file but it already exists on the target branch".to_string(),
                });
            }
            continue;
        }

        // Check if file still exists
        let current_content = match git_service().read_file(repo, &file.path, Some(base_ref)).await {
            Ok(c) => c,
            Err(_) => {
                if !file.is_deleted {
                    conflicts.push(ConflictInfo {
                        file_path: file.path.clone(),
                        conflict_type: ConflictType::FileDeleted,
                        details: "File no longer exists on the target branch".to_string(),
                    });
                }
                continue;
            }
        };

        // Check context lines against current content
        let current_lines: Vec<&str> = current_content.lines().collect();
        for hunk in &file.hunks {
            if hunk.expected_lines.is_empty() {
                continue;
            }
            // Check if the expected lines exist somewhere in the current file
            let first_expected = hunk.expected_lines.first().map(|s| s.trim()).unwrap_or("");
            if first_expected.is_empty() {
                continue;
            }
            let found = current_lines.iter().any(|l| l.trim() == first_expected);
            if !found {
                conflicts.push(ConflictInfo {
                    file_path: file.path.clone(),
                    conflict_type: ConflictType::ContextMismatch,
                    details: format!(
                        "Expected context line not found: \"{}\"",
                        if first_expected.len() > 60 {
                            format!("{}...", &first_expected[..60])
                        } else {
                            first_expected.to_string()
                        }
                    ),
                });
                break; // One conflict per file is enough
            }
        }
    }

    conflicts
}
