/// Check if a file path is safe (no traversal, no absolute paths).
pub fn is_safe_path(path: &str) -> bool {
    use std::path::{Component, Path};
    let p = Path::new(path);
    for component in p.components() {
        match component {
            Component::ParentDir => return false,
            Component::RootDir => return false,
            Component::Prefix(_) => return false,
            _ => {}
        }
    }
    !p.to_string_lossy().starts_with('/')
}
