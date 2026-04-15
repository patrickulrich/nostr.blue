use dioxus::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct ScrollAnchor {
    pub scroll_y: f64,
    pub feed_type_label: String,
    pub is_set: bool,
}

pub static HOME_SCROLL_ANCHOR: GlobalSignal<ScrollAnchor> = Signal::global(ScrollAnchor::default);

#[allow(dead_code)]
pub async fn restore_scroll_position(current_feed_type_label: &str) -> bool {
    let anchor = HOME_SCROLL_ANCHOR.read();
    if !anchor.is_set || anchor.feed_type_label != current_feed_type_label {
        return false;
    }
    let scroll_y = anchor.scroll_y;
    drop(anchor);
    crate::platform::timer::sleep_ms(100).await;
    set_scroll_y(scroll_y).await;
    HOME_SCROLL_ANCHOR.write().is_set = false;
    log::debug!("Restored scroll position: y={}", scroll_y);
    true
}

pub async fn get_scroll_y() -> f64 {
    let result = document::eval("return window.scrollY").await;
    result.ok().and_then(|v| v.as_f64()).unwrap_or(0.0)
}

pub async fn set_scroll_y(y: f64) {
    let _ = document::eval(&format!("window.scrollTo(0, {})", y as i64)).await;
}
