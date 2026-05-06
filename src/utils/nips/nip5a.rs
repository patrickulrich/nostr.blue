//! NIP-5A: Static Websites (nsites)
//!
//! Types and utilities for publishing static websites from Blossom assets.
//! Supports both root sites (kind 15128) and named sites (kind 35128).
#![allow(dead_code)]
use nostr_sdk::prelude::*;
use num_bigint::BigUint;
use std::borrow::Cow;

pub const KIND_NSITE_ROOT: u16 = 15128;
pub const KIND_NSITE_NAMED: u16 = 35128;
pub const MAX_DTAG_LEN: usize = 13;
pub const MIN_DTAG_LEN: usize = 1;

pub const DEFAULT_GATEWAY: &str = "https://nsite.lol";

pub const KNOWN_GATEWAYS: &[&str] = &[
    "https://nsite.lol",
    "https://nsite.run",
    "https://nwb.tf",
    "https://nosto.re",
    "https://nsite.cloud",
    "https://shakespeare.to",
    "https://pages.gittr.space",
];

#[derive(Debug, Clone, PartialEq)]
pub struct SiteManifest {
    pub d_tag: Option<String>,
    pub paths: Vec<(String, String)>,
    pub servers: Vec<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub source: Option<String>,
    pub relays: Vec<String>,
    pub created_at: u64,
    pub pubkey: String,
    pub event_id: Option<EventId>,
}

impl SiteManifest {
    pub fn path_count(&self) -> usize {
        self.paths.len()
    }

    pub fn has_entry_file(&self) -> bool {
        self.paths.iter().any(|(p, _)| {
            let normalized = p.trim_start_matches('/').to_lowercase();
            normalized == "index.html" || normalized == "404.html" || normalized == "index.md"
        })
    }

    pub fn site_url(&self, gateway: &str) -> String {
        build_nsite_url(gateway, &self.pubkey, self.d_tag.as_deref())
    }
}

pub fn parse_manifest(event: &Event) -> SiteManifest {
    let mut d_tag = None;
    let mut paths = Vec::new();
    let mut servers = Vec::new();
    let mut title = None;
    let mut description = None;
    let mut source = None;
    let mut relays = Vec::new();

    for tag in event.tags.iter() {
        match tag.as_standardized() {
            Some(TagStandard::Identifier(d)) => d_tag = Some(d.clone()),
            Some(TagStandard::Title(t)) => title = Some(t.clone()),
            Some(TagStandard::Description(d)) => description = Some(d.clone()),
            Some(TagStandard::Relays(rs)) => relays = rs.iter().map(|r| r.to_string()).collect(),
            _ => {}
        }

        let kind = tag.kind();
        if kind == TagKind::Custom(Cow::Borrowed("path")) {
            let slice = tag.as_slice();
            if slice.len() >= 3 {
                paths.push((slice[1].clone(), slice[2].clone()));
            }
        } else if kind == TagKind::Custom(Cow::Borrowed("server")) {
            if let Some(s) = tag.content() {
                servers.push(s.to_string());
            }
        } else if kind == TagKind::Custom(Cow::Borrowed("source")) {
            if let Some(s) = tag.content() {
                source = Some(s.to_string());
            }
        }
    }

    if let Some(t) = &title {
        if title.is_none() {
            title = Some(t.clone());
        }
    }

    SiteManifest {
        d_tag,
        paths,
        servers,
        title,
        description,
        source,
        relays,
        created_at: event.created_at.as_secs(),
        pubkey: event.pubkey.to_hex(),
        event_id: Some(event.id),
    }
}

pub fn build_manifest_tags(
    d_tag: Option<&str>,
    paths: &[(String, String)],
    servers: &[String],
    title: Option<&str>,
    description: Option<&str>,
    source: Option<&str>,
    relays: &[String],
) -> Vec<Tag> {
    let mut tags = Vec::new();

    if let Some(d) = d_tag {
        tags.push(Tag::identifier(d.to_string()));
    }

    for (path, hash) in paths {
        tags.push(Tag::custom(
            TagKind::custom("path"),
            vec![path.clone(), hash.clone()],
        ));
    }

    for server in servers {
        tags.push(Tag::custom(
            TagKind::custom("server"),
            vec![server.clone()],
        ));
    }

    if let Some(t) = title {
        tags.push(Tag::custom(TagKind::custom("title"), vec![t.to_string()]));
    }

    if let Some(d) = description {
        tags.push(Tag::custom(
            TagKind::custom("description"),
            vec![d.to_string()],
        ));
    }

    if let Some(s) = source {
        tags.push(Tag::custom(
            TagKind::custom("source"),
            vec![s.to_string()],
        ));
    }

    for relay in relays {
        tags.push(Tag::custom(
            TagKind::custom("relay"),
            vec![relay.clone()],
        ));
    }

    tags
}

