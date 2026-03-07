//! Native git operations using git2 (libgit2).
//!
//! Provides filesystem-based git operations for desktop and mobile platforms.
//! All functions are synchronous and should be called via `tokio::task::spawn_blocking`.

use crate::services::git_types::{CommitEntry, FileEntry};
use git2::Repository;
use std::path::Path;

/// List files in a directory within a git repository.
pub fn list_files(repo_path: &str, path: &str, git_ref: &str) -> Result<Vec<FileEntry>, String> {
    let repo = Repository::open(repo_path).map_err(|e| e.to_string())?;
    let obj = repo.revparse_single(git_ref).map_err(|e| e.to_string())?;
    let commit = obj.peel_to_commit().map_err(|e| e.to_string())?;
    let tree = commit.tree().map_err(|e| e.to_string())?;

    let subtree = if path.is_empty() || path == "." || path == "/" {
        tree
    } else {
        let entry = tree.get_path(Path::new(path)).map_err(|e| e.to_string())?;
        let obj = entry.to_object(&repo).map_err(|e| e.to_string())?;
        obj.peel_to_tree().map_err(|e| e.to_string())?
    };

    let prefix = if path.is_empty() || path == "." || path == "/" {
        String::new()
    } else {
        format!("{}/", path.trim_end_matches('/'))
    };

    let mut entries = Vec::new();
    for entry in subtree.iter() {
        let name = entry.name().unwrap_or("").to_string();
        let entry_type = match entry.kind() {
            Some(git2::ObjectType::Tree) => "tree",
            _ => "blob",
        };
        entries.push(FileEntry {
            path: format!("{}{}", prefix, name),
            name,
            entry_type: entry_type.to_string(),
        });
    }

    // Sort: directories first, then alphabetically
    entries.sort_by(|a, b| {
        let a_is_dir = a.is_directory();
        let b_is_dir = b.is_directory();
        b_is_dir.cmp(&a_is_dir).then(a.name.cmp(&b.name))
    });

    Ok(entries)
}

/// Read a file's content from the repository.
pub fn read_file(repo_path: &str, filepath: &str, git_ref: &str) -> Result<String, String> {
    let repo = Repository::open(repo_path).map_err(|e| e.to_string())?;
    let obj = repo.revparse_single(git_ref).map_err(|e| e.to_string())?;
    let commit = obj.peel_to_commit().map_err(|e| e.to_string())?;
    let tree = commit.tree().map_err(|e| e.to_string())?;
    let entry = tree
        .get_path(Path::new(filepath))
        .map_err(|e| e.to_string())?;
    let blob = entry
        .to_object(&repo)
        .map_err(|e| e.to_string())?
        .peel_to_blob()
        .map_err(|e| e.to_string())?;
    String::from_utf8(blob.content().to_vec()).map_err(|e| e.to_string())
}

/// Get the list of branches in a repository.
pub fn get_branches(repo_path: &str) -> Result<Vec<String>, String> {
    let repo = Repository::open(repo_path).map_err(|e| e.to_string())?;
    let branches = repo
        .branches(Some(git2::BranchType::Local))
        .map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    for branch in branches {
        let (branch, _) = branch.map_err(|e| e.to_string())?;
        if let Some(name) = branch.name().map_err(|e| e.to_string())? {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

/// Get commit log for a given ref.
pub fn get_log(
    repo_path: &str,
    git_ref: &str,
    count: u32,
) -> Result<Vec<CommitEntry>, String> {
    let repo = Repository::open(repo_path).map_err(|e| e.to_string())?;
    let obj = repo.revparse_single(git_ref).map_err(|e| e.to_string())?;
    let commit = obj.peel_to_commit().map_err(|e| e.to_string())?;
    let mut revwalk = repo.revwalk().map_err(|e| e.to_string())?;
    revwalk.push(commit.id()).map_err(|e| e.to_string())?;
    revwalk
        .set_sorting(git2::Sort::TIME)
        .map_err(|e| e.to_string())?;

    let mut commits = Vec::new();
    for (i, oid) in revwalk.enumerate() {
        if i >= count as usize {
            break;
        }
        let oid = oid.map_err(|e| e.to_string())?;
        let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
        let author = commit.author();
        commits.push(CommitEntry {
            oid: oid.to_string(),
            message: commit.message().unwrap_or("").to_string(),
            author: author.name().unwrap_or("").to_string(),
            email: author.email().unwrap_or("").to_string(),
            timestamp: commit.time().seconds().max(0) as u64,
            parents: commit.parent_ids().map(|id| id.to_string()).collect(),
        });
    }
    Ok(commits)
}

/// List all file paths recursively (for fuzzy finder).
pub fn list_all_paths(repo_path: &str, git_ref: &str) -> Result<Vec<String>, String> {
    let repo = Repository::open(repo_path).map_err(|e| e.to_string())?;
    let obj = repo.revparse_single(git_ref).map_err(|e| e.to_string())?;
    let commit = obj.peel_to_commit().map_err(|e| e.to_string())?;
    let tree = commit.tree().map_err(|e| e.to_string())?;

    let mut paths = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            let name = entry.name().unwrap_or("");
            paths.push(format!("{}{}", root, name));
        }
        git2::TreeWalkResult::Ok
    })
    .map_err(|e| e.to_string())?;
    Ok(paths)
}

/// Generate a unified diff between two refs.
pub fn diff_refs(repo_path: &str, base: &str, head: &str) -> Result<String, String> {
    let repo = Repository::open(repo_path).map_err(|e| e.to_string())?;

    let base_obj = repo.revparse_single(base).map_err(|e| e.to_string())?;
    let base_tree = base_obj
        .peel_to_commit()
        .map_err(|e| e.to_string())?
        .tree()
        .map_err(|e| e.to_string())?;

    let head_obj = repo.revparse_single(head).map_err(|e| e.to_string())?;
    let head_tree = head_obj
        .peel_to_commit()
        .map_err(|e| e.to_string())?
        .tree()
        .map_err(|e| e.to_string())?;

    let diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)
        .map_err(|e| e.to_string())?;

    let mut output = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let prefix = match line.origin() {
            '+' => "+",
            '-' => "-",
            ' ' => " ",
            _ => "",
        };
        output.push_str(prefix);
        if let Ok(content) = std::str::from_utf8(line.content()) {
            output.push_str(content);
        }
        true
    })
    .map_err(|e| e.to_string())?;

    Ok(output)
}
