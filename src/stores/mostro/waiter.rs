use dioxus::prelude::*;
use std::collections::HashMap;
use tokio::sync::oneshot;

struct WaiterEntry {
    sender: oneshot::Sender<mostro_core::prelude::Message>,
}

type WaiterMap = HashMap<String, WaiterEntry>;

static WAITERS: GlobalSignal<WaiterMap> = Signal::global(HashMap::new);

pub fn register_waiter(
    order_id: String,
    request_id: u64,
) -> oneshot::Receiver<mostro_core::prelude::Message> {
    let (tx, rx) = oneshot::channel();
    let key = format!("{order_id}:{request_id}");
    WAITERS.write().insert(key, WaiterEntry { sender: tx });
    rx
}

pub fn try_satisfy_waiter(
    order_id: &str,
    request_id: Option<u64>,
    message: mostro_core::prelude::Message,
) -> bool {
    let Some(rid) = request_id else {
        return false;
    };
    let key = format!("{order_id}:{rid}");
    let mut map = WAITERS.write();
    if let Some(entry) = map.remove(&key) {
        let _ = entry.sender.send(message);
        return true;
    }
    false
}

pub fn prune_waiters_for_order(order_id: &str) {
    let prefix = format!("{order_id}:");
    WAITERS.write().retain(|k, _| !k.starts_with(&prefix));
}

/// Remove a single waiter entry by `(order_id, request_id)`.
///
/// Called from the ack-timeout handler in `trade_detail.rs` to prevent
/// the `WAITERS` HashMap from growing unboundedly when daemon responses
/// never arrive (relay downtime, daemon outage, etc.). Without this, the
/// 15-second timeout drops only the `oneshot::Receiver`; the `Sender`
/// side (inside `WaiterEntry`) and the HashMap entry would leak.
#[allow(dead_code)]
pub fn prune_waiter(order_id: &str, request_id: u64) {
    let key = format!("{order_id}:{request_id}");
    WAITERS.write().remove(&key);
}
