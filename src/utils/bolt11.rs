/// Parse amount from bolt11 invoice string
pub fn parse_bolt11_amount(bolt11: &str) -> Option<u64> {
    let lower = bolt11.to_lowercase();
    let prefix_end = if lower.starts_with("lnbcrt") {
        6
    } else if lower.starts_with("lnbc") || lower.starts_with("lntb") {
        4
    } else {
        return None;
    };
    let amount_part = &lower[prefix_end..];
    let mut amount_str = String::new();
    let mut multiplier_char = None;
    for c in amount_part.chars() {
        if c.is_ascii_digit() {
            amount_str.push(c);
        } else if c == 'p' || c == 'n' || c == 'u' || c == 'm' {
            multiplier_char = Some(c);
            break;
        } else {
            break;
        }
    }
    let amount: u64 = amount_str.parse().ok()?;
    let sats = match multiplier_char {
        Some('m') => amount * 100_000,
        Some('u') => amount * 100,
        Some('n') => amount / 10,
        Some('p') => amount / 10_000,
        None => amount * 100_000_000,
        _ => return None,
    };
    Some(sats)
}
