//! Admin/solver dispute detail page.
//!
//! `/p2p/admin/dispute/:dispute_id` — full solver workflow:
//! 1. Auto-sends AdminTakeDispute to claim the dispute.
//! 2. Receives AdminTookDispute with SolverDisputeInfo.
//! 3. Renders dispute info + two chat panels (buyer, seller).
//! 4. Action buttons: Settle / Cancel with optional bond slash.

use crate::components::mostro::DisputeChat;
use crate::components::mostro::dispute_chat::{DisputeChatMsg, is_dup_dispute_msg};
use crate::components::mostro::admin::bond_slash_picker::BondSlashPicker;
use crate::components::mostro::trade_chat::encode_chat_content;
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::mostro::encrypted_attachment::{
    attachment_key_from_shared_secret, encode_nonce, encrypt_attachment, AttachmentMeta,
};
use crate::stores::mostro::{
    admin_keys, client as mostro_client, dispute_store, flow,
    node_config, parse_node_pubkey,
};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use mostro_core::prelude;
use nostr::prelude::*;
use std::time::Duration;

use mostro_core::chat::SharedKey;

#[component]
pub fn MostroAdminDisputeDetail(dispute_id: String) -> Element {
    let nav = navigator();

    if !*crate::stores::nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! {
            div { class: "min-h-screen p-4 max-w-3xl mx-auto", ClientInitializing {} }
        };
    }

    let admin = match admin_keys::try_get() {
        Some(a) => a,
        None => {
            return rsx! {
                div { class: "min-h-screen p-4 max-w-3xl mx-auto flex items-center justify-center",
                    div { class: "text-center space-y-4",
                        h3 { class: "text-lg font-medium", "Admin keys not loaded" }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm",
                            onclick: move |_| { let _ = nav.push(Route::SettingsMostro {}); },
                            "Go to Settings"
                        }
                    }
                }
            };
        }
    };

    let mut solver_info: Signal<Option<prelude::SolverDisputeInfo>> = use_signal(|| None);
    let mut taking: Signal<bool> = use_signal(|| false);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut show_slash_picker: Signal<Option<bool>> = use_signal(|| None);
    let mut action_busy: Signal<bool> = use_signal(|| false);
    let mut buyer_chat: Signal<Vec<DisputeChatMsg>> = use_signal(Vec::new);
    let mut seller_chat: Signal<Vec<DisputeChatMsg>> = use_signal(Vec::new);

    let node = match node_config::try_get() {
        Some(n) => n,
        None => return rsx! { div { class: "p-8 text-center text-muted-foreground", "Daemon not configured." } },
    };
    let node_pk = match parse_node_pubkey(&node.pubkey) {
        Ok(p) => p,
        Err(e) => return rsx! { div { class: "p-8 text-center text-red-500", "{e}" } },
    };

    // Clone data out of signals for derivation (avoids lifetime issues).
    let info_snapshot = solver_info.read().clone();
    let admin_pubkey_hex = admin.keys.public_key().to_hex();
    let admin_secret = admin.keys.secret_key().clone();

    let buyer_shared = info_snapshot.as_ref()
        .and_then(|s| s.buyer_pubkey.as_ref())
        .and_then(|pk| PublicKey::from_hex(pk).ok())
        .and_then(|cp| SharedKey::derive(&admin_secret, &cp).ok());

    let seller_shared = info_snapshot.as_ref()
        .and_then(|s| s.seller_pubkey.as_ref())
        .and_then(|pk| PublicKey::from_hex(pk).ok())
        .and_then(|cp| SharedKey::derive(&admin_secret, &cp).ok());

    let buyer_shared_for_sub = buyer_shared.clone();
    let seller_shared_for_sub = seller_shared.clone();
    let admin_pk_for_sub = admin_pubkey_hex.clone();
    let admin_keys_for_sub = admin.keys.clone();
    let node_for_take = node.clone();
    let did_for_take = dispute_id.clone();

    // AdminTakeDispute: auto-send once.
    {
        let admin_keys_for_future = admin_keys_for_sub.clone();
        use_future(move || {
            let did = did_for_take.clone();
            let keys = admin_keys_for_future.clone();
            let node = node_for_take.clone();
            let node_pk = node_pk;
            async move {
                if solver_info.read().is_some() || *taking.read() {
                    return;
                }
                if let Some(existing) = dispute_store::find_by_id(&did) {
                    if existing.status != dispute_store::DisputeStatus::Initiated {
                        return;
                    }
                }
                *taking.write() = true;
                let dispute_uuid = match uuid::Uuid::parse_str(&did) {
                    Ok(u) => u,
                    Err(e) => {
                        *error.write() = Some(format!("Invalid dispute ID: {e}"));
                        *taking.write() = false;
                        return;
                    }
                };
                let message = prelude::Message::new_dispute(
                    Some(dispute_uuid),
                    Some(flow::next_request_id()),
                    None,
                    prelude::Action::AdminTakeDispute,
                    None,
                );
                let pow = mostro_client::resolve_effective_pow(&node, node_pk).await;
                if let Err(e) = mostro_client::send_mostro_message(
                    &message, &keys, &keys, node_pk, &node.relays, pow,
                ).await {
                    *error.write() = Some(format!("Failed to take dispute: {e}"));
                }
                *taking.write() = false;
            }
        });
    }

    // Live subscription for admin responses (AdminTookDispute, AdminSettled, etc.)
    {
        let filter = Filter::new()
            .kind(Kind::GiftWrap)
            .custom_tags(
                nostr_sdk::prelude::SingleLetterTag::lowercase(nostr_sdk::prelude::Alphabet::P),
                [admin_pk_for_sub.clone()],
            )
            .limit(0);
        let keys_for_cb = admin_keys_for_sub.clone();
        crate::hooks::use_relay_subscription(
            Some(filter),
            move |event: &nostr_sdk::Event| {
                let event = event.clone();
                let keys = keys_for_cb.clone();
                spawn(async move {
                    if crate::stores::mostro::dedup::is_seen(&event.id) { return; }
                    crate::stores::mostro::dedup::mark_seen(event.id);
                    if let Ok(Some(u)) = mostro_client::unwrap_mostro_response(&event, &keys).await {
                        let action = u.message.inner_action().unwrap_or(prelude::Action::CantDo);
                        let payload = u.message.get_inner_message_kind().payload.clone();
                        match action {
                            prelude::Action::AdminTookDispute => {
                                if let Some(prelude::Payload::Dispute(_, Some(info))) = payload {
                                    *solver_info.write() = Some(info);
                                    *taking.write() = false;
                                    let toast = consume_toast();
                                    toast.info("Dispute taken".into(),
                                        ToastOptions::new().duration(Duration::from_secs(3)));
                                }
                            }
                            prelude::Action::AdminSettled | prelude::Action::AdminCanceled => {
                                let label = if action == prelude::Action::AdminSettled { "Settled" } else { "Canceled" };
                                let toast = consume_toast();
                                toast.info(format!("Dispute {label}"),
                                    ToastOptions::new().duration(Duration::from_secs(5)));
                                *action_busy.write() = false;
                            }
                            // Bug #3 fix: after AdminSettled, the daemon's
                            // `do_payment` step may fail to pay the buyer's
                            // invoice, emitting `PaymentFailed`. Without this
                            // listener, the solver sees "Settled" and clears
                            // `action_busy`, never learning the payout failed.
                            // The status transition itself is handled by
                            // `apply_mostro_action` (AdminSettled → Settled
                            // non-terminal, then PaymentFailed → PaymentFailed).
                            prelude::Action::PaymentFailed => {
                                if let Some(prelude::Payload::PaymentFailed(info)) = &payload {
                                    let toast = consume_toast();
                                    toast.warning(
                                        "Payout failed".into(),
                                        ToastOptions::new()
                                            .description(format!(
                                                "Buyer invoice payment failed (attempt {}). \
                                                 Daemon will retry in {}s.",
                                                info.payment_attempts,
                                                info.payment_retries_interval
                                            ))
                                            .duration(Duration::from_secs(8)),
                                    );
                                }
                            }
                            prelude::Action::CantDo => {
                                if let Some(prelude::Payload::CantDo(Some(reason))) = payload {
                                    *error.write() = Some(
                                        crate::stores::mostro::cant_do_message(&reason));
                                }
                                *action_busy.write() = false;
                            }
                            _ => {}
                        }
                    }
                });
            },
        );
    }

    // Buyer chat subscription — hook must be called unconditionally
    // (Dioxus hooks cannot be inside `if` blocks). When the shared key
    // is not yet available, the filter is `None` and the hook skips
    // subscribing. Once solver info arrives and the component re-renders,
    // the filter becomes `Some` and the hook re-subscribes.
    let buyer_chat_filter = buyer_shared_for_sub
        .as_ref()
        .map(|bs| mostro_core::chat::chat_filter(bs.public_key()));
    let buyer_sk = buyer_shared_for_sub.clone();
    let buyer_my_pk = admin_pubkey_hex.clone();
    crate::hooks::use_relay_subscription(
        buyer_chat_filter,
        move |event: &nostr_sdk::Event| {
            let event = event.clone();
            let sk = match buyer_sk.clone() {
                Some(s) => s,
                None => return,
            };
            let my_pk = buyer_my_pk.clone();
            spawn(async move {
                if crate::stores::mostro::dedup::is_seen(&event.id) { return; }
                crate::stores::mostro::dedup::mark_seen(event.id);
                if let Ok(msg) = mostro_core::chat::unwrap_chat_message(sk.keys(), &event).await {
                    let is_me = msg.sender.to_hex() == my_pk;
                    let cm = DisputeChatMsg {
                        content: msg.content, sender_hex: msg.sender.to_hex(),
                        is_me, timestamp: msg.created_at.as_secs() as i64, attachment: None,
                    };
                    if !is_dup_dispute_msg(&buyer_chat.read(), &cm) {
                        buyer_chat.write().push(cm);
                    }
                }
            });
        },
    );

    // Seller chat subscription — same unconditional hook pattern.
    let seller_chat_filter = seller_shared_for_sub
        .as_ref()
        .map(|ss| mostro_core::chat::chat_filter(ss.public_key()));
    let seller_sk = seller_shared_for_sub.clone();
    let seller_my_pk = admin_pubkey_hex.clone();
    crate::hooks::use_relay_subscription(
        seller_chat_filter,
        move |event: &nostr_sdk::Event| {
            let event = event.clone();
            let sk = match seller_sk.clone() {
                Some(s) => s,
                None => return,
            };
            let my_pk = seller_my_pk.clone();
            spawn(async move {
                if crate::stores::mostro::dedup::is_seen(&event.id) { return; }
                crate::stores::mostro::dedup::mark_seen(event.id);
                if let Ok(msg) = mostro_core::chat::unwrap_chat_message(sk.keys(), &event).await {
                    let is_me = msg.sender.to_hex() == my_pk;
                    let cm = DisputeChatMsg {
                        content: msg.content, sender_hex: msg.sender.to_hex(),
                        is_me, timestamp: msg.created_at.as_secs() as i64, attachment: None,
                    };
                    if !is_dup_dispute_msg(&seller_chat.read(), &cm) {
                        seller_chat.write().push(cm);
                    }
                }
            });
        },
    );

    // Action handlers
    let on_settle = move |_| { *show_slash_picker.write() = Some(true); };
    let on_cancel = move |_| { *show_slash_picker.write() = Some(false); };

    let on_slash_confirm = {
        let admin_keys_clone = admin.keys.clone();
        let did = dispute_id.clone();
        let node_clone = node.clone();
        move |(slash_seller, slash_buyer): (bool, bool)| {
            let is_settle = show_slash_picker.read().unwrap_or(true);
            *show_slash_picker.write() = None;
            *action_busy.write() = true;
            let keys = admin_keys_clone.clone();
            let dispute_id = did.clone();
            let node_relays = node_clone.relays.clone();
            let node_for_pow = node_clone.clone();
            spawn(async move {
                let dispute_uuid = match uuid::Uuid::parse_str(&dispute_id) {
                    Ok(u) => u,
                    Err(e) => { *error.write() = Some(format!("Invalid dispute ID: {e}")); *action_busy.write() = false; return; }
                };
                let payload = if slash_seller || slash_buyer {
                    Some(prelude::Payload::BondResolution(prelude::BondResolution { slash_seller, slash_buyer }))
                } else { None };
                let action = if is_settle { prelude::Action::AdminSettle } else { prelude::Action::AdminCancel };
                let message = prelude::Message::new_dispute(
                    Some(dispute_uuid), Some(flow::next_request_id()), None, action, payload,
                );
                let pow = mostro_client::resolve_effective_pow(&node_for_pow, node_pk).await;
                if let Err(e) = mostro_client::send_mostro_message(
                    &message, &keys, &keys, node_pk, &node_relays, pow,
                ).await {
                    *error.write() = Some(format!("Send failed: {e}"));
                    *action_busy.write() = false;
                }
            });
        }
    };

    // Chat send handlers
    let on_buyer_chat_send = {
        let bs = buyer_shared.clone();
        let nr = node.relays.clone();
        let keys = admin.keys.clone();
        move |text: String| {
            let bs = bs.clone();
            let nr = nr.clone();
            let keys = keys.clone();
            spawn(async move {
                if let Some(shared) = bs {
                    let my_hex = keys.public_key().to_hex();
                    let now = crate::platform::timestamp::now_secs() as i64;
                    let cm = DisputeChatMsg {
                        content: text.clone(), sender_hex: my_hex.clone(),
                        is_me: true, timestamp: now, attachment: None,
                    };
                    if !is_dup_dispute_msg(&buyer_chat.read(), &cm) { buyer_chat.write().push(cm); }
                    if let Ok(event) = mostro_core::chat::wrap_chat_message(&keys, &shared.public_key(), &text).await {
                        use crate::stores::publish_queue::{self, types::QueueEventType};
                        publish_queue::enqueue(event, QueueEventType::DirectMessage, Some(nr), std::collections::HashMap::new()).await;
                    }
                }
            });
        }
    };

    let on_seller_chat_send = {
        let ss = seller_shared.clone();
        let nr = node.relays.clone();
        let keys = admin.keys.clone();
        move |text: String| {
            let ss = ss.clone();
            let nr = nr.clone();
            let keys = keys.clone();
            spawn(async move {
                if let Some(shared) = ss {
                    let my_hex = keys.public_key().to_hex();
                    let now = crate::platform::timestamp::now_secs() as i64;
                    let cm = DisputeChatMsg {
                        content: text.clone(), sender_hex: my_hex.clone(),
                        is_me: true, timestamp: now, attachment: None,
                    };
                    if !is_dup_dispute_msg(&seller_chat.read(), &cm) { seller_chat.write().push(cm); }
                    if let Ok(event) = mostro_core::chat::wrap_chat_message(&keys, &shared.public_key(), &text).await {
                        use crate::stores::publish_queue::{self, types::QueueEventType};
                        publish_queue::enqueue(event, QueueEventType::DirectMessage, Some(nr), std::collections::HashMap::new()).await;
                    }
                }
            });
        }
    };

    // Solver-side attachment upload handlers. Mirrors the user-side
    // dispute chat upload at `trade_detail.rs:1547-1651`, signed with
    // the admin/solver keys. The Blossom upload is signed with the
    // solver's long-lived admin key (analogous to the user side signing
    // with the per-trade key — the chat participant's identity signs).
    let on_buyer_chat_upload = {
        let bs = buyer_shared.clone();
        let nr = node.relays.clone();
        let keys = admin.keys.clone();
        move |(file_name, bytes, mime_type): (String, Vec<u8>, String)| {
            let bs = bs.clone();
            let nr = nr.clone();
            let keys = keys.clone();
            spawn(async move {
                let shared = match bs { Some(s) => s, None => return };
                let att_key = {
                    let raw = shared.secret_key().to_secret_bytes();
                    attachment_key_from_shared_secret(&raw)
                };
                let (encrypted, nonce) = match encrypt_attachment(&bytes, &att_key) {
                    Ok(r) => r,
                    Err(e) => {
                        log::warn!("solver attachment encrypt failed: {e}");
                        return;
                    }
                };
                let server = crate::stores::media::blossom_store::get_primary_server();
                let url = match crate::stores::media::blossom_store::upload_raw_blob_with_signer(
                    encrypted,
                    "application/octet-stream".to_string(),
                    Some(server),
                    &keys,
                ).await {
                    Ok(u) => u,
                    Err(e) => {
                        log::warn!("solver attachment upload failed: {e}");
                        return;
                    }
                };
                let meta = AttachmentMeta {
                    kind: AttachmentMeta::classify(&mime_type),
                    blossom_url: url,
                    nonce: encode_nonce(&nonce),
                    mime_type: mime_type.clone(),
                    original_size: bytes.len() as u64,
                    filename: Some(file_name.clone()),
                    encrypted_size: Some((bytes.len() + 12 + 16) as u64),
                    file_type: Some(AttachmentMeta::file_type_label(&mime_type).to_string()),
                    width: None,
                    height: None,
                };
                let content = encode_chat_content(
                    &format!("Sent a file: {file_name}"),
                    vec![meta.clone()],
                );
                // Optimistic local echo so the solver sees their upload
                // immediately (the relay round-trip can take a second).
                {
                    let now = crate::platform::timestamp::now_secs() as i64;
                    let my_hex = keys.public_key().to_hex();
                    let cm = DisputeChatMsg {
                        content: format!("Sent a file: {file_name}"),
                        sender_hex: my_hex,
                        is_me: true,
                        timestamp: now,
                        attachment: Some(meta),
                    };
                    if !is_dup_dispute_msg(&buyer_chat.read(), &cm) {
                        buyer_chat.write().push(cm);
                    }
                }
                if let Ok(event) = mostro_core::chat::wrap_chat_message(
                    &keys,
                    &shared.public_key(),
                    &content,
                ).await {
                    use crate::stores::publish_queue::{self, types::QueueEventType};
                    publish_queue::enqueue(
                        event,
                        QueueEventType::DirectMessage,
                        Some(nr),
                        std::collections::HashMap::new(),
                    ).await;
                }
            });
        }
    };

    let on_seller_chat_upload = {
        let ss = seller_shared.clone();
        let nr = node.relays.clone();
        let keys = admin.keys.clone();
        move |(file_name, bytes, mime_type): (String, Vec<u8>, String)| {
            let ss = ss.clone();
            let nr = nr.clone();
            let keys = keys.clone();
            spawn(async move {
                let shared = match ss { Some(s) => s, None => return };
                let att_key = {
                    let raw = shared.secret_key().to_secret_bytes();
                    attachment_key_from_shared_secret(&raw)
                };
                let (encrypted, nonce) = match encrypt_attachment(&bytes, &att_key) {
                    Ok(r) => r,
                    Err(e) => {
                        log::warn!("solver attachment encrypt failed: {e}");
                        return;
                    }
                };
                let server = crate::stores::media::blossom_store::get_primary_server();
                let url = match crate::stores::media::blossom_store::upload_raw_blob_with_signer(
                    encrypted,
                    "application/octet-stream".to_string(),
                    Some(server),
                    &keys,
                ).await {
                    Ok(u) => u,
                    Err(e) => {
                        log::warn!("solver attachment upload failed: {e}");
                        return;
                    }
                };
                let meta = AttachmentMeta {
                    kind: AttachmentMeta::classify(&mime_type),
                    blossom_url: url,
                    nonce: encode_nonce(&nonce),
                    mime_type: mime_type.clone(),
                    original_size: bytes.len() as u64,
                    filename: Some(file_name.clone()),
                    encrypted_size: Some((bytes.len() + 12 + 16) as u64),
                    file_type: Some(AttachmentMeta::file_type_label(&mime_type).to_string()),
                    width: None,
                    height: None,
                };
                let content = encode_chat_content(
                    &format!("Sent a file: {file_name}"),
                    vec![meta.clone()],
                );
                {
                    let now = crate::platform::timestamp::now_secs() as i64;
                    let my_hex = keys.public_key().to_hex();
                    let cm = DisputeChatMsg {
                        content: format!("Sent a file: {file_name}"),
                        sender_hex: my_hex,
                        is_me: true,
                        timestamp: now,
                        attachment: Some(meta),
                    };
                    if !is_dup_dispute_msg(&seller_chat.read(), &cm) {
                        seller_chat.write().push(cm);
                    }
                }
                if let Ok(event) = mostro_core::chat::wrap_chat_message(
                    &keys,
                    &shared.public_key(),
                    &content,
                ).await {
                    use crate::stores::publish_queue::{self, types::QueueEventType};
                    publish_queue::enqueue(
                        event,
                        QueueEventType::DirectMessage,
                        Some(nr),
                        std::collections::HashMap::new(),
                    ).await;
                }
            });
        }
    };

    let info = solver_info.read().clone();
    let admin_pk_hex = admin.keys.public_key().to_hex();

    rsx! {
        div { class: "min-h-screen p-4 max-w-3xl mx-auto space-y-4",
            div { class: "flex items-center gap-3",
                button {
                    class: "p-2 hover:bg-accent rounded-lg",
                    onclick: move |_| { let _ = nav.push(Route::MostroAdminDisputes {}); },
                    crate::components::icons::ArrowLeftIcon { class: "w-5 h-5".to_string() }
                }
                h1 { class: "text-xl font-bold", "Dispute Detail" }
            }

            if let Some(ref err) = *error.read() {
                div { class: "p-3 bg-red-500/10 border border-red-500/20 rounded-lg",
                    p { class: "text-sm text-red-500", "{err}" }
                }
            }

            if *taking.read() {
                div { class: "p-4 text-center",
                    span { class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" }
                    p { class: "text-sm text-muted-foreground mt-2", "Taking dispute…" }
                }
            }

            if let Some(ref s) = info {
                div { class: "p-4 bg-card border border-border rounded-lg space-y-3",
                    h3 { class: "text-sm font-semibold", "Dispute Info" }
                    div { class: "grid grid-cols-2 gap-2 text-sm",
                        div { span { class: "text-muted-foreground", "Order: " } span { class: "font-mono text-xs", "{s.id}" } }
                        div { span { class: "text-muted-foreground", "Kind: " } span { class: "font-medium", "{s.kind}" } }
                        div { span { class: "text-muted-foreground", "Amount: " } span { class: "font-medium", "{s.amount} sats" } }
                        div { span { class: "text-muted-foreground", "Fiat: " } span { class: "font-medium", "{s.fiat_amount}" } }
                        div { span { class: "text-muted-foreground", "Premium: " } span { class: "font-medium", "{s.premium}%" } }
                        div { span { class: "text-muted-foreground", "Payment: " } span { class: "font-medium", "{s.payment_method}" } }
                    }
                    if let Some(ref bp) = s.buyer_pubkey {
                        div { span { class: "text-xs text-muted-foreground", "Buyer: " } span { class: "text-xs font-mono", "{bp}" } }
                    }
                    if let Some(ref sp) = s.seller_pubkey {
                        div { span { class: "text-xs text-muted-foreground", "Seller: " } span { class: "text-xs font-mono", "{sp}" } }
                    }
                    if let Some(ref hash) = s.hash {
                        div { span { class: "text-xs text-muted-foreground", "Hash: " } span { class: "text-xs font-mono", "{hash}" } }
                    }
                }

                if !*action_busy.read() {
                    div { class: "flex gap-2",
                        button {
                            class: "flex-1 px-4 py-2 bg-green-600 text-white rounded-lg text-sm font-medium",
                            onclick: on_settle,
                            "Settle (Pay Buyer)"
                        }
                        button {
                            class: "flex-1 px-4 py-2 bg-red-600 text-white rounded-lg text-sm font-medium",
                            onclick: on_cancel,
                            "Cancel (Refund Seller)"
                        }
                    }
                } else {
                    div { class: "flex justify-center py-2",
                        span { class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin text-muted-foreground" }
                    }
                }

                if buyer_shared.is_some() {
                    DisputeChat {
                        messages: buyer_chat.read().clone(),
                        locked: false,
                        my_pubkey_hex: admin_pk_hex.clone(),
                        on_send: on_buyer_chat_send,
                        on_upload_file: on_buyer_chat_upload,
                    }
                }
                if seller_shared.is_some() {
                    div { class: "mt-4",
                        DisputeChat {
                            messages: seller_chat.read().clone(),
                            locked: false,
                            my_pubkey_hex: admin_pk_hex.clone(),
                            on_send: on_seller_chat_send,
                            on_upload_file: on_seller_chat_upload,
                        }
                    }
                }
            } else if !*taking.read() {
                div { class: "p-8 text-center text-muted-foreground", "Waiting for dispute data from daemon…" }
            }
        }

        if let Some(is_settle) = *show_slash_picker.read() {
            BondSlashPicker {
                show: true,
                is_settle: is_settle,
                on_confirm: on_slash_confirm,
                on_cancel: move |_| *show_slash_picker.write() = None,
            }
        }
    }
}
