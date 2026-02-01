use gloo_net::http::Request;
use regex::Regex;
/// GitHub raw content base URL for NIPs repository
const GITHUB_NIPS_BASE: &str = "https://raw.githubusercontent.com/nostr-protocol/nips/refs/heads/master";
/// Represents an official NIP from the nostr-protocol repository
#[derive(Debug, Clone, PartialEq)]
pub struct OfficialNip {
    /// NIP number (e.g., "01", "19", "C7")
    pub number: String,
    /// Human-readable title
    pub title: String,
    /// Whether this NIP is deprecated
    pub deprecated: bool,
    /// Whether this NIP is marked as unrecommended
    pub unrecommended: bool,
}
/// Represents an event kind defined in the NIPs
#[derive(Debug, Clone, PartialEq)]
pub struct EventKindInfo {
    /// Kind number or range (e.g., "1", "5000-5999")
    pub kind: String,
    /// Description of what this kind is for
    pub description: String,
    /// Which NIP defines this kind
    pub nip: String,
}
/// Fetch the README.md from the NIPs repository
pub async fn fetch_nips_readme() -> Result<String, String> {
    let url = format!("{}/README.md", GITHUB_NIPS_BASE);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch NIPs README: {}", e))?;
    if !response.ok() {
        return Err(format!("Failed to fetch NIPs README: HTTP {}", response.status()));
    }
    response.text().await.map_err(|e| format!("Failed to read response: {}", e))
}
/// Fetch the content of a specific NIP by its number
pub async fn fetch_nip_content(number: &str) -> Result<String, String> {
    let url = format!("{}/{}.md", GITHUB_NIPS_BASE, number);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch NIP-{}: {}", number, e))?;
    if !response.ok() {
        return Err(format!("NIP-{} not found (HTTP {})", number, response.status()));
    }
    response.text().await.map_err(|e| format!("Failed to read NIP-{}: {}", number, e))
}
/// Parse the NIP list from the README content
pub fn parse_nips_from_readme(content: &str) -> Vec<OfficialNip> {
    let mut nips = Vec::new();
    let nip_regex = Regex::new(
            r"^\s*-\s*\[NIP-([0-9A-Fa-f]{2}):\s*([^\]]+)\]\(([0-9A-Fa-f]{2})\.md\)(.*)$",
        )
        .unwrap();
    for line in content.lines() {
        if let Some(caps) = nip_regex.captures(line) {
            let number = caps
                .get(1)
                .map(|m| m.as_str().to_uppercase())
                .unwrap_or_default();
            let title = caps
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            let suffix = caps.get(4).map(|m| m.as_str()).unwrap_or("");
            let deprecated = suffix.to_lowercase().contains("deprecated");
            let unrecommended = suffix.to_lowercase().contains("unrecommended");
            nips.push(OfficialNip {
                number,
                title,
                deprecated,
                unrecommended,
            });
        }
    }
    nips
}
/// Parse the event kinds table from the README content
pub fn parse_event_kinds_from_readme(content: &str) -> Vec<EventKindInfo> {
    let mut kinds = Vec::new();
    let mut in_event_kinds_section = false;
    let mut found_header_separator = false;
    for line in content.lines() {
        if line.contains("## Event Kinds") {
            in_event_kinds_section = true;
            continue;
        }
        if in_event_kinds_section && line.starts_with("## ")
            && !line.contains("Event Kinds")
        {
            break;
        }
        if !in_event_kinds_section {
            continue;
        }
        if line.contains("| kind") && line.contains("| description") {
            continue;
        }
        if line.contains("|---") || line.contains("| ---") {
            found_header_separator = true;
            continue;
        }
        if !found_header_separator {
            continue;
        }
        if line.starts_with("|") && line.contains("|") {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 4 {
                let kind = parts[1].trim().trim_matches('`').to_string();
                let description = parts[2].trim().to_string();
                let nip_part = parts[3].trim();
                let nip = extract_nip_number(nip_part);
                if !kind.is_empty() && !description.is_empty() {
                    kinds
                        .push(EventKindInfo {
                            kind,
                            description,
                            nip,
                        });
                }
            }
        }
    }
    kinds
}
/// Extract NIP number from a reference like "[01](01.md)" or "01"
fn extract_nip_number(text: &str) -> String {
    let link_regex = Regex::new(r"\[([0-9A-Fa-f]{2})\]").ok();
    if let Some(regex) = link_regex {
        if let Some(caps) = regex.captures(text) {
            return caps.get(1).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
        }
    }
    let simple_regex = Regex::new(r"([0-9A-Fa-f]{2})").ok();
    if let Some(regex) = simple_regex {
        if let Some(caps) = regex.captures(text) {
            return caps.get(1).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
        }
    }
    String::new()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_nip_line() {
        let content = r#"
## List

- [NIP-01: Basic protocol flow description](01.md)
- [NIP-04: Encrypted Direct Message](04.md) --- **unrecommended**: deprecated in favor of [NIP-17](17.md)
- [NIP-C7: Chats](C7.md)
"#;
        let nips = parse_nips_from_readme(content);
        assert_eq!(nips.len(), 3);
        assert_eq!(nips[0].number, "01");
        assert_eq!(nips[0].title, "Basic protocol flow description");
        assert!(!nips[0].deprecated);
        assert!(!nips[0].unrecommended);
        assert_eq!(nips[1].number, "04");
        assert!(nips[1].deprecated);
        assert!(nips[1].unrecommended);
        assert_eq!(nips[2].number, "C7");
    }
}
