//! Mostro local notification dispatch.
//!
//! Phase 9: maps Mostro Actions to user-facing notification (title, body)
//! tuples and dispatches them via the platform-appropriate notification API.
//!
//! On web: uses the Web Notifications API (`Notification::new`) when
//! permission is granted. Works for background tabs (notification shows
//! at OS level even when the tab is not focused).
//!
//! On desktop: uses `notify-rust` for native desktop notifications.
//!
//! On mobile: delegates to the Android `NotificationManager` via JNI
//! (stub for now — Phase 10 push notifications cover the mobile case).
//!
//! Honors `p2p_settings::should_notify(action)` per-category toggles.

use crate::stores::mostro::trade_store::Trade;

/// Map a Mostro Action to a notification (title, body) tuple.
///
/// Returns `None` for actions that don't warrant a notification
/// (e.g., `NewOrder` echoes, `Rate`/`RateReceived` informational acks,
/// `CantDo` — those are surfaced via the toast queue instead).
///
/// Note: the caller is responsible for checking
/// `p2p_settings::should_notify(action)` per-category toggles before
/// calling this function. This keeps the mapper testable without a
/// Dioxus runtime.
///
/// Bug #7 fix: the `payload` parameter lets the `PaymentFailed` body
/// include the retry schedule (`payment_attempts` and
/// `payment_retries_interval`) so the user knows what to expect.
pub fn map_action_to_notification(
    action: mostro_core::prelude::Action,
    trade: &Trade,
    payload: Option<&mostro_core::prelude::Payload>,
) -> Option<(String, String)> {
    use mostro_core::prelude::{Action as A, Payload as P};

    let short_id = trade_short_id(&trade.order_id);
    let kind_label = if trade.kind.is_empty() { "" } else { &trade.kind };
    let fiat = if trade.fiat_amount.is_empty() {
        String::new()
    } else {
        format!(" {} {}", trade.fiat_amount, trade.fiat_code)
    };

    let (title, body) = match action {
        A::PayInvoice => (
            "Invoice to pay".into(),
            format!("Pay the hold invoice for your {kind_label} order{fiat} ({short_id})"),
        ),
        A::PayBondInvoice => (
            "Bond to pay".into(),
            format!("Pay the anti-abuse bond for your {kind_label} order ({short_id})"),
        ),
        A::AddInvoice => (
            "Invoice requested".into(),
            format!("Submit your payout invoice for the {kind_label} order ({short_id})"),
        ),
        A::AddBondInvoice => (
            "Bond payout requested".into(),
            format!("Submit a payout invoice to claim your bond share ({short_id})"),
        ),
        A::FiatSentOk => (
            "Fiat sent".into(),
            format!("The buyer marked fiat as sent for your {kind_label} order ({short_id})"),
        ),
        A::Released | A::HoldInvoicePaymentSettled => (
            "Sats released".into(),
            format!("The seller released the escrow for your {kind_label} order ({short_id})"),
        ),
        A::PurchaseCompleted => (
            "Trade completed".into(),
            format!("Your {kind_label} order is complete ({short_id})"),
        ),
        A::Canceled | A::HoldInvoicePaymentCanceled => (
            "Order canceled".into(),
            format!("Order {short_id} was canceled"),
        ),
        A::CooperativeCancelInitiatedByYou => (
            "Cancel requested".into(),
            format!("Waiting for counterparty to accept your cancel ({short_id})"),
        ),
        A::CooperativeCancelInitiatedByPeer => (
            "Cancel requested by peer".into(),
            format!("The counterparty wants to cancel order {short_id}"),
        ),
        A::CooperativeCancelAccepted => (
            "Cancel accepted".into(),
            format!("Order {short_id} was cooperatively canceled"),
        ),
        A::DisputeInitiatedByYou => (
            "Dispute opened".into(),
            format!("Dispute opened on order {short_id}"),
        ),
        A::DisputeInitiatedByPeer => (
            "Dispute opened by peer".into(),
            format!("Counterparty opened a dispute on order {short_id}"),
        ),
        A::AdminTookDispute => (
            "Solver assigned".into(),
            format!("A solver has been assigned to your dispute ({short_id})"),
        ),
        A::AdminCanceled => (
            "Admin canceled".into(),
            format!("An admin canceled order {short_id}"),
        ),
        A::AdminSettled => (
            "Admin settled".into(),
            format!("An admin settled order {short_id}. Payout is in progress."),
        ),
        A::PaymentFailed => {
            // Bug #7 fix: include the retry schedule from the payload so
            // the user knows how many attempts remain and when the next
            // retry will happen.
            let retry_info = match payload {
                Some(P::PaymentFailed(info)) => format!(
                    " Attempt {}, daemon retries in {}s.",
                    info.payment_attempts, info.payment_retries_interval
                ),
                _ => String::new(),
            };
            (
                "Payment failed".into(),
                format!(
                    "The payout for order {short_id} failed. A new invoice may be needed.{retry_info}"
                ),
            )
        }
        A::BuyerTookOrder => (
            "Order taken".into(),
            format!("A buyer took your {kind_label} order ({short_id})"),
        ),
        A::HoldInvoicePaymentAccepted => (
            "Payment accepted".into(),
            format!("Escrow payment accepted for order {short_id}"),
        ),
        A::WaitingSellerToPay => (
            "Waiting for seller".into(),
            format!("Waiting for the seller to pay the hold invoice ({short_id})"),
        ),
        A::WaitingBuyerInvoice => (
            "Waiting for buyer invoice".into(),
            format!("Waiting for the buyer to submit a payout invoice ({short_id})"),
        ),
        A::BondSlashed => (
            "Bond slashed".into(),
            format!("Your bond was slashed on order {short_id}"),
        ),
        A::BondInvoiceAccepted => (
            "Bond payout accepted".into(),
            format!("Your bond payout invoice was accepted ({short_id})"),
        ),
        A::BondPayoutCompleted => (
            "Bond payout received".into(),
            format!("Your bond payout was completed ({short_id})"),
        ),
        A::InvoiceUpdated => (
            "Invoice updated".into(),
            format!("The payout invoice was updated ({short_id})"),
        ),
        A::BuyerInvoiceAccepted => (
            "Invoice accepted".into(),
            format!("Your payout invoice was accepted ({short_id})"),
        ),
        A::Rate => (
            "Rate your counterparty".into(),
            format!("How was your trade? Rate order {short_id}"),
        ),
        A::RateReceived => (
            "Rating received".into(),
            format!("Your counterparty rated order {short_id}"),
        ),
        A::SendDm => (
            "New message".into(),
            format!("New chat message on order {short_id}"),
        ),
        A::TradePubkey => (
            "Counterparty revealed".into(),
            format!("The counterparty's trade pubkey was disclosed ({short_id})"),
        ),
        // Actions that don't warrant a notification:
        A::NewOrder
        | A::TakeSell
        | A::TakeBuy
        | A::FiatSent
        | A::Release
        | A::Cancel
        | A::Dispute
        | A::RateUser
        | A::RestoreSession
        | A::LastTradeIndex
        | A::Orders
        | A::CantDo
        | A::AdminTakeDispute
        | A::AdminAddSolver
        | A::AdminCancel
        | A::AdminSettle => return None,
        #[allow(unreachable_patterns)]
        _ => return None,
    };
    Some((title, body))
}