pub fn pubkey_hex_to_base36(hex: &str) -> String {
    let h = hex.trim().to_lowercase();
    if h.len() != 64 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return String::new();
    }

    let n = BigUint::parse_bytes(h.as_bytes(), 16).unwrap_or(BigUint::ZERO);
    if n == BigUint::ZERO {
        return "0".repeat(50);
    }

    let alphabet = "0123456789abcdefghijklmnopqrstuvwxyz";
    let base = BigUint::from(36u32);
    let mut n = n;
    let mut out = String::new();

    while n > BigUint::ZERO {
        let digits = (&n % &base).to_u32_digits();
        let rem = if digits.is_empty() { 0 } else { digits[0] as usize };
        out.insert(0, alphabet.chars().nth(rem).unwrap_or('0'));
        n /= &base;
    }

    if out.len() < 50 {
        out = "0".repeat(50 - out.len()) + &out;
    }

    out
}

pub fn build_nsite_url(gateway: &str, pubkey: &str, d_tag: Option<&str>) -> String {
    let base = gateway.trim_end_matches('/');
    let host = match url::Url::parse(base) {
        Ok(u) => u.host_str().unwrap_or("nsite.lol").to_string(),
        Err(_) => "nsite.lol".to_string(),
    };
    let protocol = if base.starts_with("https://") {
        "https:"
    } else {
        "http:"
    };

    match d_tag {
        None | Some("") => {
            let npub = nostr_sdk::prelude::PublicKey::from_hex(pubkey)
                .ok()
                .map(|pk| pk.to_bech32().unwrap_or_default())
                .unwrap_or_default();
            if npub.is_empty() {
                format!("{protocol}//{host}/")
            } else {
                format!("{protocol}//{npub}.{host}/")
            }
        }
        Some(d) => {
            let b36 = pubkey_hex_to_base36(pubkey);
            if b36.is_empty() {
                format!("{protocol}//{host}/")
            } else {
                format!("{protocol}//{b36}{d}.{host}/")
            }
        }
    }
}

pub fn slug_to_nsite_dtag(slug: &str) -> String {
    let mut s = slug
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s = s.trim_matches('-').to_string();

    if s.len() > MAX_DTAG_LEN {
        s = s[..MAX_DTAG_LEN].trim_end_matches('-').to_string();
    }

    if s.is_empty() {
        s = "site".to_string();
    }

    if s.ends_with('-') {
        s = s.trim_end_matches('-').to_string();
        if s.is_empty() {
            s = "site".to_string();
        }
    }

    s
}

const RESERVED_SLUGS: &[&str] = &[
    "www", "api", "app", "apps", "admin", "root", "host", "mail", "email", "ftp", "ssh", "git",
    "cdn", "static", "assets", "status", "health", "metrics", "shop", "store", "pay", "payment",
    "wallet", "billing", "invoice", "lnurl", "nwc", "login", "signin", "signup", "logout", "oauth",
    "auth", "verify", "secure", "support", "help", "info", "contact", "donate", "abuse", "legal",
    "terms", "privacy", "security", "update", "download", "install", "ns", "mx", "dmarc", "pages",
    "gittr", "nostr", "bridge", "relay", "well-known", "localhost",
];

pub fn is_reserved_slug(d_tag: &str) -> bool {
    let n = d_tag.trim().to_lowercase();
    if n.is_empty() {
        return true;
    }
    if RESERVED_SLUGS.contains(&n.as_str()) {
        return true;
    }
    for r in RESERVED_SLUGS {
        if n.starts_with(&format!("{}-", r)) || n.starts_with(&format!("{}_", r)) {
            return true;
        }
    }
    false
}

