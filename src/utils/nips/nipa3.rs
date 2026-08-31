//! NIP-A3 payment targets: `payto` tags on replaceable kind 10133 events.
//!
//! Wire format (NIP-A3): each target is a `["payto", "<type>", "<address>"]`
//! tag. Types are lowercase; this module canonicalizes common ticker aliases
//! (e.g. `btc` → `bitcoin`, `lnurl` → `lightning`, `cashapp` → `cashme`)
//! so events published by different producers render consistently.
//!
//! Rendering policy: prefer a native scheme when one is broadly deployed
//! (`bitcoin:`, `monero:`, …), use the payment page for custodial handles
//! (`https://cash.app/$…`), and fall back to the RFC-8905 `payto://<type>/<address>`
//! URI for unknown types. Silent-payment codes (BIP-352) have no registered
//! scheme and are shared by copying / QR of the raw code.
//!
//! Parsing is permissive: unknown types are kept and rendered generically
//! (the NIP's own example renders an unrecognized type), and every declared
//! target is kept — deduplication only removes exact duplicates.
use crate::stores::nostr_client;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

/// Replaceable kind declaring a user's payment targets (NIP-A3).
pub const KIND_PAYMENT_TARGETS: u16 = 10133;

const PAYTO_TAG: &str = "payto";
const ALT_TEXT: &str = "Declares payment addresses (NIP-A3 payto targets)";

/// A single parsed `payto` target.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PayToTarget {
    /// Canonical lowercase payment type (e.g. `bitcoin`, `cashme`) or a
    /// trimmed lowercase unknown type.
    pub payto_type: String,
    /// The address / handle / lightning address.
    pub address: String,
}

/// How a target is consumed by the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderKind {
    /// Lightning-family: routes through the existing zap / LNURL flow.
    NativeLightning,
    /// On-chain bitcoin: `bitcoin:` URI with QR + copy.
    NativeBitcoin,
    /// Everything else: QR + copy + native-scheme or payment-page handoff.
    Generic,
}

/// Static metadata for a canonical NIP-A3 payment type.
pub struct PayToMethodDef {
    pub type_key: &'static str,
    pub label: &'static str,
    pub ticker: &'static str,
    pub render: RenderKind,
    pub placeholder: &'static str,
}

impl PayToMethodDef {
    /// Preferred clickable URI for an address of this type, if any.
    pub fn uri(&self, address: &str) -> Option<String> {
        let addr = address.trim();
        match self.type_key {
            "bitcoin" | "bip353" => Some(format!("bitcoin:{addr}")),
            "lightning" => Some(format!("lightning:{addr}")),
            "monero" => Some(format!("monero:{addr}")),
            "ethereum" => Some(format!("ethereum:{addr}")),
            "litecoin" => Some(format!("litecoin:{addr}")),
            "zcash" => Some(format!("zcash:{addr}")),
            "nano" => Some(format!("nano:{addr}")),
            "solana" => Some(format!("solana:{addr}")),
            "cashme" => {
                let handle = strip_handle(addr);
                Some(format!("https://cash.app/${handle}"))
            }
            "venmo" => {
                let handle = strip_handle(addr);
                Some(format!("https://venmo.com/{handle}"))
            }
            "paypal" => {
                let handle = strip_handle(addr);
                Some(format!("https://paypal.me/{handle}"))
            }
            "revolut" => {
                let handle = strip_handle(addr);
                Some(format!("https://revolut.me/{handle}"))
            }
            // Silent-payment codes (BIP-352) have no registered URI scheme;
            // they are consumed by copying or scanning the raw code.
            "bip352" => None,
            _ => None,
        }
    }

    /// Soft validation for the editor. Unknown/custom types accept any
    /// non-empty value.
    pub fn validate(&self, address: &str) -> bool {
        let addr = address.trim();
        if addr.is_empty() {
            return false;
        }
        match self.type_key {
            "bitcoin" => is_bitcoin_address(addr),
            "lightning" => is_lightning_authority(addr),
            "monero" => is_monero_address(addr),
            "ethereum" => is_evm_address(addr),
            "litecoin" => is_litecoin_address(addr),
            "zcash" => is_zcash_address(addr),
            "nano" => is_nano_address(addr),
            "solana" => is_solana_address(addr),
            "cashme" | "venmo" | "paypal" | "revolut" => is_payment_handle(addr),
            "bip352" => is_silent_payment_code(addr),
            "bip353" => is_handle_domain(addr),
            _ => true,
        }
    }
}

