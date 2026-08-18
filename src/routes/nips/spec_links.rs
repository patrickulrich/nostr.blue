//! Rewrite relative cross-spec links in rendered spec HTML.
//!
//! Spec docs (NIPs/NUTs/BUDs/NKBIPs) copy their cross-references verbatim from
//! upstream as relative `.md` links — e.g. `[NIP-13](13.md)`, or NUT reference
//! definitions like `[04]: 04.md` consumed via `[minting tokens][04]`. After
//! rendering these become `<a href="13.md">`, which resolve to nothing in the
//! app (our files are named `nip_13.md`, and we don't even ship NIP-13).
//!
//! This module post-processes the rendered HTML so those anchors actually work:
//!
//! - Links to specs we **support** → in-app `/nips/<route_id>` (same-tab).
//! - Links to specs we **don't** support → the canonical upstream URL (GitHub
//!   for NIPs/NUTs/BUDs, nostr.blue wiki for NKBIPs) opened in a new tab.
//!
//! Bare-number links (e.g. `13.md`) are resolved under the source document's
//! spec family, so `13.md` in a NUT doc means NUT-13 while in a NIP doc it means
//! NIP-13 — matching the upstream repositories' own cross-linking convention.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::routes::nips::registry::{self, SpecType};

/// Matches an opening `<a ...>` tag, capturing its full attribute list.
static ANCHOR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)<a\b(?P<attrs>[^>]*)>"#).expect("anchor regex"));

/// Extracts an `href="..."` value from within an anchor's attribute list.
static HREF_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\bhref="(?P<href>[^"]+)""#).expect("href regex"));

/// Rewrite spec cross-links in already-rendered, sanitized HTML.
///
/// `source` is the spec family of the document the HTML came from, used to
/// disambiguate bare-number references (`13.md` → NIP-13 vs NUT-13).
pub fn rewrite_spec_link_html(html: &str, source: SpecType) -> String {
    ANCHOR_RE
        .replace_all(html, |caps: &regex::Captures| {
            let attrs = caps.name("attrs").map(|m| m.as_str()).unwrap_or("");
            let Some(hcaps) = HREF_RE.captures(attrs) else {
                // No href on this anchor — leave it untouched.
                return caps[0].to_string();
            };
            let href = &hcaps["href"];
            let Some((new_href, external)) = resolve_spec_href(href, source) else {
                // Not a relative spec-file reference — leave it untouched.
                return caps[0].to_string();
            };
            // Swap the href value, preserving all other attributes (e.g. the
            // `rel="noopener noreferrer"` ammonia already added). `replacen`
            // with count 1 replaces only the matched href occurrence (an
            // anchor has exactly one href; ammonia sanitizes beforehand).
            let replaced = format!("href=\"{new_href}\"");
            let mut new_attrs = attrs.replacen(&hcaps[0], &replaced, 1);
            // External (unsupported) links open in a new tab so the user stays
            // in the app. Supported links navigate same-tab (in-app route).
            if external && !new_attrs.contains("target=") {
                new_attrs.push_str(" target=\"_blank\"");
            }
            format!("<a{}>", new_attrs)
        })
        .to_string()
}

/// Resolve a relative spec-file href to `(new_href, is_external)`.
///
/// Returns `None` for anything that isn't a relative spec reference (absolute
/// URLs, in-app paths, fragments, non-`.md` links), so those anchors are left
/// untouched by the caller.
fn resolve_spec_href(href: &str, source: SpecType) -> Option<(String, bool)> {
    if href.contains("://")
        || href.starts_with('/')
        || href.starts_with('#')
        || href.starts_with("mailto:")
    {
        return None;
    }
    let (spec_type, number) = parse_spec_file(href, source)?;
    let route_id = format!("{}-{}", spec_type.prefix(), number);
    match registry::find(&route_id) {
        // Supported -> in-app detail route (same-tab navigation).
        Some(spec) => Some((format!("/nips/{}", spec.route_id()), false)),
        // Unsupported -> canonical upstream URL, opened in a new tab.
        None => Some((registry::upstream_url_for(spec_type, &number), true)),
    }
}

