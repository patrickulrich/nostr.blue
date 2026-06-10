use crate::utils::clipboard::copy_to_clipboard;
use dioxus::prelude::*;
use qrcode::render::svg;
use qrcode::QrCode;

fn generate_qr_svg(data: &str) -> String {
    match QrCode::new(data) {
        Ok(code) => code
            .render::<svg::Color>()
            .min_dimensions(200, 200)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build(),
        Err(_) => "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\" viewBox=\"0 0 200 200\"><rect width=\"200\" height=\"200\" fill=\"#ffffff\"/><text x=\"100\" y=\"100\" text-anchor=\"middle\" fill=\"#666666\" font-size=\"14\">QR Error</text></svg>".to_string(),
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct HoldInvoicePanelProps {
    pub invoice: String,
    pub updated_at: Option<i64>,
    #[props(default = false)]
    pub is_bond: bool,
    #[props(default = None)]
    pub bond_payout_deadline: Option<i64>,
}

#[component]
pub fn HoldInvoicePanel(props: HoldInvoicePanelProps) -> Element {
    let mut copied = use_signal(|| false);
    let qr = generate_qr_svg(&props.invoice);

    let header = if props.is_bond {
        "Bond Invoice"
    } else {
        "Lightning Invoice"
    };

    let expiry_window_secs = crate::stores::social::mostro::MOSTRO_NODE_INFO
        .read()
        .as_ref()
        .and_then(|info| info.hold_invoice_expiration_window);

    let remaining = if props.is_bond {
        props.bond_payout_deadline.map(|deadline| {
            let now = crate::platform::timestamp::now_secs() as i64;
            (deadline - now).max(0)
        })
    } else {
        match (props.updated_at, expiry_window_secs) {
            (Some(updated), Some(window)) => {
                let now = crate::platform::timestamp::now_secs() as i64;
                let deadline = updated + window as i64;
                Some((deadline - now).max(0))
            }
            _ => None,
        }
    };

    let expiry_label = if props.is_bond {
        "Claim deadline"
    } else {
        "Time remaining to pay"
    };
    let expired_label = if props.is_bond {
        "Claim window has expired."
    } else {
        "Invoice may have expired. Contact the daemon admin if payment fails."
    };

    rsx! {
        div { class: "p-4 bg-card border border-border rounded-lg",
            h3 { class: "text-sm font-semibold mb-3", "{header}" }
            if let Some(secs) = remaining {
                if secs > 0 {
                    div { class: "mb-3 p-2 bg-amber-500/10 border border-amber-500/20 rounded-lg",
                        p { class: "text-xs text-amber-600 dark:text-amber-400",
                            "{expiry_label}: {format_countdown(secs)}"
                        }
                    }
                } else {
                    div { class: "mb-3 p-2 bg-red-500/10 border border-red-500/20 rounded-lg",
                        p { class: "text-xs text-red-500",
                            "{expired_label}"
                        }
                    }
                }
            }
            div { class: "flex flex-col items-center gap-3",
                div {
                    class: "bg-white rounded-lg p-2",
                    dangerous_inner_html: "{qr}",
                }
                div { class: "flex gap-2 w-full",
                    button {
                        class: "flex-1 px-3 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium",
                        onclick: {
                            let invoice = props.invoice.clone();
                            move |_| {
                                let inv = invoice.clone();
                                spawn(async move {
                                    let _ = crate::platform::lightning::open_lightning_invoice(&inv).await;
                                });
                            }
                        },
                        "Open in Wallet"
                    }
                    button {
                        class: "px-3 py-2 border border-border rounded-lg text-sm",
                        onclick: {
                            let invoice = props.invoice.clone();
                            move |_| {
                                let inv = invoice.clone();
                                spawn(async move {
                                    if copy_to_clipboard(&inv).await.is_ok() {
                                        copied.set(true);
                                    }
                                });
                            }
                        },
                        if *copied.read() { "Copied!" } else { "Copy" }
                    }
                }
                p { class: "text-xs text-muted-foreground text-center break-all mt-1",
                    {props.invoice.chars().take(40).collect::<String>()}
                    "..."
                }
            }
        }
    }
}

fn format_countdown(secs: i64) -> String {
    if secs <= 0 {
        return "expired".to_string();
    }
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}
