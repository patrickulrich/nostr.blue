//! App-shell modal for BUD-07 Blossom payment challenges (402).
//!
//! Rendered from the Layout whenever [`BLOSSOM_PAYMENT_PROMPT`] is set. The
//! pending upload/download task is waiting on the proof map — as soon as a
//! payment succeeds here, the proof is submitted and the blocked operation
//! retries automatically with it.

use dioxus::prelude::*;

use crate::stores::media::blossom_payment::{
    self, dismiss_prompt, parse_lightning_request, submit_proof, LightningDetails, PaymentPrompt,
    BLOSSOM_PAYMENT_PROMPT,
};

/// Which payment method the modal can actually execute for this challenge.
enum PaymentUi {
    Lightning(LightningDetails),
    #[cfg(feature = "cashu")]
    Cashu(blossom_payment::CashuDetails),
    Unsupported,
}

#[component]
pub fn BlossomPaymentModal() -> Element {
    let prompt = BLOSSOM_PAYMENT_PROMPT.read().clone();
    match prompt {
        None => rsx! {},
        Some(prompt) => rsx! { PaymentModalInner { prompt } },
    }
}

#[component]
fn PaymentModalInner(prompt: PaymentPrompt) -> Element {
    let mut status = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let paying = use_signal(|| false);
    let server = prompt.server.clone();
    let sha = prompt.sha256.clone();

    let lightning = prompt.lightning_request().and_then(|r| {
        parse_lightning_request(&r.request)
            .map_err(|e| {
                log::warn!("Unusable lightning challenge: {e}");
                e
            })
            .ok()
    });
    let ui = if let Some(details) = lightning {
        PaymentUi::Lightning(details)
    } else if let Some(request) = prompt.cashu_request() {
        #[cfg(feature = "cashu")]
        {
            match blossom_payment::decode_cashu_request(&request.request) {
                Ok(details) => PaymentUi::Cashu(details),
                Err(e) => {
                    log::warn!("Unusable cashu challenge: {e}");
                    PaymentUi::Unsupported
                }
            }
        }
        #[cfg(not(feature = "cashu"))]
        {
            let _ = request;
            PaymentUi::Unsupported
        }
    } else {
        PaymentUi::Unsupported
    };

    let mut close = move || {
        dismiss_prompt();
        status.set(None);
        error.set(None);
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4",
            onclick: move |_| { close() },
            div {
                class: "bg-card border border-border rounded-xl max-w-md w-full p-5 space-y-4",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div { class: "flex items-start justify-between gap-2",
                    h3 { class: "text-lg font-semibold", "Blossom server requires payment" }
                    button {
                        class: "text-muted-foreground hover:text-foreground",
                        onclick: move |_| { close() },
                        "✕"
                    }
                }
                if let Some(reason) = prompt.reason.as_deref() {
                    p { class: "text-sm text-muted-foreground", "{reason}" }
                }
                p { class: "text-xs text-muted-foreground break-all",
                    "Server: {server} · Blob: {sha}"
                }

                match ui {
                    PaymentUi::Lightning(details) => rsx! {
                        LightningPaymentSection {
                            details,
                            server: server.clone(),
                            sha: sha.clone(),
                            paying,
                            status,
                            error,
                        }
                    },
                    #[cfg(feature = "cashu")]
                    PaymentUi::Cashu(details) => rsx! {
                        CashuPaymentSection {
                            details,
                            server: server.clone(),
                            sha: sha.clone(),
                            paying,
                            status,
                            error,
                        }
                    },
                    PaymentUi::Unsupported => rsx! {
                        p { class: "text-sm text-red-500",
                            "This server requested payment with a method the client could not read or support."
                        }
                    },
                }

                div { class: "flex justify-end pt-1",
                    button {
                        class: "px-4 py-2 text-sm rounded-lg bg-accent hover:bg-muted transition",
                        onclick: move |_| { close() },
                        "Cancel"
                    }
                }
            }
        }
    }
}

