use nostr::prelude::*;

pub fn cant_do_message(reason: &mostro_core::prelude::CantDoReason) -> String {
    use mostro_core::prelude::CantDoReason;
    match reason {
        CantDoReason::InvalidSignature => "Invalid signature. Try re-authenticating.".to_string(),
        CantDoReason::InvalidTradeIndex => "Trade index conflict. Try restoring your session.".to_string(),
        CantDoReason::InvalidAmount => "The amount is invalid.".to_string(),
        CantDoReason::InvalidInvoice => "The invoice is invalid or expired.".to_string(),
        CantDoReason::InvalidPaymentRequest => "The payment request is invalid.".to_string(),
        CantDoReason::InvalidPeer => "Invalid counterparty.".to_string(),
        CantDoReason::InvalidRating => "Rating must be between 1 and 5.".to_string(),
        CantDoReason::InvalidTextMessage => "Invalid message content.".to_string(),
        CantDoReason::InvalidOrderKind => "Invalid order type.".to_string(),
        CantDoReason::InvalidOrderStatus => "The order is in an unexpected state.".to_string(),
        CantDoReason::InvalidPubkey => "Invalid public key.".to_string(),
        CantDoReason::InvalidParameters => "Invalid parameters.".to_string(),
        CantDoReason::InvalidPayload => "Invalid message payload.".to_string(),
        CantDoReason::OrderAlreadyCanceled => "This order has already been canceled.".to_string(),
        CantDoReason::CantCreateUser => "Could not create your account on the daemon.".to_string(),
        CantDoReason::IsNotYourOrder => "This is not your order.".to_string(),
        CantDoReason::NotAllowedByStatus => "Action not allowed in the current state.".to_string(),
        CantDoReason::OutOfRangeFiatAmount => "The fiat amount is outside the allowed range.".to_string(),
        CantDoReason::OutOfRangeSatsAmount => "The sats amount is outside the allowed range.".to_string(),
        CantDoReason::IsNotYourDispute => "This dispute does not belong to you.".to_string(),
        CantDoReason::DisputeTakenByAdmin => "This dispute has already been taken by an admin.".to_string(),
        CantDoReason::NotAuthorized => "You are not authorized for this action.".to_string(),
        CantDoReason::DisputeCreationError => "Could not create a dispute.".to_string(),
        CantDoReason::NotFound => "Resource not found.".to_string(),
        CantDoReason::InvalidDisputeStatus => "Invalid dispute status.".to_string(),
        CantDoReason::InvalidAction => "Invalid action.".to_string(),
        CantDoReason::PendingOrderExists => "You already have a pending order.".to_string(),
        CantDoReason::InvalidFiatCurrency => "Unsupported fiat currency.".to_string(),
        CantDoReason::TooManyRequests => "Too many requests. Please wait.".to_string(),
    }
}

pub fn parse_node_pubkey(input: &str) -> Result<PublicKey, String> {
    if input.starts_with("npub1") {
        PublicKey::from_bech32(input).map_err(|e| format!("invalid npub: {e}"))
    } else {
        PublicKey::from_hex(input).map_err(|e| format!("invalid pubkey hex: {e}"))
    }
}