/// Truncate an order ID to a readable short form.
fn trade_short_id(order_id: &str) -> String {
    if order_id.len() <= 12 {
        order_id.to_string()
    } else {
        format!("{}…{}", &order_id[..8], &order_id[order_id.len() - 4..])
    }
}

/// Build a structured [`MostroNotification`] for the given action, suitable
/// for pushing into [`notification_store::NOTIFICATIONS`]. Mirrors
/// [`map_action_to_notification`] but captures the order/dispute context
/// so the notifications screen can deep-link the user to the right place.
///
/// Returns `None` for actions that don't warrant a persisted notification
/// (same allow-list as `map_action_to_notification`).
#[allow(dead_code)]
pub fn build_notification(
    action: mostro_core::prelude::Action,
    trade: &Trade,
    payload: Option<&mostro_core::prelude::Payload>,
) -> Option<super::notification_store::MostroNotification> {
    let action_str = format!("{action:?}");
    let (title, body) = map_action_to_notification(action, trade, payload)?;
    // Normalize the Debug form to kebab-case to match mostro-core convention
    // (e.g. "PayInvoice" -> "pay-invoice"). Mirrors mostro-core's
    // `#[serde(rename_all = "kebab-case")]` on Action.
    let action_str = to_kebab_case(&action_str);

    let dispute_id = trade.dispute_id.clone();
    let now = crate::platform::timestamp::now_secs() as i64;
    // Stable id: action + order_id + created_at-second so multiple distinct
    // events on the same trade get distinct notifications, but a duplicate
    // delivery (e.g. backfill + live sub) collapses to the same id.
    let id = format!("{action_str}-{}-{now}", trade.order_id);
    Some(super::notification_store::MostroNotification {
        id,
        order_id: Some(trade.order_id.clone()),
        dispute_id,
        daemon_pubkey: trade.daemon_pubkey.clone(),
        action_str,
        title,
        body,
        created_at: now,
        read_at: None,
    })
}

