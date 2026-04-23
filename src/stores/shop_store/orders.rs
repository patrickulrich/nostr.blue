//! Order management for the shop store
//!
//! Handles order creation, processing, status updates, gift-wrap messaging,
//! payment receipts, DB persistence, and order lifecycle management.

use super::*;

/// Ensure orders are loaded from DB (call when auth becomes available)
/// CDK pattern: idempotent, returns Ok(()) if already loaded or no DB
pub async fn ensure_orders_loaded() -> Result<()> {
    use std::sync::atomic::Ordering;
    let db = match SHOP_DB.read().as_ref() {
        Some(db) => db.clone(),
        None => return Ok(()),
    };
    if !ORDERS_LOADED_FROM_DB.load(Ordering::SeqCst) && restore_orders_from_db(&db).await? {
        ORDERS_LOADED_FROM_DB.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Reset orders loaded flag (call on logout)
pub fn reset_orders_loaded_flag() {
    use std::sync::atomic::Ordering;
    ORDERS_LOADED_FROM_DB.store(false, Ordering::SeqCst);
    log::debug!("Orders loaded flag reset");
}

/// Initialize the shop store and restore persisted orders from IndexedDB
pub async fn init_shop_store() -> Result<()> {
    if *SHOP_INITIALIZED.read() {
        log::debug!("Shop store already initialized");
        return Ok(());
    }
    log::info!("Initializing shop store...");
    match ShopDatabase::new().await {
        Ok(db) => {
            let db_arc = Arc::new(db);
            *SHOP_DB.write() = Some(db_arc.clone());
            log::info!("Shop database initialized");
            match restore_orders_from_db(&db_arc).await {
                Ok(true) => {
                    use std::sync::atomic::Ordering;
                    ORDERS_LOADED_FROM_DB.store(true, Ordering::SeqCst);
                    log::info!("Orders restored from database, flag set");
                }
                Ok(false) => {
                    log::debug!("Order restore skipped (no auth), will retry on auth");
                }
                Err(e) => {
                    log::warn!("Failed to restore orders from database: {}", e);
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to initialize shop database: {:?}", e);
        }
    }
    *SHOP_INITIALIZED.write() = true;
    Ok(())
}

/// Restore orders from IndexedDB into memory signals
/// Returns true if orders were actually processed (or none exist), false if skipped due to missing auth
async fn restore_orders_from_db(db: &ShopDatabase) -> Result<bool> {
    log::info!("Restoring orders from database...");
    let orders = db
        .get_all_orders()
        .await
        .map_err(|e| format!("Failed to load orders: {:?}", e))?;
    if orders.is_empty() {
        log::info!("No persisted orders found");
        return Ok(true);
    }
    let user_pubkey = match nostr_client::get_cached_pubkey() {
        Ok(pk) => pk.to_hex(),
        Err(_) => {
            log::warn!("Cannot load orders - not authenticated");
            return Ok(false);
        }
    };
    let mut buyer_orders = Vec::new();
    let mut seller_orders = Vec::new();
    for order in orders {
        if order.buyer_pubkey == user_pubkey {
            buyer_orders.push(order);
        } else if order.merchant_pubkey == user_pubkey {
            seller_orders.push(order.clone());
        }
    }
    log::info!(
        "Restored {} buyer orders, {} seller orders",
        buyer_orders.len(),
        seller_orders.len()
    );
    *BUYER_ORDERS.write() = buyer_orders;
    *SELLER_ORDERS.write() = seller_orders;
    Ok(true)
}

/// Persist an order to IndexedDB
pub async fn persist_order(order: &ShopOrder) -> Result<()> {
    let db = SHOP_DB.read();
    if let Some(db) = db.as_ref() {
        db.save_order(order)
            .await
            .map_err(|e| format!("Failed to persist order: {:?}", e))?;
        log::debug!("Persisted order {} to IndexedDB", order.order_id);
    }
    Ok(())
}

/// Update a persisted order in IndexedDB
pub async fn update_persisted_order(order: &ShopOrder) -> Result<()> {
    let db = SHOP_DB.read();
    if let Some(db) = db.as_ref() {
        db.update_order(order)
            .await
            .map_err(|e| format!("Failed to update order: {:?}", e))?;
        log::debug!("Updated order {} in IndexedDB", order.order_id);
    }
    Ok(())
}

/// Order message content structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderMessageContent {
    /// Message type (serializes as string for compatibility)
    pub message_type: String,
    /// Order ID
    pub order_id: String,
    /// Message payload (depends on type)
    pub payload: serde_json::Value,
    /// Timestamp
    pub timestamp: u64,
}

impl OrderMessageContent {
    /// Get the message type as enum (for type-safe matching)
    /// Supports both string ("order", "payment") and numeric ("1", "2") formats
    pub fn get_type(&self) -> Option<OrderMessageType> {
        if let Some(t) = OrderMessageType::from_str(&self.message_type) {
            return Some(t);
        }
        if let Ok(num) = self.message_type.parse::<u8>() {
            return OrderMessageType::from_u8(num);
        }
        None
    }

    pub fn new_order(
        order_id: &str,
        items: Vec<OrderItem>,
        shipping_address: Option<String>,
        amount_sats: u64,
        shipping_option: Option<String>,
    ) -> Self {
        Self {
            message_type: OrderMessageType::OrderCreation.as_str().to_string(),
            order_id: order_id.to_string(),
            payload: serde_json::json!(
                { "items" : items, "shipping_address" : shipping_address, "amount_sats" :
                amount_sats, "shipping_option" : shipping_option, }
            ),
            timestamp: now_secs(),
        }
    }

    pub fn new_payment(order_id: &str, payment_method: &str, payment_proof: &str) -> Self {
        Self {
            message_type: OrderMessageType::PaymentRequest.as_str().to_string(),
            order_id: order_id.to_string(),
            payload: serde_json::json!(
                { "method" : payment_method, "proof" : payment_proof, }
            ),
            timestamp: now_secs(),
        }
    }

    pub fn new_status(order_id: &str, status: &str, message: Option<&str>) -> Self {
        Self {
            message_type: OrderMessageType::StatusUpdate.as_str().to_string(),
            order_id: order_id.to_string(),
            payload: serde_json::json!({ "status" : status, "message" : message, }),
            timestamp: now_secs(),
        }
    }

    pub fn new_shipping(order_id: &str, carrier: &str, tracking_number: &str) -> Self {
        Self {
            message_type: OrderMessageType::ShippingUpdate.as_str().to_string(),
            order_id: order_id.to_string(),
            payload: serde_json::json!(
                { "carrier" : carrier, "tracking_number" : tracking_number, }
            ),
            timestamp: now_secs(),
        }
    }
}

/// Helper to create and send gift-wrapped events to both recipient and sender
///
/// Creates NIP-17 gift wraps for both the recipient and sender (for their records),
/// then sends both events. Returns (receiver_event_id, sender_event_id).
async fn send_gift_wrapped_rumor(
    _client: &nostr_sdk::Client,
    signer: &impl nostr_sdk::NostrSigner,
    recipient_pk: PublicKey,
    sender_pk: PublicKey,
    rumor: nostr::UnsignedEvent,
    _log_context: &str,
) -> Result<(String, String)> {
    let receiver_gift_wrap = EventBuilder::gift_wrap(signer, &recipient_pk, rumor.clone(), [])
        .await
        .map_err(|e| format!("Failed to create receiver gift wrap: {}", e))?;
    let sender_gift_wrap = EventBuilder::gift_wrap(signer, &sender_pk, rumor, [])
        .await
        .map_err(|e| format!("Failed to create sender gift wrap: {}", e))?;
    let receiver_event_id = receiver_gift_wrap.id.to_hex();
    crate::stores::publish_queue::enqueue(
        receiver_gift_wrap,
        crate::stores::publish_queue::types::QueueEventType::DirectMessage,
        None,
        std::collections::HashMap::new(),
    ).await;
    let sender_event_id = sender_gift_wrap.id.to_hex();
    crate::stores::publish_queue::enqueue(
        sender_gift_wrap,
        crate::stores::publish_queue::types::QueueEventType::DirectMessage,
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok((receiver_event_id, sender_event_id))
}

/// Send an encrypted order message to a recipient via NIP-17 gift wrap
pub async fn send_order_message(
    recipient_pubkey: &str,
    content: OrderMessageContent,
) -> Result<String> {
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let recipient_pk = PublicKey::parse(recipient_pubkey)
        .map_err(|e| format!("Invalid recipient pubkey: {}", e))?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let sender_pk = crate::stores::nostr_client::get_cached_pubkey()
        .map_err(|e| format!("Failed to get sender pubkey: {}", e))?;
    let message_json = serde_json::to_string(&content)
        .map_err(|e| format!("Failed to serialize order message: {}", e))?;
    log::info!(
        "Sending order message to {}: type={}",
        recipient_pubkey,
        content.message_type
    );
    let rumor = crate::utils::nips::nip89::tag_event_builder(EventBuilder::private_msg_rumor(
        recipient_pk,
        message_json,
    ))
    .build(sender_pk);
    send_gift_wrapped_rumor(
        &client,
        &signer,
        recipient_pk,
        sender_pk,
        rumor,
        "order message",
    )
    .await?;
    Ok(content.order_id)
}

/// Send a Kind 17 payment receipt to merchant via NIP-17 gift wrap
///
/// This is sent by the buyer after successful payment to confirm the transaction.
/// Includes payment proof that the merchant can verify.
pub async fn send_payment_receipt(
    merchant_pubkey: &str,
    order_id: &str,
    amount_sats: u64,
    payment_method: &str,
    medium_reference: &str,
    proof: &str,
) -> Result<String> {
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let recipient_pk =
        PublicKey::parse(merchant_pubkey).map_err(|e| format!("Invalid merchant pubkey: {}", e))?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let sender_pk = crate::stores::nostr_client::get_cached_pubkey()
        .map_err(|e| format!("Failed to get sender pubkey: {}", e))?;
    let content = format!(
        "Payment receipt for order {} - {} sats via {}",
        order_id, amount_sats, payment_method,
    );
    let tags = vec![
        Tag::public_key(recipient_pk),
        Tag::custom(
            TagKind::Custom("subject".into()),
            vec!["order-receipt".to_string()],
        ),
        Tag::custom(TagKind::Custom("order".into()), vec![order_id.to_string()]),
        Tag::custom(
            TagKind::Custom("payment".into()),
            vec![
                payment_method.to_string(),
                medium_reference.to_string(),
                proof.to_string(),
            ],
        ),
        Tag::custom(
            TagKind::Custom("amount".into()),
            vec![amount_sats.to_string(), "sat".to_string()],
        ),
    ];
    let rumor = crate::utils::nips::nip89::tag_event_builder(
        EventBuilder::new(Kind::Custom(KIND_PAYMENT_RECEIPT), &content).tags(tags),
    )
    .build(sender_pk);
    log::info!(
        "Sending payment receipt for order {} to merchant {}",
        order_id,
        merchant_pubkey
    );
    send_gift_wrapped_rumor(
        &client,
        &signer,
        recipient_pk,
        sender_pk,
        rumor,
        "payment receipt",
    )
    .await?;
    Ok(order_id.to_string())
}

/// Validate payment proof format based on payment method
///
/// Returns Ok(()) if valid, Err with message if invalid.
fn validate_payment_proof(payment_method: &str, payment_proof: &str) -> Result<()> {
    if payment_proof.is_empty() {
        return Err("Payment proof is required".to_string());
    }
    match payment_method {
        "lightning" => {
            for preimage in payment_proof.split(',') {
                let trimmed = preimage.trim();
                if trimmed == "manual_payment" {
                    continue;
                }
                if trimmed.len() != 64 {
                    return Err(format!(
                        "Invalid Lightning preimage length: expected 64 hex chars, got {}",
                        trimmed.len(),
                    ));
                }
                if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err("Invalid Lightning preimage: must be hexadecimal".to_string());
                }
            }
        }
        "cashu" => {
            if !payment_proof.starts_with("cashu") {
                return Err("Invalid Cashu token format: must start with 'cashu'".to_string());
            }
        }
        "bitcoin" => {
            if payment_proof.len() != 64 {
                return Err("Invalid Bitcoin txid: expected 64 hex chars".to_string());
            }
            if !payment_proof.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("Invalid Bitcoin txid: must be hexadecimal".to_string());
            }
        }
        _ => {
            log::warn!(
                "Unknown payment method '{}', skipping proof validation",
                payment_method
            );
        }
    }
    Ok(())
}

/// Create and place a shop order
///
/// This creates an order and sends the initial order message to the merchant.
/// Returns the order ID.
pub async fn create_shop_order(
    items: Vec<CartItem>,
    shipping_address: Option<String>,
    shipping_option: Option<String>,
    payment_method: &str,
    payment_proof: &str,
) -> Result<String> {
    let buyer_pubkey = nostr_client::get_cached_pubkey()
        .map(|pk| pk.to_hex())
        .map_err(|_| "Cannot checkout - not authenticated".to_string())?;
    if items.is_empty() {
        return Err("Cannot create order with no items".to_string());
    }
    validate_payment_proof(payment_method, payment_proof)?;
    let order_id = crate::utils::format::generate_unique_id();
    let mut merchants: HashMap<String, (Vec<OrderItem>, u64)> = HashMap::new();
    for item in &items {
        let merchant_pubkey = item.product.pubkey.clone();
        let order_item = OrderItem {
            product_coordinate: item.product.coordinate.clone(),
            quantity: item.quantity,
        };
        let price_sats = if item.product.price.currency.eq_ignore_ascii_case("sats")
            || item.product.price.currency.eq_ignore_ascii_case("sat")
        {
            item.product.price.amount as u64
        } else {
            return Err(
                format!(
                    "Cannot checkout: \"{}\" has unsupported currency '{}'. Only sats pricing is currently supported.",
                    item.product.title,
                    item.product.price.currency,
                ),
            );
        };
        let item_total = price_sats
            .checked_mul(item.quantity as u64)
            .ok_or_else(|| {
                format!(
                    "Arithmetic overflow calculating total for '{}' (price {} x quantity {})",
                    item.product.title, price_sats, item.quantity,
                )
            })?;
        let entry = merchants
            .entry(merchant_pubkey)
            .or_insert((Vec::new(), 0u64));
        entry.0.push(order_item);
        entry.1 = entry
            .1
            .checked_add(item_total)
            .ok_or_else(|| "Arithmetic overflow calculating merchant subtotal".to_string())?;
    }
    let total_sats: u64 = {
        let mut total = 0u64;
        for (_, subtotal) in merchants.values() {
            total = total
                .checked_add(*subtotal)
                .ok_or_else(|| "Arithmetic overflow calculating order total".to_string())?;
        }
        total
    };
    let medium_reference = match payment_method {
        "cashu" => "ecash".to_string(),
        "lightning" => "lightning".to_string(),
        "bitcoin" => "bitcoin".to_string(),
        _ => payment_method.to_string(),
    };
    for (merchant_pubkey, (merchant_items, subtotal)) in &merchants {
        let order_msg = OrderMessageContent::new_order(
            &order_id,
            merchant_items.clone(),
            shipping_address.clone(),
            *subtotal,
            shipping_option.clone(),
        );
        send_order_message(merchant_pubkey, order_msg).await?;
        let payment_msg =
            OrderMessageContent::new_payment(&order_id, payment_method, payment_proof);
        send_order_message(merchant_pubkey, payment_msg).await?;
        if let Err(e) = send_payment_receipt(
            merchant_pubkey,
            &order_id,
            total_sats,
            payment_method,
            &medium_reference,
            payment_proof,
        )
        .await
        {
            log::warn!("Failed to send payment receipt: {}", e);
        }
    }
    let now = now_secs();
    let merchant_count = merchants.len();
    for (merchant_pubkey, (merchant_items, merchant_total)) in &merchants {
        let merchant_order_id = if merchant_count > 1 {
            let short_pk = truncate_pubkey(merchant_pubkey);
            format!("{}-{}", order_id, short_pk)
        } else {
            order_id.clone()
        };
        let merchant_shipping = items
            .iter()
            .find(|item| &item.product.pubkey == merchant_pubkey)
            .and_then(|item| item.selected_shipping.clone());
        let order = ShopOrder {
            order_id: merchant_order_id,
            buyer_pubkey: buyer_pubkey.clone(),
            merchant_pubkey: merchant_pubkey.clone(),
            items: merchant_items.clone(),
            amount_sats: *merchant_total,
            status: OrderStatus::Pending,
            shipping_status: None,
            shipping_option: merchant_shipping,
            shipping_address: shipping_address.clone(),
            tracking_number: None,
            carrier: None,
            email: None,
            phone: None,
            created_at: now,
            updated_at: now,
            paid_at: Some(now),
            shipped_at: None,
            delivered_at: None,
            note: None,
            download_url: None,
            license_key: None,
        };
        add_buyer_order(order).await;
    }
    log::info!(
        "Created order: {} with {} items, total: {} sats",
        order_id,
        items.len(),
        total_sats
    );
    Ok(order_id)
}

/// Fetch orders for the current user (as buyer)
pub async fn fetch_my_orders() -> Result<Vec<ShopOrder>> {
    Ok(BUYER_ORDERS.read().clone())
}

/// Fetch orders for the current user (as seller)
pub async fn fetch_seller_orders() -> Result<Vec<ShopOrder>> {
    Ok(SELLER_ORDERS.read().clone())
}

/// Process an incoming order message and update local state
/// Uses OrderStatus::from_str() and ShippingStatus::from_str() for parsing
/// sender_pubkey is provided when message comes from gift wrap to identify buyer
pub async fn process_order_message(
    msg: &OrderMessageContent,
    sender_pubkey: Option<&str>,
) -> Result<()> {
    let order_id = &msg.order_id;
    let updated_order: Option<ShopOrder>;
    match msg.get_type() {
        Some(OrderMessageType::OrderCreation) => {
            log::info!("Received new order: {}", order_id);
            if let Some(buyer) = sender_pubkey {
                if let Some(items_value) = msg.payload.get("items") {
                    if let Ok(items) = serde_json::from_value::<Vec<OrderItem>>(items_value.clone())
                    {
                        let shipping_address = msg
                            .payload
                            .get("shipping_address")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                        let shipping_option = msg
                            .payload
                            .get("shipping_option")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                        let total_sats = msg
                            .payload
                            .get("amount_sats")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let merchant_pubkey = match nostr_client::get_cached_pubkey() {
                            Ok(pk) => pk.to_hex(),
                            Err(_) => {
                                log::warn!("Deferring order message - not authenticated");
                                return Err(
                                    "Not authenticated - order message will be reprocessed"
                                        .to_string(),
                                );
                            }
                        };
                        let order = ShopOrder {
                            order_id: order_id.clone(),
                            buyer_pubkey: buyer.to_string(),
                            merchant_pubkey,
                            items,
                            amount_sats: total_sats,
                            status: OrderStatus::Pending,
                            shipping_status: None,
                            shipping_option,
                            shipping_address,
                            tracking_number: None,
                            carrier: None,
                            email: None,
                            phone: None,
                            created_at: msg.timestamp,
                            updated_at: msg.timestamp,
                            paid_at: None,
                            shipped_at: None,
                            delivered_at: None,
                            note: None,
                            download_url: None,
                            license_key: None,
                        };
                        add_seller_order(order).await;
                        log::info!("Added order {} to seller orders", order_id);
                    }
                }
            }
            updated_order = None;
        }
        Some(OrderMessageType::PaymentRequest) => {
            log::info!("Received payment for order: {}", order_id);
            let mut orders = BUYER_ORDERS.write();
            updated_order = if let Some(order) = orders.iter_mut().find(|o| o.order_id == *order_id)
            {
                order.status = OrderStatus::Confirmed;
                order.paid_at = Some(msg.timestamp);
                order.updated_at = msg.timestamp;
                Some(order.clone())
            } else {
                None
            };
        }
        Some(OrderMessageType::StatusUpdate) => {
            let mut orders = BUYER_ORDERS.write();
            updated_order =
                if let Some(status_str) = msg.payload.get("status").and_then(|v| v.as_str()) {
                    if let Some(new_status) = OrderStatus::from_str(status_str) {
                        log::info!("Order {} status updated to: {}", order_id, new_status);
                        if let Some(order) = orders.iter_mut().find(|o| o.order_id == *order_id) {
                            order.status = new_status;
                            order.updated_at = msg.timestamp;
                            if !new_status.is_active() {
                                log::info!("Order {} is no longer active", order_id);
                            }
                            Some(order.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
        }
        Some(OrderMessageType::ShippingUpdate) => {
            let tracking = msg.payload.get("tracking_number").and_then(|v| v.as_str());
            let carrier = msg.payload.get("carrier").and_then(|v| v.as_str());
            let status_str = msg.payload.get("status").and_then(|v| v.as_str());
            log::info!(
                "Shipping update for order {}: tracking={:?}",
                order_id,
                tracking
            );
            let mut orders = BUYER_ORDERS.write();
            updated_order = if let Some(order) = orders.iter_mut().find(|o| o.order_id == *order_id)
            {
                if let Some(t) = tracking {
                    order.tracking_number = Some(t.to_string());
                }
                if let Some(c) = carrier {
                    order.carrier = Some(c.to_string());
                }
                if let Some(s) = status_str {
                    if let Some(ship_status) = ShippingStatus::from_str(s) {
                        order.shipping_status = Some(ship_status);
                        if ship_status == ShippingStatus::Shipped {
                            order.shipped_at = Some(msg.timestamp);
                        } else if ship_status == ShippingStatus::Delivered {
                            order.delivered_at = Some(msg.timestamp);
                        }
                    }
                }
                order.updated_at = msg.timestamp;
                Some(order.clone())
            } else {
                None
            };
        }
        Some(OrderMessageType::Message) => {
            if let Some(text) = msg.payload.get("text").and_then(|v| v.as_str()) {
                log::info!("Message for order {}: {}", order_id, text);
            }
            updated_order = None;
        }
        None => {
            log::warn!("Unknown order message type: {}", msg.message_type);
            updated_order = None;
        }
    }
    if let Some(order) = updated_order {
        if let Err(e) = update_persisted_order(&order).await {
            log::warn!("Failed to persist order update to IndexedDB: {}", e);
        }
    }
    Ok(())
}

/// Listen for order updates via NIP-17 gift wrap messages
/// This would be called on app startup to process any pending messages
pub async fn listen_for_order_updates() -> Result<()> {
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let my_pubkey = crate::stores::nostr_client::get_cached_pubkey()
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(my_pubkey)
        .limit(100);
    log::info!("Fetching order update messages...");
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch gift wraps: {}", e))?;
    log::info!("Found {} gift wrap events", events.len());
    for event in events.iter() {
        let event_id = event.id.to_hex();
        if PROCESSED_ORDER_EVENTS.read().contains(&event_id) {
            log::debug!("Skipping already processed order event: {}", event_id);
            continue;
        }
        match client.unwrap_gift_wrap(event).await {
            Ok(unwrapped) => {
                let rumor = unwrapped.rumor;
                let kind_num = rumor.kind.as_u16();
                if kind_num == KIND_ORDER_MESSAGE || kind_num == KIND_PAYMENT_RECEIPT {
                    let sender_pubkey = rumor.pubkey.to_hex();
                    match serde_json::from_str::<OrderMessageContent>(&rumor.content) {
                        Ok(msg) => {
                            if let Err(e) = process_order_message(&msg, Some(&sender_pubkey)).await
                            {
                                log::error!("Failed to process order message: {}", e);
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to parse order message: {}", e);
                        }
                    }
                }
                PROCESSED_ORDER_EVENTS.write().put(event_id, ());
            }
            Err(e) => {
                log::debug!("Failed to unwrap gift wrap: {}", e);
            }
        }
    }
    Ok(())
}

/// Get count of active orders (pending, confirmed, processing)
pub fn get_active_order_count() -> usize {
    BUYER_ORDERS
        .read()
        .iter()
        .filter(|o| o.status.is_active())
        .count()
}

/// Get count of seller's active orders
pub fn get_seller_active_order_count() -> usize {
    SELLER_ORDERS
        .read()
        .iter()
        .filter(|o| o.status.is_active())
        .count()
}

/// Update order status (seller action)
pub async fn update_order_status(
    order_id: &str,
    new_status: OrderStatus,
    shipping_status: Option<ShippingStatus>,
    tracking_number: Option<String>,
    carrier: Option<String>,
) -> Result<()> {
    let mut orders = SELLER_ORDERS.write();
    let order = orders
        .iter_mut()
        .find(|o| o.order_id == order_id)
        .ok_or("Order not found")?;
    order.status = new_status;
    order.updated_at = now_secs();
    if let Some(ref ship_status) = shipping_status {
        order.shipping_status = Some(*ship_status);
        if *ship_status == ShippingStatus::Shipped {
            order.shipped_at = Some(order.updated_at);
        } else if *ship_status == ShippingStatus::Delivered {
            order.delivered_at = Some(order.updated_at);
        }
    }
    if let Some(ref tracking) = tracking_number {
        order.tracking_number = Some(tracking.clone());
    }
    if let Some(ref c) = carrier {
        order.carrier = Some(c.clone());
    }
    let buyer_pubkey = order.buyer_pubkey.clone();
    let order_for_persist = order.clone();
    let content = if shipping_status.is_some() || tracking_number.is_some() {
        OrderMessageContent::new_shipping(
            order_id,
            carrier.as_deref().unwrap_or(""),
            tracking_number.as_deref().unwrap_or(""),
        )
    } else {
        OrderMessageContent::new_status(order_id, new_status.as_str(), None)
    };
    drop(orders);
    send_order_message(&buyer_pubkey, content).await?;
    if let Err(e) = update_persisted_order(&order_for_persist).await {
        log::warn!("Failed to persist seller order update to IndexedDB: {}", e);
    }
    Ok(())
}

/// Add an order to seller orders (called when receiving order from buyer)
pub async fn add_seller_order(order: ShopOrder) {
    if let Err(e) = persist_order(&order).await {
        log::warn!("Failed to persist seller order to IndexedDB: {}", e);
    }
    SELLER_ORDERS.write().push(order);
}

/// Add an order to buyer orders (called when creating order)
pub async fn add_buyer_order(order: ShopOrder) {
    if let Err(e) = persist_order(&order).await {
        log::warn!("Failed to persist buyer order to IndexedDB: {}", e);
    }
    BUYER_ORDERS.write().push(order);
}
