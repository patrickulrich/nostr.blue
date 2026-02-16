//! Bounty Badge Component
//!
//! Displays a bounty amount with lightning icon in compact or full format.
use dioxus::prelude::*;

/// Display a bounty amount badge with lightning bolt
#[component]
pub fn BountyBadge(amount: u64, #[props(default = false)] compact: bool) -> Element {
    let formatted = format_bounty_amount(amount);
    rsx! {
        span { class: "inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-yellow-500/20 text-yellow-400 text-xs font-medium",
            "\u{26A1}"
            if compact {
                "{formatted}"
            } else {
                "{format_with_commas(amount)} sats bounty"
            }
        }
    }
}

#[allow(dead_code)]
fn format_bounty_amount(amount: u64) -> String {
    if amount >= 1_000_000 {
        format!("{}M", amount / 1_000_000)
    } else if amount >= 1_000 {
        format!("{}K", amount / 1_000)
    } else {
        amount.to_string()
    }
}

#[allow(dead_code)]
fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Display a bounty amount badge with status indicator
#[component]
pub fn BountyStatusBadge(amount: u64, status: crate::utils::nip34::BountyStatus) -> Element {
    let (bg_class, text_class) = match status {
        crate::utils::nip34::BountyStatus::Pending => ("bg-yellow-500/20", "text-yellow-400"),
        crate::utils::nip34::BountyStatus::Claimed => ("bg-blue-500/20", "text-blue-400"),
        crate::utils::nip34::BountyStatus::Paid => ("bg-green-500/20", "text-green-400"),
    };
    let label = status.label();
    rsx! {
        span { class: "inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full {bg_class} {text_class} text-xs font-medium",
            "\u{26A1}"
            "{format_with_commas(amount)} sats"
            span { class: "opacity-70", "\u{2022}" }
            "{label}"
        }
    }
}
