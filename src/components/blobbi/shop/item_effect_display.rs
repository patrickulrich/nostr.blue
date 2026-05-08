use dioxus::prelude::*;

use crate::components::blobbi::shop::shop_items::ShopItem;

const STAT_ORDER: &[&str] = &["hunger", "happiness", "energy", "hygiene", "health"];

fn stat_icon(stat: &str) -> &'static str {
    match stat {
        "hunger" => "\u{1F354}",
        "happiness" => "\u{1F60A}",
        "energy" => "\u{26A1}",
        "hygiene" => "\u{2728}",
        "health" => "\u{2764}\u{FE0F}",
        _ => "?",
    }
}

fn stat_label(stat: &str) -> &'static str {
    match stat {
        "hunger" => "Hunger",
        "happiness" => "Happy",
        "energy" => "Energy",
        "hygiene" => "Hygiene",
        "health" => "Health",
        _ => "???",
    }
}

fn format_delta(delta: f64) -> String {
    let v = delta.round() as i32;
    if v >= 0 {
        format!("+{}", v)
    } else {
        format!("{}", v)
    }
}

fn ordered_stat_changes(item: &ShopItem) -> Vec<(&str, f64)> {
    let mut changes: Vec<(&str, f64)> = item
        .stat_changes
        .iter()
        .filter(|(s, _)| STAT_ORDER.contains(s))
        .cloned()
        .collect();
    changes.sort_by_key(|(s, _)| {
        STAT_ORDER.iter().position(|&o| o == *s).unwrap_or(999)
    });
    changes
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EffectDisplayMode {
    #[default]
    Inline,
    Badges,
    Grid,
}

#[component]
pub fn EffectDisplay(item: ShopItem, #[props(default)] mode: EffectDisplayMode, #[props(default = 0)] max_effects: usize) -> Element {
    let changes = ordered_stat_changes(&item);
    if changes.is_empty() {
        return rsx! {
            span { class: "text-[10px] text-muted-foreground italic", "No stat effects" }
        };
    }

    let (shown, extra) = if max_effects > 0 && changes.len() > max_effects {
        (&changes[..max_effects], changes.len() - max_effects)
    } else {
        (changes.as_slice(), 0)
    };

    match mode {
        EffectDisplayMode::Inline => rsx! {
            span { class: "text-[10px] text-muted-foreground",
                for (i, (stat, delta)) in shown.iter().enumerate() {
                    {if i > 0 { ", " } else { "" }}
                    span {
                        class: if *delta >= 0.0 { "text-green-500" } else { "text-red-400" },
                        "{stat_label(stat)}{format_delta(*delta)}"
                    }
                }
                if extra > 0 {
                    span { class: "text-muted-foreground", " +{extra} more" }
                }
            }
        },
        EffectDisplayMode::Badges => rsx! {
            div { class: "flex flex-wrap gap-1",
                for (stat, delta) in shown {
                    {
                        let icon = stat_icon(stat);
                        let text = format_delta(*delta);
                        let class = if *delta >= 0.0 {
                            "text-[10px] px-1.5 py-0.5 rounded-full bg-green-500/10 text-green-500"
                        } else {
                            "text-[10px] px-1.5 py-0.5 rounded-full bg-red-500/10 text-red-400"
                        };
                        rsx! {
                            span {
                                class: "{class}",
                                "{icon} {text}"
                            }
                        }
                    }
                }
                if extra > 0 {
                    span { class: "text-[10px] px-1.5 py-0.5 rounded-full bg-muted text-muted-foreground",
                        "+{extra} more"
                    }
                }
            }
        },
        EffectDisplayMode::Grid => rsx! {
            div { class: "grid grid-cols-2 gap-1",
                for (stat, delta) in shown {
                    {
                        let icon = stat_icon(stat);
                        let label = stat_label(stat);
                        let text = format_delta(*delta);
                        let class = if *delta >= 0.0 { "text-green-500" } else { "text-red-400" };
                        rsx! {
                            div {
                                class: "flex items-center gap-1 text-[10px]",
                                span { "{icon}" }
                                span { class: "{class}", "{label}{text}" }
                            }
                        }
                    }
                }
                if extra > 0 {
                    span { class: "text-[10px] text-muted-foreground col-span-2",
                        "+{extra} more effects"
                    }
                }
            }
        },
    }
}
