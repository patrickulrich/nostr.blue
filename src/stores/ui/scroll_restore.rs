use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct ScrollAnchor {
    pub scroll_y: f64,
    pub feed_type_label: String,
    pub is_set: bool,
}

pub static HOME_SCROLL_ANCHOR: GlobalSignal<ScrollAnchor> = Signal::global(ScrollAnchor::default);

pub static SCROLL_POSITIONS: GlobalSignal<HashMap<String, f64>> =
    Signal::global(HashMap::new);

pub fn save_scroll(route_key: &str, y: f64) {
    SCROLL_POSITIONS.write().insert(route_key.to_string(), y);
}

pub fn get_scroll(route_key: &str) -> Option<f64> {
    SCROLL_POSITIONS.peek().get(route_key).copied()
}

pub async fn setup_scroll_tracker() {
    let _ = document::eval(r#"
        if (!window.__nostrBlueScrollReady) {
            window.__nostrBlueLastScrollY = 0;
            window.addEventListener("scroll", () => {
                window.__nostrBlueLastScrollY = window.scrollY;
            }, { passive: true });
            window.__nostrBlueScrollReady = true;
        }
    "#).await;
}

pub async fn get_tracked_scroll_y() -> f64 {
    document::eval("return window.__nostrBlueLastScrollY || 0")
        .await
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

pub async fn setup_popstate_flag() {
    let _ = document::eval(r#"
        if (!window.__nostrBluePopstateReady) {
            window.__nostrBlueWasPopstate = false;
            window.addEventListener("popstate", () => {
                window.__nostrBlueWasPopstate = true;
            });
            window.__nostrBluePopstateReady = true;
        }
    "#).await;
}

pub async fn was_popstate_nav() -> bool {
    document::eval("return window.__nostrBlueWasPopstate === true")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

pub async fn clear_popstate_flag() {
    let _ = document::eval("window.__nostrBlueWasPopstate = false").await;
}

pub async fn get_scroll_y() -> f64 {
    let result = document::eval("return window.scrollY").await;
    result.ok().and_then(|v| v.as_f64()).unwrap_or(0.0)
}

pub async fn set_scroll_y(y: f64) {
    let _ = document::eval(&format!("window.scrollTo(0, {})", y as i64)).await;
}