/// Canonical payment types from the NIP-A3 "commonly used" table.
pub const PAYMENT_METHODS: &[PayToMethodDef] = &[
    PayToMethodDef { type_key: "bitcoin", label: "Bitcoin", ticker: "BTC", render: RenderKind::NativeBitcoin, placeholder: "bc1… or sp1…" },
    PayToMethodDef { type_key: "lightning", label: "Lightning", ticker: "LBTC", render: RenderKind::NativeLightning, placeholder: "you@walletofsatoshi.com" },
    PayToMethodDef { type_key: "monero", label: "Monero", ticker: "XMR", render: RenderKind::Generic, placeholder: "4… (Monero address)" },
    PayToMethodDef { type_key: "ethereum", label: "Ethereum", ticker: "ETH", render: RenderKind::Generic, placeholder: "0x… (Ethereum address)" },
    PayToMethodDef { type_key: "litecoin", label: "Litecoin", ticker: "LTC", render: RenderKind::Generic, placeholder: "ltc1… / L… (Litecoin address)" },
    PayToMethodDef { type_key: "zcash", label: "Zcash", ticker: "ZEC", render: RenderKind::Generic, placeholder: "zs1… / u1… (Zcash address)" },
    PayToMethodDef { type_key: "nano", label: "Nano", ticker: "XNO", render: RenderKind::Generic, placeholder: "nano_… (Nano address)" },
    PayToMethodDef { type_key: "solana", label: "Solana", ticker: "SOL", render: RenderKind::Generic, placeholder: "Base58 Solana address" },
    PayToMethodDef { type_key: "cashme", label: "Cash App", ticker: "Cash App", render: RenderKind::Generic, placeholder: "$cashtag" },
    PayToMethodDef { type_key: "venmo", label: "Venmo", ticker: "Venmo", render: RenderKind::Generic, placeholder: "@username" },
    PayToMethodDef { type_key: "paypal", label: "PayPal", ticker: "PayPal", render: RenderKind::Generic, placeholder: "PayPal.me username" },
    PayToMethodDef { type_key: "revolut", label: "Revolut", ticker: "Revolut", render: RenderKind::Generic, placeholder: "@revtag" },
    PayToMethodDef { type_key: "bip352", label: "Silent Payments", ticker: "BIP-352", render: RenderKind::Generic, placeholder: "sp1… (silent payment code)" },
    PayToMethodDef { type_key: "bip353", label: "Bitcoin DNS", ticker: "BIP-353", render: RenderKind::Generic, placeholder: "you@domain.tld" },
];

/// Map common ticker aliases and alternate spellings to canonical types.
fn canonical_alias(type_str: &str) -> Option<&'static str> {
    match type_str {
        "btc" | "onchain" => Some("bitcoin"),
        "ln" | "lnurl" => Some("lightning"),
        "eth" => Some("ethereum"),
        "xmr" => Some("monero"),
        "ltc" => Some("litecoin"),
        "zec" => Some("zcash"),
        "xno" | "xrb" => Some("nano"),
        "sol" => Some("solana"),
        "cashapp" => Some("cashme"),
        "sp" | "silentpayments" => Some("bip352"),
        "bip353" | "dns" => Some("bip353"),
        _ => None,
    }
}

/// Resolve a canonical method definition for a type string (canonical or alias).
pub fn method_for(type_str: &str) -> Option<&'static PayToMethodDef> {
    PAYMENT_METHODS
        .iter()
        .find(|m| m.type_key == type_str)
        .or_else(|| canonical_alias(type_str).and_then(|c| PAYMENT_METHODS.iter().find(|m| m.type_key == c)))
}

/// Normalize a raw type: trim, lowercase, canonicalize aliases.
pub fn normalize_type(raw: &str) -> String {
    let trimmed = raw.trim().to_lowercase();
    canonical_alias(&trimmed).unwrap_or(&trimmed).to_string()
}

