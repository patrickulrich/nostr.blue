use regex::Regex;
/// Base URL for locally-served NIPs documentation
const NIPS_BASE: &str = "/docs/nips";
/// Base URL for Cashu NUTs documentation
const NUTS_BASE: &str = "/docs/nuts";
/// Base URL for Blossom BUDs documentation
const BUDS_BASE: &str = "/docs/blossom/buds";
/// Blossom README path
const BUDS_README: &str = "/docs/blossom/README.md";
/// Base URL for NKBIPs documentation
const NKBIPS_BASE: &str = "/docs/NKBIPs";
/// Market specification path
const MARKET_SPEC_PATH: &str = "/docs/market-spec/spec.md";
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
/// Fetch the README.md from the NIPs documentation
pub async fn fetch_nips_readme() -> Result<String, String> {
    let url = format!("{}/README.md", NIPS_BASE);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch NIPs README: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Failed to fetch NIPs README: HTTP {}", response.status()));
    }
    response.text().await.map_err(|e| format!("Failed to read response: {}", e))
}
/// Fetch the content of a specific NIP by its number
pub async fn fetch_nip_content(number: &str) -> Result<String, String> {
    let url = format!("{}/{}.md", NIPS_BASE, number);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch NIP-{}: {}", number, e))?;
    if !response.status().is_success() {
        return Err(format!("NIP-{} not found (HTTP {})", number, response.status()));
    }
    response.text().await.map_err(|e| format!("Failed to read NIP-{}: {}", number, e))
}
/// Parse the NIP list from the README content
pub fn parse_nips_from_readme(content: &str) -> Vec<OfficialNip> {
    let mut nips = Vec::new();
    let nip_regex = Regex::new(
            r"^\s*-\s*\[NIP-([0-9A-Fa-f]{2,3}):\s*([^\]]+)\]\(([0-9A-Fa-f]{2,3})\.md\)(.*)$",
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
    let link_regex = Regex::new(r"\[([0-9A-Fa-f]{2,3})\]").ok();
    if let Some(regex) = link_regex {
        if let Some(caps) = regex.captures(text) {
            return caps.get(1).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
        }
    }
    let simple_regex = Regex::new(r"([0-9A-Fa-f]{2,3})").ok();
    if let Some(regex) = simple_regex {
        if let Some(caps) = regex.captures(text) {
            return caps.get(1).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
        }
    }
    String::new()
}
/// Generic doc spec entry (for NUTs, BUDs, NKBIPs)
#[derive(Debug, Clone, PartialEq)]
pub struct DocSpec {
    /// Spec number (e.g. "00", "01")
    pub number: String,
    /// Human-readable title
    pub title: String,
    /// Optional category (e.g. "Mandatory", "Optional")
    pub category: Option<String>,
}
/// Fetch the NUTs README
pub async fn fetch_nuts_readme() -> Result<String, String> {
    let url = format!("{}/README.md", NUTS_BASE);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch NUTs README: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Failed to fetch NUTs README: HTTP {}", response.status()));
    }
    response.text().await.map_err(|e| format!("Failed to read response: {}", e))
}
/// Fetch the Blossom README
pub async fn fetch_buds_readme() -> Result<String, String> {
    let response = reqwest::get(BUDS_README)
        .await
        .map_err(|e| format!("Failed to fetch BUDs README: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Failed to fetch BUDs README: HTTP {}", response.status()));
    }
    response.text().await.map_err(|e| format!("Failed to read response: {}", e))
}
/// Fetch a specific NUT by number
pub async fn fetch_nut_content(number: &str) -> Result<String, String> {
    let url = format!("{}/{}.md", NUTS_BASE, number);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch NUT-{}: {}", number, e))?;
    if !response.status().is_success() {
        return Err(format!("NUT-{} not found (HTTP {})", number, response.status()));
    }
    response.text().await.map_err(|e| format!("Failed to read NUT-{}: {}", number, e))
}
/// Fetch a specific BUD by number
pub async fn fetch_bud_content(number: &str) -> Result<String, String> {
    let url = format!("{}/{}.md", BUDS_BASE, number);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch BUD-{}: {}", number, e))?;
    if !response.status().is_success() {
        return Err(format!("BUD-{} not found (HTTP {})", number, response.status()));
    }
    response.text().await.map_err(|e| format!("Failed to read BUD-{}: {}", number, e))
}
/// Fetch a specific NKBIP by number
pub async fn fetch_nkbip_content(number: &str) -> Result<String, String> {
    let url = format!("{}/{}.md", NKBIPS_BASE, number);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch NKBIP-{}: {}", number, e))?;
    if !response.status().is_success() {
        return Err(format!("NKBIP-{} not found (HTTP {})", number, response.status()));
    }
    response.text().await.map_err(|e| format!("Failed to read NKBIP-{}: {}", number, e))
}
/// Fetch the market specification document
pub async fn fetch_market_spec() -> Result<String, String> {
    let response = reqwest::get(MARKET_SPEC_PATH)
        .await
        .map_err(|e| format!("Failed to fetch market spec: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Market spec not found (HTTP {})", response.status()));
    }
    response.text().await.map_err(|e| format!("Failed to read market spec: {}", e))
}
/// Parse NUT entries from the README content.
/// The format has two tables (Mandatory / Optional) with rows like `| [00][00] | Description |`
pub fn parse_nuts_from_readme(content: &str) -> Vec<DocSpec> {
    let mut nuts = Vec::new();
    let mut current_category: Option<String> = None;
    let num_regex = Regex::new(r"^\|\s*\[(\d{2})\]").unwrap();
    for line in content.lines() {
        if line.starts_with("### ") {
            let heading = line.trim_start_matches('#').trim().to_string();
            if heading == "Mandatory" || heading == "Optional" {
                current_category = Some(heading);
            }
        }
        // Stop parsing when we hit the Wallets/Mints section
        if line.starts_with("#### ") {
            break;
        }
        if let Some(caps) = num_regex.captures(line) {
            let number = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            // Extract description from the second column
            let parts: Vec<&str> = line.split('|').collect();
            let description = if parts.len() >= 3 {
                parts[2].trim().to_string()
            } else {
                String::new()
            };
            nuts.push(DocSpec {
                number,
                title: description,
                category: current_category.clone(),
            });
        }
    }
    nuts
}
/// Parse BUD entries from the Blossom README content.
/// The format is bullet lines like `- [BUD-00: Title](./buds/00.md)`
pub fn parse_buds_from_readme(content: &str) -> Vec<DocSpec> {
    let mut buds = Vec::new();
    let bud_regex = Regex::new(r"^\s*-\s*\[BUD-(\d{2}):\s*([^\]]+)\]").unwrap();
    for line in content.lines() {
        if let Some(caps) = bud_regex.captures(line) {
            let number = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let title = caps.get(2).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            buds.push(DocSpec {
                number,
                title,
                category: None,
            });
        }
    }
    buds
}
/// Parse NKBIP entries. The README has no structured list, so we hardcode known entries.
/// TODO: parse README headings to build DocSpec entries dynamically
pub fn parse_nkbips_from_readme(_content: &str) -> Vec<DocSpec> {
    vec![DocSpec {
        number: "01".to_string(),
        title: "Knowledge Base Specification".to_string(),
        category: None,
    }]
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
- [NIP-100: WebRTC Signaling](100.md)
"#;
        let nips = parse_nips_from_readme(content);
        assert_eq!(nips.len(), 4);
        assert_eq!(nips[0].number, "01");
        assert_eq!(nips[0].title, "Basic protocol flow description");
        assert!(!nips[0].deprecated);
        assert!(!nips[0].unrecommended);
        assert_eq!(nips[1].number, "04");
        assert!(nips[1].deprecated);
        assert!(nips[1].unrecommended);
        assert_eq!(nips[2].number, "C7");
        assert_eq!(nips[3].number, "100");
        assert_eq!(nips[3].title, "WebRTC Signaling");
    }
    #[test]
    fn test_parse_nuts_from_readme() {
        let content = r#"
### Mandatory

| NUT #    | Description             |
| -------- | ----------------------- |
| [00][00] | Cryptography and Models |
| [01][01] | Mint public keys        |

### Optional

| #        | Description                       | Wallets | Mints |
| -------- | --------------------------------- | ------- | ----- |
| [07][07] | Token state check                 | impl    | impl  |
| [08][08] | Overpaid Lightning fees           | impl    | impl  |

#### Wallets:
"#;
        let nuts = parse_nuts_from_readme(content);
        assert_eq!(nuts.len(), 4);
        assert_eq!(nuts[0].number, "00");
        assert_eq!(nuts[0].title, "Cryptography and Models");
        assert_eq!(nuts[0].category, Some("Mandatory".to_string()));
        assert_eq!(nuts[2].number, "07");
        assert_eq!(nuts[2].category, Some("Optional".to_string()));
    }
    #[test]
    fn test_parse_buds_from_readme() {
        let content = r#"
## BUDs

- [BUD-00: Blossom Upgrade Documents](./buds/00.md)
- [BUD-01: Server requirements and blob retrieval](./buds/01.md)
- [BUD-02: Blob upload and management](./buds/02.md)
"#;
        let buds = parse_buds_from_readme(content);
        assert_eq!(buds.len(), 3);
        assert_eq!(buds[0].number, "00");
        assert_eq!(buds[0].title, "Blossom Upgrade Documents");
        assert_eq!(buds[1].number, "01");
        assert_eq!(buds[1].title, "Server requirements and blob retrieval");
    }
    #[test]
    fn test_parse_nkbips() {
        let nkbips = parse_nkbips_from_readme("");
        assert_eq!(nkbips.len(), 1);
        assert_eq!(nkbips[0].number, "01");
    }
}
