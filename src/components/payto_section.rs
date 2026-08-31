//! NIP-A3 payment-target UI: profile chips and the generic payment panel.
//!
//! `PayToChips` renders one pill chip per declared `payto` target (plus a
//! lightning chip when the profile has a kind-0 lightning address). Chip
//! click routes lightning-family targets through the zap flow and everything
//! else to the platform URI opener; long-press (touch) / right-click
//! (desktop) copies the address.
//!
//! `PayToTargetPanel` is the full payment surface used inside the zap modal:
//! a QR of the preferred URI (scannable by wallet apps), a copyable address,
//! and an "Open in …" handoff button.
use crate::components::icons::{BitcoinIcon, QrCodeIcon, WalletLineIcon, ZapIcon};
use crate::hooks::use_long_press;
use crate::hooks::use_payto_targets;use crate::platform::payto::open_payment_uri;
use crate::utils::nips::nipa3::{
    label_for, render_kind_for, short_address, uri_for, PayToTarget, RenderKind,
};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions, Toasts};
use qrcode::render::svg;
use qrcode::QrCode;
use std::time::Duration;

async fn copy_address(address: String, label: String, toast: Toasts) {
    match crate::platform::clipboard::copy_to_clipboard(&address).await {
        Ok(()) => toast.success(
            format!("Copied {label} address"),
            ToastOptions::new().duration(Duration::from_secs(2)),
        ),
        Err(e) => toast.error(
            "Copy failed".to_string(),
            ToastOptions::new().description(e).duration(Duration::from_secs(3)),
        ),
    }
}

fn open_target(target: PayToTarget, toast: Toasts) {
    let label = label_for(&target);
    let Some(uri) = uri_for(&target) else {
        // No usable scheme (e.g. silent-payment codes): copy instead.
        spawn(async move {
            copy_address(target.address, label, toast).await;
        });
        return;
    };
    spawn(async move {
        if let Err(e) = open_payment_uri(&uri).await {
            let description = if e == "no_handler" {
                format!("No payment app found for {label}")
            } else {
                e
            };
            toast.error(
                "Could not open payment".to_string(),
                ToastOptions::new().description(description).duration(Duration::from_secs(3)),
            );
        }
    });
}

/// Accent classes per canonical type; unknown types get the neutral chip.
fn chip_classes(payto_type: &str) -> &'static str {
    match payto_type {
        "bitcoin" => "text-orange-500 border-orange-500/40 bg-orange-500/10",
        "lightning" => "text-yellow-500 border-yellow-500/40 bg-yellow-500/10",
        "monero" => "text-orange-600 border-orange-600/40 bg-orange-600/10",
        "ethereum" => "text-indigo-400 border-indigo-400/40 bg-indigo-400/10",
        "litecoin" => "text-sky-400 border-sky-400/40 bg-sky-400/10",
        "zcash" => "text-amber-400 border-amber-400/40 bg-amber-400/10",
        "nano" => "text-teal-400 border-teal-400/40 bg-teal-400/10",
        "solana" => "text-purple-400 border-purple-400/40 bg-purple-400/10",
        "cashme" | "venmo" | "paypal" | "revolut" => {
            "text-blue-500 border-blue-500/40 bg-blue-500/10"
        }
        _ => "text-muted-foreground border-border bg-muted",
    }
}

/// Row of payment chips for a profile. Renders nothing when the user has
/// neither a lightning address nor declared targets.
#[component]
pub fn PayToChips(
    pubkey: String,
    /// Profile kind-0 lightning address, if any — surfaced as a zap chip.
    lud16: Option<String>,
    /// Invoked for lightning-family chips (opens the zap flow).
    #[props(default)]
    on_zap: Option<EventHandler<()>>,
) -> Element {
    let targets = use_payto_targets(&pubkey);

    rsx! {
        {
            let targets_now = targets.read().clone();
            let lightning_target = targets_now
                .iter()
                .find(|t| render_kind_for(t) == RenderKind::NativeLightning)
                .cloned();
            let show_lightning_chip = lud16.is_some() || lightning_target.is_some();
            if targets_now.is_empty() && !show_lightning_chip {
                return rsx! {};
            }
            rsx! {
                div { class: "flex flex-wrap gap-2 mt-3",
                    if show_lightning_chip {
                        PayToChip {
                            target: PayToTarget {
                                payto_type: "lightning".to_string(),
                                address: lud16
                                    .clone()
                                    .or_else(|| lightning_target.as_ref().map(|t| t.address.clone()))
                                    .unwrap_or_default(),
                            },
                            on_zap: on_zap,
                        }
                    }
                    for target in targets_now.iter().filter(|t| render_kind_for(t) != RenderKind::NativeLightning) {
                        PayToChip { target: target.clone(), on_zap: on_zap }
                    }
                }
            }
        }
    }
}