/// Build a chat-message notification (P2P or dispute). Used by the chat
/// receive handlers when the chat panel isn't focused.
#[allow(dead_code)]
pub fn build_chat_notification(
    is_dispute: bool,
    order_id: &str,
    dispute_id: Option<&str>,
    daemon_pubkey: &str,
    preview: &str,
) -> super::notification_store::MostroNotification {
    let action_str = if is_dispute {
        "dispute-chat-message"
    } else {
        "chat-message"
    };
    let title = if is_dispute {
        "Dispute message".to_string()
    } else {
        "Trade chat message".to_string()
    };
    let body = if preview.is_empty() {
        format!("New message on order {}", trade_short_id(order_id))
    } else {
        // Truncate preview to keep notifications list scannable.
        let max = 120;
        let preview = if preview.chars().count() > max {
            let cutoff: String = preview.chars().take(max).collect();
            format!("{cutoff}…")
        } else {
            preview.to_string()
        };
        format!("{}: {}", trade_short_id(order_id), preview)
    };
    let now = crate::platform::timestamp::now_secs() as i64;
    super::notification_store::MostroNotification {
        id: format!("{action_str}-{order_id}-{now}"),
        order_id: Some(order_id.to_string()),
        dispute_id: dispute_id.map(|s| s.to_string()),
        daemon_pubkey: daemon_pubkey.to_string(),
        action_str: action_str.to_string(),
        title,
        body,
        created_at: now,
        read_at: None,
    }
}

/// Convert a PascalCase / Debug enum name to kebab-case.
fn to_kebab_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && ch.is_uppercase() {
            out.push('-');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

/// Dispatch a local notification. Platform-specific:
///
/// - **Web**: uses `web_sys::Notification` (Web Notifications API).
///   Requires `Notification.permission == "granted"`.
/// - **Desktop**: uses `notify_rust` for native desktop notifications.
/// - **Mobile**: not implemented; trade events still queue via
///   `MOSTRO_BACKGROUND_TOASTS` and surface on app resume.
pub fn show_notification(title: &str, body: &str) {
    #[cfg(feature = "web")]
    {
        show_notification_web(title, body);
    }
    #[cfg(all(feature = "native", not(feature = "mobile_platform")))]
    {
        show_notification_desktop(title, body);
    }
    #[cfg(feature = "mobile_platform")]
    {
        // Mobile local notifications are not yet implemented. Trade
        // events still queue via `MOSTRO_BACKGROUND_TOASTS` and are
        // drained by `mostro_toast_drainer` when the app returns to
        // the foreground. Tracked separately from this module.
        log::info!("Mostro notification (mobile, queued off): {title} — {body}");
    }
}

#[cfg(feature = "web")]
fn show_notification_web(title: &str, body: &str) {
    use wasm_bindgen::JsCast;
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };

    // Access the global Notification constructor via Reflect.
    let notification_ctor = match js_sys::Reflect::get(&window, &"Notification".into()) {
        Ok(n) if !n.is_undefined() => n,
        _ => return,
    };

    // Check permission via the static Notification.permission property.
    let permission = js_sys::Reflect::get(&notification_ctor, &"permission".into())
        .ok()
        .and_then(|p| p.as_string());
    if permission.as_deref() != Some("granted") {
        return;
    }

    // Build options object { body: "..." }.
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &"body".into(), &wasm_bindgen::JsValue::from_str(body));

    // Call `new Notification(title, opts)` via Reflect::construct.
    let ctor_fn = match notification_ctor.dyn_into::<js_sys::Function>() {
        Ok(f) => f,
        Err(_) => return,
    };
    let title_val = wasm_bindgen::JsValue::from_str(title);
    let args = js_sys::Array::new();
    args.push(&title_val);
    args.push(&opts);
    let _ = js_sys::Reflect::construct(&ctor_fn, &args);
}

#[cfg(all(feature = "native", not(feature = "mobile_platform")))]
fn show_notification_desktop(title: &str, body: &str) {
    // notify-rust is feature-gated to the `desktop` feature. The
    // `cfg` on this function guarantees the dependency is present
    // whenever this code compiles. If the desktop feature isn't
    // enabled (e.g. mobile-only build), this function is absent and
    // the caller in `show_notification` falls through to the mobile
    // or web arm.
    use notify_rust::Notification;
    if let Err(e) = Notification::new()
        .appname("nostr.blue")
        .summary(title)
        .body(body)
        .show()
    {
        log::warn!("Mostro desktop notification failed: {e}");
    }
}

