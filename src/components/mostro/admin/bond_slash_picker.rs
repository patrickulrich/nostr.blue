//! Bond slash picker modal for admin/solver dispute finalization.
//!
//! When a solver settles or cancels a dispute, they can optionally direct
//! the daemon to slash the losing party's anti-abuse bond. This modal
//! provides a 3-way choice: no slash, slash seller, or slash buyer.

use dioxus::prelude::*;

/// The slash decision chosen by the solver.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BondSlashChoice {
    None,
    SlashSeller,
    SlashBuyer,
    SlashBoth,
}

impl BondSlashChoice {
    /// Convert to the mostro-core `BondResolution` payload.
    #[allow(dead_code)]
    pub fn to_payload(self) -> Option<mostro_core::prelude::Payload> {
        match self {
            BondSlashChoice::None => None,
            BondSlashChoice::SlashSeller => Some(mostro_core::prelude::Payload::BondResolution(
                mostro_core::prelude::BondResolution {
                    slash_seller: true,
                    slash_buyer: false,
                },
            )),
            BondSlashChoice::SlashBuyer => Some(mostro_core::prelude::Payload::BondResolution(
                mostro_core::prelude::BondResolution {
                    slash_seller: false,
                    slash_buyer: true,
                },
            )),
            BondSlashChoice::SlashBoth => Some(mostro_core::prelude::Payload::BondResolution(
                mostro_core::prelude::BondResolution {
                    slash_seller: true,
                    slash_buyer: true,
                },
            )),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct BondSlashPickerProps {
    pub show: bool,
    pub is_settle: bool,
    pub on_confirm: EventHandler<(bool, bool)>,
    pub on_cancel: EventHandler<()>,
}

/// Modal that lets the solver choose whether to slash bonds before
/// finalizing a dispute.
#[component]
pub fn BondSlashPicker(props: BondSlashPickerProps) -> Element {
    let choice = use_signal(|| BondSlashChoice::None);

    if !props.show {
        return rsx! {};
    }

    let action_label = if props.is_settle { "Settle" } else { "Cancel" };
    let action_cls = if props.is_settle {
        "bg-green-600 text-white"
    } else {
        "bg-red-600 text-white"
    };

    rsx! {
        div { class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| props.on_cancel.call(()),
            div {
                class: "bg-card border border-border rounded-lg p-6 max-w-sm w-full mx-4 space-y-4",
                onclick: move |e| e.stop_propagation(),
                h3 { class: "text-lg font-semibold", "Bond Resolution" }
                p { class: "text-sm text-muted-foreground",
                    "Choose whether to slash the anti-abuse bonds before {action_label.to_lowercase()}ing this dispute."
                }
                div { class: "space-y-2",
                    { render_slash_option(
                        "No slash (release both bonds)",
                        BondSlashChoice::None,
                        choice,
                    )}
                    { render_slash_option(
                        "Slash seller's bond",
                        BondSlashChoice::SlashSeller,
                        choice,
                    )}
                    { render_slash_option(
                        "Slash buyer's bond",
                        BondSlashChoice::SlashBuyer,
                        choice,
                    )}
                    { render_slash_option(
                        "Slash both bonds",
                        BondSlashChoice::SlashBoth,
                        choice,
                    )}
                }
                {render_node_share_info(*choice.read())}
                div { class: "flex gap-2 pt-2",
                    button {
                        class: "flex-1 px-4 py-2 border border-border rounded-lg text-sm",
                        onclick: move |_| props.on_cancel.call(()),
                        "Back"
                    }
                    button {
                        class: "flex-1 px-4 py-2 {action_cls} rounded-lg text-sm font-medium",
                        onclick: move |_| {
                            let c = *choice.read();
                            let (ss, sb) = match c {
                                BondSlashChoice::None => (false, false),
                                BondSlashChoice::SlashSeller => (true, false),
                                BondSlashChoice::SlashBuyer => (false, true),
                                BondSlashChoice::SlashBoth => (true, true),
                            };
                            props.on_confirm.call((ss, sb));
                        },
                        "{action_label}"
                    }
                }
            }
        }
    }
}

fn render_slash_option(
    label: &str,
    value: BondSlashChoice,
    mut choice: Signal<BondSlashChoice>,
) -> Element {
    let is_selected = *choice.read() == value;
    let cls = if is_selected {
        "w-full p-3 border-2 border-primary rounded-lg text-sm font-medium bg-primary/10"
    } else {
        "w-full p-3 border border-border rounded-lg text-sm hover:bg-accent transition"
    };
    let label = label.to_string();
    rsx! {
        button {
            class: "{cls}",
            onclick: move |_| choice.set(value),
            "{label}"
        }
    }
}

/// Show the node's share of the slashed bond (if configured) when a
/// slash option is selected. Helps the solver understand the economic
/// implications: the daemon keeps `bond_slash_node_share_pct` of the
/// slashed amount, and the rest goes to the winning counterparty.
fn render_node_share_info(choice: BondSlashChoice) -> Element {
    if choice == BondSlashChoice::None {
        return rsx! {};
    }
    let node_share_pct = crate::stores::mostro::MOSTRO_NODE_CONFIG
        .read()
        .as_ref()
        .and_then(|c| c.bond_slash_node_share_pct);
    if let Some(pct) = node_share_pct {
        let display = format!("{:.1}", pct * 100.0);
        rsx! {
            p {
                class: "text-xs text-muted-foreground",
                "The node keeps {display}% of the slashed bond; the remainder goes to the winning counterparty."
            }
        }
    } else {
        rsx! {}
    }
}