/// One payment chip: icon + label + short address. Click opens the payment
/// (zap flow for lightning, platform URI handoff otherwise); long-press or
/// right-click copies the address.
#[component]
fn PayToChip(
    target: PayToTarget,
    #[props(default)]
    on_zap: Option<EventHandler<()>>,
) -> Element {
    let toast = consume_toast();
    let label = label_for(&target);
    let ticker = match crate::utils::nips::nipa3::method_for(&target.payto_type) {
        Some(method) => method.ticker,
        None => "PAY",
    };
    let display = short_address(&target.address);
    let class = format!(
        "inline-flex items-center gap-1.5 px-3 py-1 rounded-full border text-xs font-semibold transition hover:brightness-110 {}",
        chip_classes(&target.payto_type)
    );
    let is_lightning = render_kind_for(&target) == RenderKind::NativeLightning;
    let title_address = target.address.clone();

    let click_target = target.clone();
    let click_toast = toast;
    let on_click = move |_| {
        if is_lightning {
            if let Some(handler) = on_zap.as_ref() {
                handler.call(());
                return;
            }
        }
        open_target(click_target.clone(), click_toast);
    };

    let long_press_address = target.address.clone();
    let long_press_label = label.clone();
    let (on_touch_start, on_touch_move, on_touch_end, on_touch_cancel) = use_long_press(
        Callback::new(move |_| {
            let address = long_press_address.clone();
            let label = long_press_label.clone();
            let toast = consume_toast();
            spawn(async move {
                copy_address(address, label, toast).await;
            });
        }),
        use_long_press::DEFAULT_LONG_PRESS_MS,
    );

    let context_address = target.address.clone();
    let context_label = label.clone();

    rsx! {
        button {
            class: "{class}",
            title: "{title_address}",
            aria_label: "Pay via {label}",
            r#type: "button",
            onclick: on_click,
            oncontextmenu: move |e: MouseEvent| {
                if cfg!(feature = "mobile_platform") {
                    return;
                }
                e.prevent_default();
                let address = context_address.clone();
                let label = context_label.clone();
                let toast = consume_toast();
                spawn(async move {
                    copy_address(address, label, toast).await;
                });
            },
            ontouchstart: on_touch_start,
            ontouchmove: on_touch_move,
            ontouchend: on_touch_end,
            ontouchcancel: on_touch_cancel,
            if target.payto_type == "bitcoin" {
                BitcoinIcon { class: "w-3.5 h-3.5".to_string() }
            } else if is_lightning {
                ZapIcon { class: "w-3.5 h-3.5".to_string() }
            } else {
                WalletLineIcon { class: "w-3.5 h-3.5".to_string() }
            }
            span { "{ticker}" }
            span { class: "font-mono opacity-70 max-w-[180px] truncate", "{display}" }
        }
    }
}

/// Full payment panel for a generic target: QR of the preferred URI (or the
/// bare address when no scheme exists), copyable address, and an open
/// handoff button.
#[component]
pub fn PayToTargetPanel(target: PayToTarget) -> Element {
    let toast = consume_toast();
    let label = label_for(&target);
    let uri = uri_for(&target);
    // QR encodes the preferred URI so wallet apps can act on a scan;
    // bare address for types without a scheme.
    let qr_value = uri.clone().unwrap_or_else(|| target.address.clone());
    let qr_svg = QrCode::new(qr_value.as_bytes()).ok().map(|code| {
        code.render::<svg::Color>()
            .min_dimensions(220, 220)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build()
    });

    let copy_address_value = target.address.clone();
    let copy_label = label.clone();
    let on_copy = move |_| {
        let address = copy_address_value.clone();
        let label = copy_label.clone();
        let toast = consume_toast();
        spawn(async move {
            copy_address(address, label, toast).await;
        });
    };

    let open_target_value = target.clone();
    let on_open = move |_| {
        open_target(open_target_value.clone(), toast);
    };

    rsx! {
        div { class: "space-y-4",
            div { class: "flex justify-center",
                if let Some(qr_string) = qr_svg {
                    div {
                        class: "p-3 bg-white rounded-xl",
                        dangerous_inner_html: "{qr_string}",
                    }
                } else {
                    div { class: "p-4 bg-muted rounded-xl text-sm text-muted-foreground",
                        "Address too long for a QR code"
                    }
                }
            }
            button {
                class: "w-full flex items-center justify-center gap-2 rounded-full border px-4 py-2 text-sm font-mono text-muted-foreground hover:bg-muted/50 transition",
                r#type: "button",
                title: "{target.address}",
                aria_label: "Copy {label} address",
                onclick: on_copy,
                QrCodeIcon { class: "w-3.5 h-3.5".to_string() }
                span { class: "truncate", "{target.address}" }
            }
            if uri.is_some() {
                button {
                    class: "w-full flex items-center justify-center gap-2 bg-primary text-primary-foreground px-4 py-2 rounded hover:bg-primary/90 transition",
                    r#type: "button",
                    onclick: on_open,
                    WalletLineIcon { class: "w-4 h-4".to_string() }
                    "Open in {label}"
                }
            } else {
                p { class: "text-xs text-muted-foreground text-center",
                    "Copy or scan the {label} address to pay"
                }
            }
        }
    }
}