/// Parse the `payto` tags of a kind 10133 event into targets.
///
/// Permissive: keeps unknown types (rendered generically) and every declared
/// target; deduplicates exact `(canonical type, address)` pairs. Output is
/// ordered by the canonical registry first, then unknown types in first-seen
/// order, for stable rendering.
pub fn parse_payto_targets(event: &Event) -> Vec<PayToTarget> {
    if event.kind.as_u16() != KIND_PAYMENT_TARGETS {
        return Vec::new();
    }
    let mut canonical: Vec<PayToTarget> = Vec::new();
    let mut unknown: Vec<PayToTarget> = Vec::new();
    for tag in event.tags.iter() {
        let values = tag.as_slice();
        if values.first().map(|s| s.as_str()) != Some(PAYTO_TAG) {
            continue;
        }
        let (Some(raw_type), Some(raw_address)) = (values.get(1), values.get(2)) else {
            continue;
        };
        let payto_type = normalize_type(raw_type);
        let address = raw_address.trim().to_string();
        if payto_type.is_empty() || address.is_empty() {
            continue;
        }
        let target = PayToTarget { payto_type, address };
        let bucket = if method_for(&target.payto_type).is_some() {
            &mut canonical
        } else {
            &mut unknown
        };
        if !bucket.contains(&target) {
            bucket.push(target);
        }
    }
    canonical.sort_by_key(|t| {
        PAYMENT_METHODS
            .iter()
            .position(|m| m.type_key == t.payto_type)
            .unwrap_or(usize::MAX)
    });
    canonical.extend(unknown);
    canonical
}

/// RFC-8905 fallback URI for unknown types, hardened for hostile events:
/// the type must be a lowercase token (`^[a-z0-9][a-z0-9-]*$`), the address
/// is percent-encoded (raw interpolation would hand spaces, `?`/`#`/`/`
/// metacharacters and control bytes to OS URI handlers), and oversized
/// inputs are rejected. `None` makes the chip copy/QR-only.
fn fallback_payto_uri(payto_type: &str, address: &str) -> Option<String> {
    const MAX_TYPE_BYTES: usize = 32;
    const MAX_ADDRESS_BYTES: usize = 512;
    if payto_type.is_empty() || payto_type.len() > MAX_TYPE_BYTES {
        return None;
    }
    let first_alnum = payto_type
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let charset_ok = payto_type
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !first_alnum || !charset_ok {
        return None;
    }
    let addr = address.trim();
    if addr.is_empty() || addr.len() > MAX_ADDRESS_BYTES {
        return None;
    }
    Some(format!(
        "payto://{payto_type}/{}",
        urlencoding::encode(addr)
    ))
}

/// Preferred clickable URI for a target, honoring the per-type policy.
/// `None` for types without a usable scheme (QR / copy show the raw address).
pub fn uri_for(target: &PayToTarget) -> Option<String> {
    if let Some(method) = method_for(&target.payto_type) {
        return method.uri(&target.address);
    }
    // RFC-8905 fallback for unknown types (validated + percent-encoded).
    fallback_payto_uri(&target.payto_type, &target.address)
}

/// Display label: canonical label, or the type capitalized.
pub fn label_for(target: &PayToTarget) -> String {
    method_for(&target.payto_type)
        .map(|m| m.label.to_string())
        .unwrap_or_else(|| {
            let mut chars = target.payto_type.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Pay".to_string(),
            }
        })
}

/// Render kind for a target (generic for unknown types).
pub fn render_kind_for(target: &PayToTarget) -> RenderKind {
    method_for(&target.payto_type)
        .map(|m| m.render)
        .unwrap_or(RenderKind::Generic)
}

/// Short display form: handles and short values shown as-is, long addresses
/// truncated to `first8…last4`. Truncation snaps to char boundaries — the
/// address comes from untrusted kind-10133 events and byte-slicing
/// multibyte content would panic the render path.
pub fn short_address(address: &str) -> String {
    if address.contains('@') || address.contains('/') || address.len() <= 18 {
        return address.to_string();
    }
    let prefix_len = address
        .char_indices()
        .take_while(|(i, _)| *i < 8)
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    let mut suffix_start = address.len().saturating_sub(4);
    while suffix_start < address.len() && !address.is_char_boundary(suffix_start) {
        suffix_start += 1;
    }
    format!("{}…{}", &address[..prefix_len], &address[suffix_start..])
}

