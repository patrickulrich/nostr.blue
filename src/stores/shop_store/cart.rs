//! Cart management for the shop store
//!
//! Handles adding/removing items, quantity updates, shipping selection,
//! and cart total recalculation.

use super::*;

/// Add a product to the cart
pub fn add_to_cart(product: Product, quantity: u32) {
    let mut items = CART_ITEMS.write();
    if let Some(existing) = items
        .iter_mut()
        .find(|item| item.product.naddr == product.naddr)
    {
        existing.quantity += quantity;
    } else {
        items
            .push(CartItem {
                product,
                quantity,
                selected_shipping: None,
            });
    }
    drop(items);
    recalculate_cart_total();
}

/// Update quantity for a cart item
pub fn update_cart_quantity(naddr: &str, quantity: u32) {
    let mut items = CART_ITEMS.write();
    if quantity == 0 {
        items.retain(|item| item.product.naddr != naddr);
    } else if let Some(item) = items.iter_mut().find(|item| item.product.naddr == naddr)
    {
        item.quantity = quantity;
    }
    drop(items);
    recalculate_cart_total();
}

/// Remove an item from the cart
pub fn remove_from_cart(naddr: &str) {
    let mut items = CART_ITEMS.write();
    items.retain(|item| item.product.naddr != naddr);
    drop(items);
    recalculate_cart_total();
}

/// Set shipping option for a cart item
pub fn set_cart_item_shipping(naddr: &str, shipping_naddr: Option<String>) {
    let mut items = CART_ITEMS.write();
    if let Some(item) = items.iter_mut().find(|item| item.product.naddr == naddr) {
        item.selected_shipping = shipping_naddr;
    }
    drop(items);
    recalculate_cart_total();
}

/// Clear the entire cart
pub fn clear_cart() {
    CART_ITEMS.write().clear();
    *CART_TOTAL_SATS.write() = 0;
}

/// Get cart item count
pub fn get_cart_count() -> usize {
    CART_ITEMS.read().iter().map(|item| item.quantity as usize).sum()
}

/// Recalculate cart total (internal helper)
pub(super) fn recalculate_cart_total() {
    let items = CART_ITEMS.read();
    let mut total: u64 = 0;
    for item in items.iter() {
        if let Some(sats) = item.product.price.to_sats() {
            total += sats * item.quantity as u64;
        } else {
            log::warn!(
                "Unable to convert {} {} to sats for product {}", item.product.price
                .amount, item.product.price.currency, item.product.title
            );
        }
        if let Some(shipping_naddr) = &item.selected_shipping {
            if let Some(shipping) = get_cached_shipping(shipping_naddr) {
                if let Some(shipping_sats) = shipping.to_sats() {
                    total += shipping_sats;
                } else {
                    log::warn!(
                        "Unable to convert shipping cost {} {} to sats for {}", shipping
                        .base_price, shipping.currency, shipping.title
                    );
                }
            }
        }
    }
    *CART_TOTAL_SATS.write() = total;
}