/// Parse a relative spec filename into `(spec family, number)`.
///
/// Explicit prefixes (`nip_29.md`, `nut_05.md`, …) resolve to their own family.
/// Bare numbers (`13.md`) inherit the source document's family.
fn parse_spec_file(href: &str, source: SpecType) -> Option<(SpecType, String)> {
    // Strip any query/fragment before inspecting the extension.
    let path = href.split(['?', '#']).next().unwrap_or(href);
    if !path.ends_with(".md") {
        return None;
    }
    let stem = path.trim_end_matches(".md");
    for (prefix, spec_type) in [
        ("nip_", SpecType::Nip),
        ("nut_", SpecType::Nut),
        ("bud_", SpecType::Bud),
        ("nkbip_", SpecType::Nkbip),
        ("dip_", SpecType::Dip),
    ] {
        if let Some(num) = stem.strip_prefix(prefix) {
            if is_spec_number(num) {
                return Some((spec_type, num.to_string()));
            }
        }
    }
    // Bare number -> source family.
    if is_spec_number(stem) {
        return Some((source, stem.to_string()));
    }
    None
}

/// A spec number is 1–4 hex digits (covers `01`, `13`, `100`, `5A`, `C7`, `F4`).
/// Restricting to hex avoids misclassifying incidental links like `readme.md`.
fn is_spec_number(s: &str) -> bool {
    (1..=4).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_nip_bare_link_goes_to_github() {
        let html = r#"<p>see <a href="13.md" rel="noopener noreferrer">NIP-13</a></p>"#;
        let out = rewrite_spec_link_html(html, SpecType::Nip);
        assert!(out.contains("href=\"https://github.com/nostr-protocol/nips/blob/master/13.md\""));
        // External (unsupported) links open in a new tab.
        assert!(out.contains("target=\"_blank\""));
        // ammonia's rel is preserved.
        assert!(out.contains("rel=\"noopener noreferrer\""));
    }

    #[test]
    fn supported_nip_bare_link_goes_in_app() {
        let html = r#"<a href="29.md" rel="noopener noreferrer">NIP-29</a>"#;
        let out = rewrite_spec_link_html(html, SpecType::Nip);
        assert!(out.contains("href=\"/nips/nip-29\""));
        assert!(!out.contains("target=\"_blank\""));
    }

    #[test]
    fn bare_number_inherits_source_family() {
        // NUT-13 is supported; a bare `13.md` in a NUT doc resolves to NUT-13.
        let html = r#"<a href="13.md">NUT-13</a>"#;
        let out = rewrite_spec_link_html(html, SpecType::Nut);
        assert!(out.contains("href=\"/nips/nut-13\""));
        // The same bare `13.md` in a NIP doc resolves to NIP-13 (unsupported -> GitHub).
        let out2 = rewrite_spec_link_html(html, SpecType::Nip);
        assert!(out2.contains("nips/blob/master/13.md"));
    }

    #[test]
    fn explicit_prefix_link_resolves_regardless_of_source() {
        // `nut_05.md` inside a NIP doc still resolves to NUT-05 (in-app).
        let html = r#"<a href="nut_05.md">NUT-05</a>"#;
        let out = rewrite_spec_link_html(html, SpecType::Nip);
        assert!(out.contains("href=\"/nips/nut-05\""));
    }

    #[test]
    fn absolute_and_fragment_and_non_md_links_untouched() {
        let html = concat!(
            r#"<a href="https://example.com">x</a>"#,
            r#"<a href="/nips/nip-01">x</a>"#,
            r##"<a href="#serialization">x</a>"##,
            r#"<a href="image.png">x</a>"#,
        );
        let out = rewrite_spec_link_html(html, SpecType::Nip);
        assert_eq!(out, html);
    }

    #[test]
    fn anchors_without_href_untouched() {
        let html = r#"<a name="foo"></a>"#;
        let out = rewrite_spec_link_html(html, SpecType::Nip);
        assert_eq!(out, html);
    }

    #[test]
    fn non_spec_md_filename_untouched() {
        // `readme.md` is not a hex spec number - left alone.
        let html = r#"<a href="readme.md">README</a>"#;
        let out = rewrite_spec_link_html(html, SpecType::Nip);
        assert_eq!(out, html);
    }

    #[test]
    fn does_not_match_other_a_tags_like_abbr() {
        let html = r#"<abbr title="x">y</abbr>"#;
        let out = rewrite_spec_link_html(html, SpecType::Nip);
        assert_eq!(out, html);
    }
}
