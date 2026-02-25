//! Dependency Viewer Component
//!
//! Parses and displays dependency information from common package files.
#![allow(dead_code)]
use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub dep_type: DepType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DepType {
    Runtime,
    Dev,
    Build,
    Optional,
}

impl DepType {
    fn label(&self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Dev => "dev",
            Self::Build => "build",
            Self::Optional => "optional",
        }
    }
    fn css_class(&self) -> &'static str {
        match self {
            Self::Runtime => "bg-blue-500/10 text-blue-500",
            Self::Dev => "bg-purple-500/10 text-purple-500",
            Self::Build => "bg-orange-500/10 text-orange-500",
            Self::Optional => "bg-gray-500/10 text-gray-500",
        }
    }
}

/// Parse dependencies from Cargo.toml content
pub fn parse_cargo_toml(content: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();
    let mut section = "";

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed;
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let dep_type = if section.contains("dependencies") {
            if section.contains("dev-dependencies") {
                DepType::Dev
            } else if section.contains("build-dependencies") {
                DepType::Build
            } else {
                DepType::Runtime
            }
        } else {
            continue;
        };

        // Parse "name = version" or "name = { version = "x" }"
        if let Some((name, rest)) = trimmed.split_once('=') {
            let name = name.trim().to_string();
            let rest = rest.trim();
            let (version, dep_type) = if rest.starts_with('"') {
                (rest.trim_matches('"').to_string(), dep_type)
            } else if rest.contains('{') {
                // Parse inline table format in a single pass: { version = "x", optional = true, features = [...] }
                let mut ver = None;
                let mut is_optional = false;
                for token in rest.split(',') {
                    let token = token.trim().trim_matches('{').trim_matches('}').trim();
                    if let Some((k, v)) = token.split_once('=') {
                        let k = k.trim();
                        let v = v.trim();
                        match k {
                            "version" => ver = Some(v.trim_matches('"').to_string()),
                            "optional" => is_optional = v.trim_matches('"') == "true",
                            _ => {}
                        }
                    }
                }
                let version = ver.unwrap_or_else(|| "*".to_string());
                let dep_type = if is_optional { DepType::Optional } else { dep_type };
                (version, dep_type)
            } else {
                ("*".to_string(), dep_type)
            };
            deps.push(Dependency {
                name,
                version,
                dep_type,
            });
        }
    }
    deps
}

/// Parse dependencies from package.json content
pub fn parse_package_json(content: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
        if let Some(obj) = json.get("dependencies").and_then(|v| v.as_object()) {
            for (name, version) in obj {
                deps.push(Dependency {
                    name: name.clone(),
                    version: version.as_str().unwrap_or("*").to_string(),
                    dep_type: DepType::Runtime,
                });
            }
        }
        if let Some(obj) = json.get("devDependencies").and_then(|v| v.as_object()) {
            for (name, version) in obj {
                deps.push(Dependency {
                    name: name.clone(),
                    version: version.as_str().unwrap_or("*").to_string(),
                    dep_type: DepType::Dev,
                });
            }
        }
        if let Some(obj) = json.get("optionalDependencies").and_then(|v| v.as_object()) {
            for (name, version) in obj {
                deps.push(Dependency {
                    name: name.clone(),
                    version: version.as_str().unwrap_or("*").to_string(),
                    dep_type: DepType::Optional,
                });
            }
        }
        if let Some(obj) = json.get("peerDependencies").and_then(|v| v.as_object()) {
            for (name, version) in obj {
                deps.push(Dependency {
                    name: name.clone(),
                    version: version.as_str().unwrap_or("*").to_string(),
                    dep_type: DepType::Runtime,
                });
            }
        }
        if let Some(obj) = json.get("bundleDependencies").and_then(|v| v.as_object()) {
            for (name, version) in obj {
                deps.push(Dependency {
                    name: name.clone(),
                    version: version.as_str().unwrap_or("*").to_string(),
                    dep_type: DepType::Runtime,
                });
            }
        }
    }
    deps
}

/// PEP 508 version operators, ordered longest-first so multi-char operators match before single-char.
const PEP508_OPERATORS: &[&str] = &["===", "~=", "!=", "<=", ">=", "==", "<", ">"];

/// Parse dependencies from requirements.txt
pub fn parse_requirements_txt(content: &str) -> Vec<Dependency> {
    content
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .map(|line| {
            let line = line.trim();
            // Try each PEP 508 operator in order (longest first)
            let mut found = None;
            for op in PEP508_OPERATORS {
                if let Some(pos) = line.find(op) {
                    let name = line[..pos].trim();
                    let version = line[pos..].trim();
                    found = Some((name.to_string(), version.to_string()));
                    break;
                }
            }
            match found {
                Some((name, version)) => Dependency {
                    name,
                    version,
                    dep_type: DepType::Runtime,
                },
                None => Dependency {
                    name: line.to_string(),
                    version: "*".to_string(),
                    dep_type: DepType::Runtime,
                },
            }
        })
        .collect()
}

/// Detect file type and parse
pub fn parse_dependencies(filename: &str, content: &str) -> Vec<Dependency> {
    if filename.ends_with("Cargo.toml") {
        parse_cargo_toml(content)
    } else if filename.ends_with("package.json") {
        parse_package_json(content)
    } else if filename.ends_with("requirements.txt") {
        parse_requirements_txt(content)
    } else {
        Vec::new()
    }
}

#[component]
pub fn DependencyViewer(deps: Vec<Dependency>, filename: String) -> Element {
    let runtime_count = deps.iter().filter(|d| d.dep_type == DepType::Runtime).count();
    let dev_count = deps.iter().filter(|d| d.dep_type == DepType::Dev).count();
    let build_count = deps.iter().filter(|d| d.dep_type == DepType::Build).count();
    let optional_count = deps.iter().filter(|d| d.dep_type == DepType::Optional).count();

    rsx! {
        div { class: "space-y-4",
            // Header
            div { class: "flex items-center justify-between",
                h3 { class: "text-lg font-semibold text-foreground", "Dependencies" }
                span { class: "text-sm text-muted-foreground",
                    "from {filename} · {deps.len()} total"
                }
            }

            // Stats bar
            div { class: "flex gap-4 text-sm",
                if runtime_count > 0 {
                    span { class: "text-blue-500", "{runtime_count} runtime" }
                }
                if dev_count > 0 {
                    span { class: "text-purple-500", "{dev_count} dev" }
                }
                if build_count > 0 {
                    span { class: "text-orange-500", "{build_count} build" }
                }
                if optional_count > 0 {
                    span { class: "text-gray-500", "{optional_count} optional" }
                }
            }

            // Dependency list
            div { class: "bg-card border border-border rounded-lg overflow-hidden",
                // Table header
                div { class: "grid grid-cols-3 gap-4 p-3 bg-muted text-xs font-medium text-muted-foreground uppercase tracking-wider",
                    span { "Package" }
                    span { "Version" }
                    span { "Type" }
                }
                // Rows
                for dep in deps.iter() {
                    div { key: "{dep.name}:{dep.dep_type:?}", class: "grid grid-cols-3 gap-4 p-3 border-t border-border hover:bg-accent/30 transition text-sm",
                        span { class: "font-medium text-foreground truncate", "{dep.name}" }
                        code { class: "text-muted-foreground font-mono text-xs", "{dep.version}" }
                        span {
                            span { class: "px-2 py-0.5 rounded-full text-xs {dep.dep_type.css_class()}",
                                "{dep.dep_type.label()}"
                            }
                        }
                    }
                }
            }
        }
    }
}
