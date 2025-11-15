/// Quick zap service for one-click lightning zaps
use nostr_sdk::{PublicKey, EventId, RelayUrl};
use crate::services::lnurl;
use crate::stores::{signer, nwc_store, settings_store, nostr_client};
use std::time::Duration;

/// Result of a quick zap attempt
#[derive(Debug, Clone)]
pub enum QuickZapResult {
    /// Zap was successful via the specified method
    Success { method: PaymentMethod },
    /// Zap failed with an error
    Error { message: String },
}

/// Payment method used for the zap
#[derive(Debug, Clone)]
pub enum PaymentMethod {
    WebLN,
    NostrWalletConnect,
}

impl PaymentMethod {
    pub fn as_str(&self) -> &str {
        match self {
            PaymentMethod::WebLN => "WebLN",
            PaymentMethod::NostrWalletConnect => "Nostr Wallet Connect",
        }
    }
}

/// Attempt a quick zap with default amount (21 sats)
pub async fn quick_zap(
    recipient_pubkey: String,
    lud16: Option<String>,
    lud06: Option<String>,
    event_id: Option<String>,
) -> QuickZapResult {
    quick_zap_with_amount(recipient_pubkey, lud16, lud06, event_id, 21, None).await
}

/// Attempt a quick zap with custom amount and optional message
pub async fn quick_zap_with_amount(
    recipient_pubkey: String,
    lud16: Option<String>,
    lud06: Option<String>,
    event_id: Option<String>,
    amount_sats: u64,
    message: Option<String>,
) -> QuickZapResult {
    // Get signer
    let signer_type = match signer::get_signer() {
        Some(s) => s,
        None => {
            return QuickZapResult::Error {
                message: "No signer available. Please connect a signer first.".to_string(),
            };
        }
    };

    // Parse recipient pubkey
    let recipient_pubkey = match PublicKey::parse(&recipient_pubkey) {
        Ok(pk) => pk,
        Err(e) => {
            return QuickZapResult::Error {
                message: format!("Invalid recipient pubkey: {}", e),
            };
        }
    };

    // Parse event ID if provided
    let event_id = if let Some(eid_str) = event_id {
        match EventId::parse(&eid_str) {
            Ok(eid) => Some(eid),
            Err(e) => {
                return QuickZapResult::Error {
                    message: format!("Invalid event ID: {}", e),
                };
            }
        }
    } else {
        None
    };

    // Get relays from client
    let relays = if let Some(client) = nostr_client::get_client() {
        client
            .relays()
            .await
            .into_iter()
            .map(|(url, _)| url)
            .take(5)
            .collect::<Vec<RelayUrl>>()
    } else {
        return QuickZapResult::Error {
            message: "No relays available".to_string(),
        };
    };

    // Prepare zap
    let (pay_info, amount_msats) = match lnurl::prepare_zap(
        lud16.as_deref(),
        lud06.as_deref(),
        amount_sats,
    )
    .await
    {
        Ok(info) => info,
        Err(e) => {
            return QuickZapResult::Error {
                message: format!("Failed to prepare zap: {}", e),
            };
        }
    };

    // Create zap request builder
    let builder = lnurl::create_zap_request_unsigned(
        recipient_pubkey,
        relays,
        amount_msats,
        message,
        event_id,
    );

    // Sign the zap request
    let zap_request = match signer_type {
        signer::SignerType::Keys(ref keys) => match builder.sign_with_keys(keys) {
            Ok(event) => event,
            Err(e) => {
                return QuickZapResult::Error {
                    message: format!("Failed to sign zap request: {}", e),
                };
            }
        },
        #[cfg(target_family = "wasm")]
        signer::SignerType::BrowserExtension(ref signer) => {
            #[allow(unused_imports)]
            use nostr::signer::NostrSigner;
            match builder.sign(signer.as_ref()).await {
                Ok(event) => event,
                Err(e) => {
                    return QuickZapResult::Error {
                        message: format!("Failed to sign zap request: {}", e),
                    };
                }
            }
        }
        signer::SignerType::NostrConnect(ref nostr_connect) => {
            #[allow(unused_imports)]
            use nostr::signer::NostrSigner;
            match builder.sign(nostr_connect.as_ref()).await {
                Ok(event) => event,
                Err(e) => {
                    return QuickZapResult::Error {
                        message: format!("Failed to sign zap request: {}", e),
                    };
                }
            }
        }
    };

    // Request invoice
    let lnurl_param = if lud16.is_some() {
        None
    } else {
        lud06.as_deref()
    };

    let invoice = match lnurl::request_zap_invoice(
        &pay_info.callback,
        amount_msats,
        &zap_request,
        lnurl_param,
    )
    .await
    {
        Ok(response) => response.pr,
        Err(e) => {
            return QuickZapResult::Error {
                message: format!("Failed to get invoice: {}", e),
            };
        }
    };

    // Try to pay automatically based on preferences
    let payment_preference = settings_store::SETTINGS.read().payment_method_preference.clone();
    let nwc_available = nwc_store::is_connected();
    let webln_available = is_webln_available();

    // Attempt payment based on preference
    match payment_preference.as_str() {
        "nwc_first" if nwc_available => {
            // Try NWC first
            if let Ok(_) = nwc_store::pay_invoice(invoice.clone()).await {
                return QuickZapResult::Success {
                    method: PaymentMethod::NostrWalletConnect,
                };
            }
            // Fall through to WebLN if NWC fails
            if webln_available {
                if let Ok(_) = attempt_webln_payment(&invoice).await {
                    return QuickZapResult::Success {
                        method: PaymentMethod::WebLN,
                    };
                }
            }
        }
        "webln_first" if webln_available => {
            // Try WebLN first
            if let Ok(_) = attempt_webln_payment(&invoice).await {
                return QuickZapResult::Success {
                    method: PaymentMethod::WebLN,
                };
            }
            // Fall through to NWC if WebLN fails
            if nwc_available {
                if let Ok(_) = nwc_store::pay_invoice(invoice.clone()).await {
                    return QuickZapResult::Success {
                        method: PaymentMethod::NostrWalletConnect,
                    };
                }
            }
        }
        _ => {
            // Default: try NWC if available, then WebLN
            if nwc_available {
                if let Ok(_) = nwc_store::pay_invoice(invoice.clone()).await {
                    return QuickZapResult::Success {
                        method: PaymentMethod::NostrWalletConnect,
                    };
                }
            }
            if webln_available {
                if let Ok(_) = attempt_webln_payment(&invoice).await {
                    return QuickZapResult::Success {
                        method: PaymentMethod::WebLN,
                    };
                }
            }
        }
    }

    // If we got here, auto-payment failed
    QuickZapResult::Error {
        message: "Payment failed. Please check your WebLN or NWC connection.".to_string(),
    }
}