/// Phase 9.7: request notification permission from the user.
///
/// On web: calls `Notification.requestPermission()` via JS interop.
/// On desktop/mobile: no permission needed (or handled by OS).
pub fn request_permission() {
    #[cfg(feature = "web")]
    {
        use wasm_bindgen::JsCast;
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let notification_ctor = match js_sys::Reflect::get(&window, &"Notification".into()) {
            Ok(n) if !n.is_undefined() => n,
            _ => return,
        };
        let request_fn = js_sys::Reflect::get(&notification_ctor, &"requestPermission".into())
            .ok()
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
        if let Some(req) = request_fn {
            let _ = req.call0(&notification_ctor);
        }
    }
    #[cfg(not(feature = "web"))]
    {
        // No permission needed on desktop/mobile.
    }
}

/// Phase 9.7: check whether notification permission is granted.
pub fn has_notification_permission() -> bool {
    #[cfg(feature = "web")]
    {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return false,
        };
        let notification_ctor = match js_sys::Reflect::get(&window, &"Notification".into()) {
            Ok(n) if !n.is_undefined() => n,
            _ => return false,
        };
        let permission = js_sys::Reflect::get(&notification_ctor, &"permission".into())
            .ok()
            .and_then(|p| p.as_string());
        permission.as_deref() == Some("granted")
    }
    #[cfg(not(feature = "web"))]
    {
        true // Always allowed on desktop/mobile
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::mostro::trade_store::tests::default_test_trade;

    #[test]
    fn test_pay_invoice_maps_to_notification() {
        let trade = default_test_trade(crate::stores::mostro::trade_store::TradeStatus::Active);
        let result = map_action_to_notification(
            mostro_core::prelude::Action::PayInvoice,
            &trade,
            None,
        );
        assert!(result.is_some());
        let (title, body) = result.unwrap();
        assert!(title.contains("Invoice"));
        assert!(body.contains("hold invoice"));
    }

    #[test]
    fn test_released_maps_to_notification() {
        let trade = default_test_trade(crate::stores::mostro::trade_store::TradeStatus::FiatSent);
        let result = map_action_to_notification(
            mostro_core::prelude::Action::Released,
            &trade,
            None,
        );
        assert!(result.is_some());
        let (title, _) = result.unwrap();
        assert!(title.contains("released"));
    }

    #[test]
    fn test_new_order_does_not_notify() {
        let trade = default_test_trade(crate::stores::mostro::trade_store::TradeStatus::Pending);
        let result = map_action_to_notification(
            mostro_core::prelude::Action::NewOrder,
            &trade,
            None,
        );
        assert!(result.is_none(), "NewOrder should not produce a notification");
    }

    #[test]
    fn test_cant_do_does_not_notify() {
        let trade = default_test_trade(crate::stores::mostro::trade_store::TradeStatus::Active);
        let result = map_action_to_notification(
            mostro_core::prelude::Action::CantDo,
            &trade,
            None,
        );
        assert!(result.is_none(), "CantDo should not produce a notification (toasts handle it)");
    }

    #[test]
    fn test_bond_slashed_maps_to_notification() {
        let trade = default_test_trade(crate::stores::mostro::trade_store::TradeStatus::Dispute);
        let result = map_action_to_notification(
            mostro_core::prelude::Action::BondSlashed,
            &trade,
            None,
        );
        assert!(result.is_some());
        let (title, _) = result.unwrap();
        assert!(title.contains("Bond slashed"));
    }

    /// Bug #7 regression test: PaymentFailed notification must include
    /// the retry schedule when the payload carries PaymentFailedInfo.
    #[test]
    fn test_payment_failed_includes_retry_schedule() {
        let trade = default_test_trade(crate::stores::mostro::trade_store::TradeStatus::Settled);
        let info = mostro_core::prelude::PaymentFailedInfo {
            payment_attempts: 3,
            payment_retries_interval: 120,
        };
        let payload = mostro_core::prelude::Payload::PaymentFailed(info);
        let result = map_action_to_notification(
            mostro_core::prelude::Action::PaymentFailed,
            &trade,
            Some(&payload),
        );
        assert!(result.is_some());
        let (_title, body) = result.unwrap();
        assert!(
            body.contains("Attempt 3"),
            "body must include payment_attempts; got: {body}"
        );
        assert!(
            body.contains("120s"),
            "body must include retries_interval; got: {body}"
        );
    }

    #[test]
    fn test_trade_short_id() {
        assert_eq!(trade_short_id("123e4567-e89b-12d3-a456-426614174000"), "123e4567…4000");
        assert_eq!(trade_short_id("short"), "short");
        assert_eq!(trade_short_id("123456789012"), "123456789012");
        assert_eq!(trade_short_id("1234567890123"), "12345678…0123");
    }
}
