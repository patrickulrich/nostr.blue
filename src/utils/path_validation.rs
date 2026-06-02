/// Check if a file path is safe (no traversal, no absolute paths, no NUL bytes).
pub fn is_safe_path(path: &str) -> bool {
    if path.contains('\0') {
        return false;
    }
    use std::path::{Component, Path};
    let p = Path::new(path);
    !p.components()
        .any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
        && !p.to_string_lossy().starts_with('/')
}