/// Check if WebLN is available
fn is_webln_available() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;
        if let Some(window) = window() {
            return js_sys::Reflect::has(&window, &wasm_bindgen::JsValue::from_str("webln"))
                .unwrap_or(false);
        }
    }
    false
}

/// Attempt WebLN payment
async fn attempt_webln_payment(invoice: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_namespace = ["window", "webln"], js_name = enable, catch)]
            async fn webln_enable() -> Result<JsValue, JsValue>;

            #[wasm_bindgen(js_namespace = ["window", "webln"], js_name = sendPayment, catch)]
            async fn webln_send_payment(invoice: &str) -> Result<JsValue, JsValue>;
        }

        // Enable WebLN
        webln_enable()
            .await
            .map_err(|e| format!("WebLN enable failed: {:?}", e))?;

        // Send payment
        let result = webln_send_payment(invoice)
            .await
            .map_err(|e| format!("WebLN payment failed: {:?}", e))?;

        if result.is_null() || result.is_undefined() {
            return Err("WebLN returned null/undefined".to_string());
        }

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    Err("WebLN only available in browser".to_string())
}

/// Get available payment method for display
pub fn get_available_payment_method() -> Option<PaymentMethod> {
    let nwc_available = nwc_store::is_connected();
    let webln_available = is_webln_available();

    let preference = settings_store::SETTINGS.read().payment_method_preference.clone();

    match preference.as_str() {
        "nwc_first" if nwc_available => Some(PaymentMethod::NostrWalletConnect),
        "webln_first" if webln_available => Some(PaymentMethod::WebLN),
        _ => {
            if nwc_available {
                Some(PaymentMethod::NostrWalletConnect)
            } else if webln_available {
                Some(PaymentMethod::WebLN)
            } else {
                None
            }
        }
    }
}
