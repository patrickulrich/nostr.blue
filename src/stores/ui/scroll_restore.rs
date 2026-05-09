use dioxus::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct ScrollAnchor {
    pub scroll_y: f64,
    pub feed_type_label: String,
    pub is_set: bool,
}

pub static HOME_SCROLL_ANCHOR: GlobalSignal<ScrollAnchor> = Signal::global(ScrollAnchor::default);

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
