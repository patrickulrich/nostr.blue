use dioxus::prelude::*;

pub static ONLINE_STATUS: GlobalSignal<bool> = Signal::global(|| true);

pub async fn setup_online_status() {
    #[cfg(feature = "web")]
    {
        let _ = document::eval(r#"
            if (!window.__nostrBlueOnlineReady) {
                window.addEventListener("online", () => { window.__nostrBlueOnline = true; });
                window.addEventListener("offline", () => { window.__nostrBlueOnline = false; });
                window.__nostrBlueOnlineReady = true;
                window.__nostrBlueOnline = navigator.onLine;
            }
        "#).await;

        let online: bool = document::eval("return navigator.onLine")
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        *ONLINE_STATUS.write() = online;

        poll_js_online().await;
    }

    #[cfg(not(feature = "web"))]
    {
        *ONLINE_STATUS.write() = true;
    }
}

#[cfg(feature = "web")]
async fn poll_js_online() {
    loop {
        crate::platform::timer::sleep_ms(5000).await;
        let online: bool = document::eval(
            "return window.__nostrBlueOnline !== undefined ? window.__nostrBlueOnline : navigator.onLine",
        )
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
        *ONLINE_STATUS.write() = online;
    }
}