pub fn content_type_for_path(path: &str) -> String {
    let lower = path.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");

    match ext {
        "html" | "htm" => "text/html; charset=utf-8".to_string(),
        "css" => "text/css; charset=utf-8".to_string(),
        "js" | "mjs" | "cjs" => "text/javascript".to_string(),
        "json" => "application/json; charset=utf-8".to_string(),
        "map" => "application/json".to_string(),
        "webmanifest" => "application/manifest+json".to_string(),
        "txt" => "text/plain; charset=utf-8".to_string(),
        "md" => "text/markdown; charset=utf-8".to_string(),
        "svg" => "image/svg+xml".to_string(),
        "png" => "image/png".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        "ico" => "image/vnd.microsoft.icon".to_string(),
        "woff" => "font/woff".to_string(),
        "woff2" => "font/woff2".to_string(),
        "ttf" => "font/ttf".to_string(),
        "otf" => "font/otf".to_string(),
        "xml" => "application/xml; charset=utf-8".to_string(),
        "wasm" => "application/wasm".to_string(),
        "webm" => "video/webm".to_string(),
        "mp4" => "video/mp4".to_string(),
        "weba" => "audio/webm".to_string(),
        "mp3" => "audio/mpeg".to_string(),
        "ogg" => "audio/ogg".to_string(),
        "pdf" => "application/pdf".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

const SKIP_PREFIXES: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    ".next",
    "__pycache__",
    ".cache",
    ".turbo",
    "coverage",
    ".nuxt",
    ".output",
    ".vercel",
    ".netlify",
];

const STATIC_EXTENSIONS: &[&str] = &[
    "html", "htm", "css", "js", "mjs", "cjs", "json", "webmanifest", "map", "txt", "md", "xml",
    "svg", "png", "jpg", "jpeg", "gif", "webp", "ico", "webp", "woff", "woff2", "ttf", "otf",
    "wasm", "webm", "mp4", "weba", "mp3", "ogg", "pdf",
];

pub fn is_static_file(path: &str) -> bool {
    let normalized = path.trim_start_matches("./").trim_start_matches('/');
    if normalized.is_empty() {
        return false;
    }
    for prefix in SKIP_PREFIXES {
        if normalized.starts_with(&format!("{}/", prefix)) || normalized == *prefix {
            return false;
        }
    }
    let ext = normalized.rsplit('.').next().unwrap_or("");
    STATIC_EXTENSIONS.contains(&ext)
}

pub fn has_pages_entry_file(files: &[crate::services::git_types::FileEntry]) -> bool {
    if files.is_empty() {
        return false;
    }
    files.iter().any(|f| {
        let p = f
            .path
            .trim_start_matches("./")
            .trim_start_matches('/')
            .trim()
            .to_lowercase();
        if p.is_empty() || p.contains('/') {
            return false;
        }
        p == "index.html" || p == "404.html" || p == "index.md"
    })
}

pub const PAGES_README_BEGIN: &str = "<!-- pages:begin -->";
pub const PAGES_README_END: &str = "<!-- pages:end -->";

pub fn build_pages_readme_block(site_url: &str, d_tag: &str) -> String {
    format!(
        "\n\n{begin}\n## Static Pages\n\n**Live site** (`{d_tag}`): {url}\n\nPublished on relays per [NIP-5A](https://github.com/nostr-protocol/nips/blob/master/5A.md).\n{end}\n",
        begin = PAGES_README_BEGIN,
        end = PAGES_README_END,
        d_tag = d_tag,
        url = site_url,
    )
}

pub fn upsert_pages_readme_section(readme: &str, site_url: &str, d_tag: &str) -> String {
    let block = build_pages_readme_block(site_url, d_tag).trim().to_string();
    let re = regex::Regex::new(r"<!-- pages:begin -->[\s\S]*?<!-- pages:end -->").unwrap();
    if re.is_match(readme) {
        re.replace(readme, block.as_str()).to_string()
    } else {
        let cur = readme.trim_end().to_string();
        if cur.is_empty() {
            block
        } else {
            format!("{}\n\n{}", cur, block)
        }
    }
}

pub fn validate_pages_readme_block(readme: &str, site_url: &str) -> Result<(), String> {
    let trimmed = readme.trim();
    if trimmed.is_empty() {
        return Err("README is empty. Add a Pages block or enable auto-update on push.".to_string());
    }
    if !readme.contains("pages:begin") {
        return Err("Add the fenced Pages block (<!-- pages:begin --> ... <!-- pages:end -->) or enable auto-update.".to_string());
    }
    let re = regex::Regex::new(r"<!-- pages:begin -->[\s\S]*?<!-- pages:end -->").unwrap();
    let m = re.find(readme);
    if m.is_none() {
        return Err("Found begin marker but block is incomplete (missing <!-- pages:end -->).".to_string());
    }
    let block = m.unwrap().as_str();
    let url_no_slash = site_url.trim_end_matches('/');
    if !block.contains(site_url) && !block.contains(url_no_slash) {
        return Err(format!("The Pages README block must include the live URL:\n{}", site_url));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pubkey_hex_to_base36() {
        let hex = "266815e0c9210dfa324c6cba3573b14bee49da4209a9456f9484e5106cd408a5";
        let b36 = pubkey_hex_to_base36(hex);
        assert_eq!(b36.len(), 50);
        assert!(b36.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_slug_to_nsite_dtag() {
        assert_eq!(slug_to_nsite_dtag("my-project"), "my-project");
        assert_eq!(slug_to_nsite_dtag("My Project!"), "my-project");
        assert_eq!(slug_to_nsite_dtag("a-very-long-name-that-exceeds"), "a-very-long-n");
        assert_eq!(slug_to_nsite_dtag(""), "site");
        assert_eq!(slug_to_nsite_dtag("---"), "site");
    }

    #[test]
    fn test_is_reserved_slug() {
        assert!(is_reserved_slug("www"));
        assert!(is_reserved_slug("api"));
        assert!(!is_reserved_slug("my-app"));
        assert!(is_reserved_slug("gittr"));
        assert!(is_reserved_slug("pages"));
    }

    #[test]
    fn test_content_type_for_path() {
        assert_eq!(content_type_for_path("/index.html"), "text/html; charset=utf-8");
        assert_eq!(content_type_for_path("app.wasm"), "application/wasm");
        assert_eq!(content_type_for_path("style.css"), "text/css; charset=utf-8");
    }

    #[test]
    fn test_is_static_file() {
        assert!(is_static_file("index.html"));
        assert!(is_static_file("assets/app.js"));
        assert!(is_static_file("font.woff2"));
        assert!(!is_static_file("node_modules/foo.js"));
        assert!(!is_static_file(".git/config"));
        assert!(!is_static_file("README"));
    }
}