#[component]
fn LightningPaymentSection(
    details: LightningDetails,
    server: String,
    sha: String,
    paying: Signal<bool>,
    status: Signal<Option<String>>,
    error: Signal<Option<String>>,
) -> Element {
    let invoice_for_pay = details.invoice.clone();
    let invoice_for_qr = details.invoice.clone();
    let invoice_for_copy = details.invoice.clone();
    let invoice_for_wallet = details.invoice.clone();
    let server_pay = server.clone();
    let sha_pay = sha.clone();

    rsx! {
        div { class: "space-y-3",
            div { class: "flex items-baseline justify-between",
                span { class: "text-2xl font-bold", "{amount_label(details.amount_sats)}" }
                if !details.memo.is_empty() {
                    span { class: "text-sm text-muted-foreground", "{details.memo}" }
                }
            }
            div {
                class: "flex justify-center bg-white rounded-lg p-2",
                dangerous_inner_html: "{generate_invoice_qr_svg(&invoice_for_qr)}",
            }
            if let Some(err) = error.read().as_ref() {
                p { class: "text-sm text-red-500", "{err}" }
            }
            if let Some(s) = status.read().as_ref() {
                p { class: "text-sm text-green-600", "{s}" }
            }
            div { class: "flex flex-wrap gap-2",
                button {
                    class: "px-4 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition disabled:opacity-50",
                    disabled: *paying.read(),
                    onclick: move |_| {
                        paying.set(true);
                        error.set(None);
                        let details_invoice = invoice_for_pay.clone();
                        let server = server_pay.clone();
                        let sha = sha_pay.clone();
                        spawn(async move {
                            match blossom_payment::pay_lightning_webln(&details_invoice).await {
                                Ok(proof) => {
                                    submit_proof(&server, &sha, proof);
                                }
                                Err(e) => {
                                    error.set(Some(format!("WebLN: {e}. Pay via QR/wallet instead.")));
                                }
                            }
                            paying.set(false);
                        });
                    },
                    if *paying.read() { "Paying…" } else { "Pay with WebLN" }
                }
                button {
                    class: "px-4 py-2 text-sm rounded-lg bg-accent hover:bg-muted transition",
                    onclick: move |_| {
                        let invoice = invoice_for_wallet.clone();
                        spawn(async move {
                            if let Err(e) = crate::platform::lightning::open_lightning_invoice(&invoice).await {
                                log::warn!("open wallet: {e}");
                            }
                        });
                    },
                    "Open wallet"
                }
                button {
                    class: "px-4 py-2 text-sm rounded-lg bg-accent hover:bg-muted transition",
                    onclick: move |_| {
                        let invoice = invoice_for_copy.clone();
                        spawn(async move {
                            match crate::platform::clipboard::copy_to_clipboard(&invoice).await {
                                Ok(()) => status.set(Some("Invoice copied".to_string())),
                                Err(e) => error.set(Some(format!("Copy failed: {e}"))),
                            }
                        });
                    },
                    "Copy invoice"
                }
            }
            p { class: "text-xs text-muted-foreground",
                "The blocked operation retries automatically once payment completes."
            }
        }
    }
}

#[cfg(feature = "cashu")]
#[component]
fn CashuPaymentSection(
    details: crate::stores::media::blossom_payment::CashuDetails,
    server: String,
    sha: String,
    paying: Signal<bool>,
    status: Signal<Option<String>>,
    error: Signal<Option<String>>,
) -> Element {
    let server_pay = server.clone();
    let sha_pay = sha.clone();
    rsx! {
        div { class: "space-y-3",
            div { class: "flex items-baseline justify-between",
                span { class: "text-2xl font-bold", "{amount_label(details.amount)}" }
                span { class: "text-sm text-muted-foreground", "ecash (cashu)" }
            }
            if let Some(desc) = details.description.as_deref() {
                p { class: "text-sm text-muted-foreground", "{desc}" }
            }
            p { class: "text-xs text-muted-foreground break-all",
                "Paying with tokens from: {details.mints.join(\", \")}"
            }
            if let Some(err) = error.read().as_ref() {
                p { class: "text-sm text-red-500", "{err}" }
            }
            if let Some(s) = status.read().as_ref() {
                p { class: "text-sm text-green-600", "{s}" }
            }
            button {
                class: "w-full px-4 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition disabled:opacity-50",
                disabled: *paying.read(),
                onclick: move |_| {
                    paying.set(true);
                    error.set(None);
                    let details = details.clone();
                    let server = server_pay.clone();
                    let sha = sha_pay.clone();
                    spawn(async move {
                        match blossom_payment::pay_with_cashu(&details).await {
                            Ok(proof) => submit_proof(&server, &sha, proof),
                            Err(e) => error.set(Some(e)),
                        }
                        paying.set(false);
                    });
                },
                if *paying.read() { "Paying…" } else { "Pay with Cashu wallet" }
            }
        }
    }
}

fn amount_label(sats: u64) -> String {
    format!("{sats} sats")
}

fn generate_invoice_qr_svg(invoice: &str) -> String {
    match qrcode::QrCode::new(invoice.trim().to_uppercase()) {
        Ok(code) => code
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(200, 200)
            .dark_color(qrcode::render::svg::Color("#000000"))
            .light_color(qrcode::render::svg::Color("#ffffff"))
            .build(),
        Err(_) => "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\" viewBox=\"0 0 200 200\"><rect width=\"200\" height=\"200\" fill=\"#ffffff\"/></svg>".to_string(),
    }
}
