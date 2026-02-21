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

        let dep_type = match section {
            "[dependencies]" => DepType::Runtime,
            "[dev-dependencies]" => DepType::Dev,
            "[build-dependencies]" => DepType::Build,
            _ => continue,
        };

        // Parse "name = version" or "name = { version = "x" }"
        if let Some((name, rest)) = trimmed.split_once('=') {
            let name = name.trim().to_string();
            let rest = rest.trim();
            let version = if rest.starts_with('"') {
                rest.trim_matches('"').to_string()
            } else if rest.contains("version") {
                // Parse table format
                rest.split("version")
                    .nth(1)
                    .and_then(|s| s.split('"').nth(1))
                    .unwrap_or("*")
                    .to_string()
            } else {
                "*".to_string()
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
    }
    deps
}

/// Parse dependencies from requirements.txt
pub fn parse_requirements_txt(content: &str) -> Vec<Dependency> {
    content
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .map(|line| {
            let line = line.trim();
            if let Some((name, version)) = line.split_once("==") {
                Dependency {
                    name: name.to_string(),
                    version: version.to_string(),
                    dep_type: DepType::Runtime,
                }
            } else if let Some((name, version)) = line.split_once(">=") {
                Dependency {
                    name: name.to_string(),
                    version: format!(">={}", version),
                    dep_type: DepType::Runtime,
                }
            } else {
                Dependency {
                    name: line.to_string(),
                    version: "*".to_string(),
                    dep_type: DepType::Runtime,
                }
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
    let runtime: Vec<_> = deps.iter().filter(|d| d.dep_type == DepType::Runtime).collect();
    let dev: Vec<_> = deps.iter().filter(|d| d.dep_type == DepType::Dev).collect();
    let build: Vec<_> = deps.iter().filter(|d| d.dep_type == DepType::Build).collect();
    let optional: Vec<_> = deps.iter().filter(|d| d.dep_type == DepType::Optional).collect();

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
                if !runtime.is_empty() {
                    span { class: "text-blue-500", "{runtime.len()} runtime" }
                }
                if !dev.is_empty() {
                    span { class: "text-purple-500", "{dev.len()} dev" }
                }
                if !build.is_empty() {
                    span { class: "text-orange-500", "{build.len()} build" }
                }
                if !optional.is_empty() {
                    span { class: "text-gray-500", "{optional.len()} optional" }
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