/// Build a kind 10133 event builder from targets, preserving any unrelated
/// tags and content from a previous version (read-modify-write).
pub fn build_payto_event(targets: &[PayToTarget], previous: Option<&Event>) -> EventBuilder {
    let mut tags: Vec<Tag> = previous
        .map(|prev| {
            prev.tags
                .iter()
                .filter(|tag| {
                    let name = tag.as_slice().first().map(|s| s.as_str()).unwrap_or("");
                    name != PAYTO_TAG && name != "alt"
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    for target in targets {
        tags.push(Tag::custom(
            TagKind::custom(PAYTO_TAG),
            vec![target.payto_type.clone(), target.address.clone()],
        ));
    }
    tags.push(Tag::custom(
        TagKind::custom("alt"),
        vec![ALT_TEXT.to_string()],
    ));
    let content = previous.map(|prev| prev.content.as_str()).unwrap_or("");
    EventBuilder::new(Kind::Custom(KIND_PAYMENT_TARGETS), content).tags(tags)
}

/// Publish the signed-in user's payment targets (kind 10133) through the
/// publish queue. The replaceable kind coalesces with any queued version.
/// Returns the event id.
pub async fn publish_payment_targets(targets: Vec<PayToTarget>) -> Result<String, String> {
    nostr_client::get_client().ok_or("Client not initialized")?;
    if !nostr_client::has_signer() {
        return Err("No signer available".to_string());
    }

    // Read-modify-write: preserve content and unrelated tags of the current
    // version, if any.
    let prev = fetch_own_payment_targets_event().await;
    let builder = build_payto_event(&targets, prev.as_ref());

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign payment targets: {e}"))?;
    let event_id = event.id.to_hex();

    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other(
            "payment_targets".to_string(),
        ),
        None,
        HashMap::new(),
    )
    .await;

    Ok(event_id)
}

/// Fetch the signed-in user's current kind 10133 event (unsigned fetch of
/// the latest replaceable version, gossip-routed to the author's relays).
async fn fetch_own_payment_targets_event() -> Option<Event> {
    let pubkey_hex = crate::stores::auth_store::get_pubkey()?;
    let pubkey = PublicKey::parse(&pubkey_hex).ok()?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_PAYMENT_TARGETS))
        .author(pubkey)
        .limit(1);
    nostr_client::fetch_events_aggregated_outbox(filter, Duration::from_secs(3))
        .await
        .ok()
        .and_then(|events| events.into_iter().max_by_key(|e| e.created_at))
}

// ---------------------------------------------------------------------------
// Address validators (shape checks for the editor)
// ---------------------------------------------------------------------------

fn strip_handle(address: &str) -> &str {
    let trimmed = address.trim();
    trimmed.trim_start_matches(['$', '@'])
}

fn is_bitcoin_address(address: &str) -> bool {
    // Mainnet on-chain address (base58, bech32, bech32m).
    if let Ok(unchecked) = bitcoin::Address::from_str(address) {
        if unchecked
            .require_network(bitcoin::Network::Bitcoin)
            .is_ok()
        {
            return true;
        }
    }
    // BIP-352 silent payment codes are also valid bitcoin targets.
    is_silent_payment_code(address)
}

fn is_silent_payment_code(address: &str) -> bool {
    let value = address.trim();
    if !value.starts_with("sp1") && !value.starts_with("tsp1") {
        return false;
    }
    matches!(
        bech32::decode(value).map(|(hrp, _)| hrp.as_str().to_string()),
        Ok(hrp) if hrp == "sp" || hrp == "tsp"
    )
}

fn is_lightning_authority(address: &str) -> bool {
    let value = address.trim();
    let lowered = value.to_ascii_lowercase();
    if lowered.starts_with("lnurl1") && value.len() > 10 {
        // Charset check on the data part (the `1` is the bech32 separator).
        return bech32_charset(&lowered["lnurl1".len()..]);
    }
    is_handle_domain(value)
}

fn is_handle_domain(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.contains('@')
        && !value.contains(char::is_whitespace)
}

fn is_monero_address(address: &str) -> bool {
    let value = address.trim();
    let len = value.len();
    (len == 95 || len == 106)
        && value.starts_with(['4', '8'])
        && value.chars().all(|c| c.is_ascii_alphanumeric() && !matches!(c, '0' | 'O' | 'I' | 'l'))
}

fn is_evm_address(address: &str) -> bool {
    let value = address.trim();
    value.len() == 42
        && value.starts_with("0x")
        && value[2..].chars().all(|c| c.is_ascii_hexdigit())
}

fn is_litecoin_address(address: &str) -> bool {
    let value = address.trim();
    (26..=62).contains(&value.len())
        && (value.starts_with('L')
            || value.starts_with('M')
            || value.starts_with("ltc1"))
        && value.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_zcash_address(address: &str) -> bool {
    let value = address.trim();
    (35..=100).contains(&value.len())
        && (value.starts_with("t1")
            || value.starts_with("t3")
            || value.starts_with("zs1")
            || value.starts_with("u1"))
}

fn is_nano_address(address: &str) -> bool {
    let value = address.trim();
    let rest = value
        .strip_prefix("nano_")
        .or_else(|| value.strip_prefix("xrb_"))
        .unwrap_or("");
    // Nano alphabet: 13456789abcdefghijkmnopqrstuwxyz (no 0, 2, l, v).
    rest.len() == 60
        && rest.starts_with(['1', '3'])
        && rest
            .chars()
            .all(|c| matches!(c, '1'|'3'|'4'|'5'|'6'|'7'|'8'|'9'|'a'..='k'|'m'..='u'|'w'..='z'))
}

fn is_solana_address(address: &str) -> bool {
    let value = address.trim();
    (43..=44).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() && !matches!(c, '0' | 'O' | 'I' | 'l'))
}

fn is_payment_handle(address: &str) -> bool {
    let handle = strip_handle(address);
    (1..=64).contains(&handle.len())
        && handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn bech32_charset(value: &str) -> bool {
    value
        .chars()
        .all(|c| matches!(c.to_ascii_lowercase(), 'p' | 'z' | 'r' | 'y' | '9' | 'x' | '8' | 'g' | 'f' | '2' | 't' | 'v' | 'd' | 'w' | '0' | 's' | '3' | 'j' | 'n' | '5' | '4' | 'k' | 'h' | 'c' | 'e' | '6' | 'm' | 'u' | 'a' | '7' | 'l' | 'q'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_event(tags: Vec<Tag>, content: &str) -> Event {
        let keys = Keys::generate();
        EventBuilder::new(Kind::Custom(KIND_PAYMENT_TARGETS), content)
            .tags(tags)
            .sign_with_keys(&keys)
            .unwrap()
    }

    fn payto(type_str: &str, address: &str) -> Tag {
        Tag::custom(
            TagKind::custom(PAYTO_TAG),
            vec![type_str.to_string(), address.to_string()],
        )
    }

    #[test]
    fn parses_payto_tags() {
        let event = signed_event(
            vec![
                payto("bitcoin", "bc1qxq66e0t8d7ugdecwnmv58e90tpry23nc84pg9k"),
                payto("monero", "nano_1dctqbmqxfppo9pswbm6kg9d4s4mbraqn8i4m7ob9gnzz91aurmuho48jx3c"),
            ],
            "",
        );
        let targets = parse_payto_targets(&event);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].payto_type, "bitcoin");
        assert_eq!(targets[1].payto_type, "monero");
    }

    #[test]
    fn rejects_other_kinds() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "")
            .tags(vec![payto("bitcoin", "bc1qxq66e0t8d7ugdecwnmv58e90tpry23nc84pg9k")])
            .sign_with_keys(&keys)
            .unwrap();
        assert!(parse_payto_targets(&event).is_empty());
    }

    #[test]
    fn canonicalizes_aliases() {
        let event = signed_event(
            vec![
                payto("BTC", "bc1qxq66e0t8d7ugdecwnmv58e90tpry23nc84pg9k"),
                payto("lnurl", "someone@walletofsatoshi.com"),
                payto("cashapp", "$someone"),
            ],
            "",
        );
        let targets = parse_payto_targets(&event);
        assert_eq!(targets[0].payto_type, "bitcoin");
        assert_eq!(targets[1].payto_type, "lightning");
        assert_eq!(targets[2].payto_type, "cashme");
    }

    #[test]
    fn keeps_unknown_types_in_fallback_order() {
        let event = signed_event(
            vec![
                payto("unknowntype", "l7tbta5b9xze6ckkfc99uohzxd009b0r"),
                payto("monero", "4AbCdefGhIjKlmNoPqRsTuVwXyZ1234567890123456789012345678901234567890123456789012345678"),
            ],
            "",
        );
        let targets = parse_payto_targets(&event);
        assert_eq!(targets.len(), 2);
        // Canonical types sort first regardless of tag order.
        assert_eq!(targets[0].payto_type, "monero");
        assert_eq!(targets[1].payto_type, "unknowntype");
        assert_eq!(
            uri_for(&targets[1]).unwrap(),
            "payto://unknowntype/l7tbta5b9xze6ckkfc99uohzxd009b0r"
        );
        assert_eq!(label_for(&targets[1]), "Unknowntype");
    }

    #[test]
    fn dedupes_exact_pairs() {
        let event = signed_event(
            vec![
                payto("bitcoin", "bc1qxq66e0t8d7ugdecwnmv58e90tpry23nc84pg9k"),
                payto("btc", "bc1qxq66e0t8d7ugdecwnmv58e90tpry23nc84pg9k"),
                payto("bitcoin", "1BoatSLRHtKNngkdXEeobR76b53LETtpyT"),
            ],
            "",
        );
        let targets = parse_payto_targets(&event);
        // Alias of the same address dedupes; a different address is kept.
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn skips_empty_entries() {
        let event = signed_event(
            vec![payto("", "something"), payto("bitcoin", ""), payto("  ", "  ")],
            "",
        );
        assert!(parse_payto_targets(&event).is_empty());
    }

    #[test]
    fn uri_policy_matrix() {
        let bitcoin = PayToTarget { payto_type: "bitcoin".into(), address: "bc1qxq66e0t8d7ugdecwnmv58e90tpry23nc84pg9k".into() };
        assert_eq!(uri_for(&bitcoin).unwrap(), "bitcoin:bc1qxq66e0t8d7ugdecwnmv58e90tpry23nc84pg9k");
        let ln = PayToTarget { payto_type: "lightning".into(), address: "someone@walletofsatoshi.com".into() };
        assert_eq!(uri_for(&ln).unwrap(), "lightning:someone@walletofsatoshi.com");
        let cash = PayToTarget { payto_type: "cashme".into(), address: "$handle".into() };
        assert_eq!(uri_for(&cash).unwrap(), "https://cash.app/$handle");
        let venmo = PayToTarget { payto_type: "venmo".into(), address: "@user".into() };
        assert_eq!(uri_for(&venmo).unwrap(), "https://venmo.com/user");
        let paypal = PayToTarget { payto_type: "paypal".into(), address: "user".into() };
        assert_eq!(uri_for(&paypal).unwrap(), "https://paypal.me/user");
        let sp = PayToTarget { payto_type: "bip352".into(), address: "sp1q0u2nf...".into() };
        assert!(uri_for(&sp).is_none());
    }

    #[test]
    fn validators() {
        assert!(is_bitcoin_address("bc1qxq66e0t8d7ugdecwnmv58e90tpry23nc84pg9k"));
        assert!(is_bitcoin_address("1BoatSLRHtKNngkdXEeobR76b53LETtpyT"));
        assert!(!is_bitcoin_address("not-an-address"));
        assert!(is_lightning_authority("someone@walletofsatoshi.com"));
        assert!(is_lightning_authority("LNURL1DP68GURN8GHJ7UM9WFMXJCM99E3K7MF0V9CXJ0M385EKVCENXC6R2C35"));
        assert!(!is_lightning_authority("no-domain"));
        assert!(is_evm_address("0x1234567890abcdef1234567890abcdef12345678"));
        assert!(!is_evm_address("0x123"));
        assert!(is_payment_handle("$valid.handle-1"));
        assert!(!is_payment_handle("has space"));
        assert!(is_nano_address("nano_1dctqbmqxfppo9pswbm6kg9d4s4mbraqn8i4m7ob9gnzz91aurmuho48jx3c"));
        assert!(!is_nano_address("nano_wrong"));
    }

    #[test]
    fn short_address_shapes() {
        assert_eq!(short_address("someone@domain.com"), "someone@domain.com");
        let long = "bc1qxq66e0t8d7ugdecwnmv58e90tpry23nc84pg9k";
        assert_eq!(short_address(long), "bc1qxq66…pg9k");
    }

    #[test]
    fn short_address_multibyte_does_not_panic() {
        // Hostile address: >18 bytes, no '@' or '/', byte offsets 8 and
        // len-4 land mid-char (4-byte emoji / 2-byte é). Must truncate on
        // char boundaries instead of panicking.
        let hostile = "a\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{e9}\u{e9}\u{e9}\u{e9}";
        let short = short_address(hostile);
        assert!(short.starts_with("a\u{1F600}")); // prefix ≤ 8 bytes, boundary-snapped
        assert!(short.contains('…'));
        // Suffix starts at the first boundary ≥ len-4 (len=29: bytes 25..29 = "éé").
        assert!(short.ends_with("\u{e9}\u{e9}"));

        // Pure-emoji address (every offset 1..3 off a boundary).
        let emoji = "\u{1F600}".repeat(10);
        let short = short_address(&emoji);
        assert!(short.contains('…'));

        // Mixed CJK.
        let cjk = "汉字汉字汉字汉字汉字汉字汉字汉字";
        let _ = short_address(cjk);
    }

    #[test]
    fn fallback_uri_percent_encodes_and_validates() {
        // Plain alphanumeric address passes through unchanged (matches the
        // NIP-A3 example rendering).
        let ok = PayToTarget {
            payto_type: "unknowntype".into(),
            address: "l7tbta5b9xze6ckkfc99uohzxd009b0r".into(),
        };
        assert_eq!(
            uri_for(&ok).unwrap(),
            "payto://unknowntype/l7tbta5b9xze6ckkfc99uohzxd009b0r"
        );
        // Metacharacters in the address are percent-encoded (RFC-8905 path
        // segment), never handed raw to an OS URI handler.
        let meta = PayToTarget {
            payto_type: "weird".into(),
            address: "a b?c#d/e".into(),
        };
        assert_eq!(uri_for(&meta).unwrap(), "payto://weird/a%20b%3Fc%23d%2Fe");
        // Non-ASCII addresses are encoded too.
        let unicode = PayToTarget {
            payto_type: "weird".into(),
            address: "привет".into(),
        };
        assert!(uri_for(&unicode)
            .unwrap()
            .starts_with("payto://weird/%D0%BF"));
    }

    #[test]
    fn fallback_uri_rejects_hostile_types_and_oversized() {
        for bad in [
            "has space",
            "With/Slash",
            "u:r",
            "x#y",
            "n\u{c9}w",
            "-leading",
            "",
        ] {
            let t = PayToTarget {
                payto_type: bad.to_string(),
                address: "abc".into(),
            };
            assert!(uri_for(&t).is_none(), "type {bad:?} should be rejected");
        }
        // Oversized address / type are rejected (copy/QR-only).
        let huge_addr = PayToTarget {
            payto_type: "ok".into(),
            address: "x".repeat(600),
        };
        assert!(uri_for(&huge_addr).is_none());
        let huge_type = PayToTarget {
            payto_type: "t".repeat(64),
            address: "abc".into(),
        };
        assert!(uri_for(&huge_type).is_none());
    }

    #[test]
    fn build_round_trip_preserves_content_and_tags() {
        let keys = Keys::generate();
        let prev = EventBuilder::new(Kind::Custom(KIND_PAYMENT_TARGETS), "kept content")
            .tags(vec![
                payto("monero", "4AbCdefGhIjKlmNoPqRsTuVwXyZ1234567890123456789012345678901234567890123456789012345678"),
                Tag::custom(TagKind::custom("custom"), vec!["keep".to_string()]),
            ])
            .sign_with_keys(&keys)
            .unwrap();

        let targets = vec![PayToTarget {
            payto_type: "bitcoin".into(),
            address: "bc1qxq66e0t8d7ugdecwnmv58e90tpry23nc84pg9k".into(),
        }];
        let next = build_payto_event(&targets, Some(&prev))
            .sign_with_keys(&keys)
            .unwrap();

        assert_eq!(next.content, "kept content");
        let parsed = parse_payto_targets(&next);
        assert_eq!(parsed, targets);
        // Unrelated tag survives; alt tag present.
        assert!(next.tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("custom")));
        assert!(next.tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("alt")));
    }

    #[test]
    fn build_from_scratch_has_alt() {
        let keys = Keys::generate();
        let targets = vec![PayToTarget { payto_type: "lightning".into(), address: "a@b.com".into() }];
        let event = build_payto_event(&targets, None)
            .sign_with_keys(&keys)
            .unwrap();
        assert_eq!(event.kind.as_u16(), KIND_PAYMENT_TARGETS);
        assert_eq!(parse_payto_targets(&event), targets);
        assert!(event.tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("alt")));
    }
}
