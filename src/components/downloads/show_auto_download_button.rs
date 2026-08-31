//! Per-show auto-download toggle for podcast show pages (native only).
//!
//! Sits next to the Share button on the RSS show page. Enabling records the
//! flag and immediately enqueues the newest `episodes_per_show` episodes
//! (Settings → Downloads); disabling stops future auto-downloads but keeps
//! already-downloaded files.

#[cfg(feature = "native")]
use crate::components::icons;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ShowAutoDownloadButtonProps {
    pub feed_url: String,
    pub title: Option<String>,
    pub image: Option<String>,
    /// Extra classes for the button element.
    #[props(default)]
    pub class: String,
}

#[component]
pub fn ShowAutoDownloadButton(props: ShowAutoDownloadButtonProps) -> Element {
    #[cfg(not(feature = "native"))]
    {
        let _ = props;
        return rsx! {};
    }
    #[cfg(feature = "native")]
    {
        let feed_url = props.feed_url.clone();
        let mut enabled = use_resource({
            let feed_url = feed_url.clone();
            move || {
                let feed_url = feed_url.clone();
                async move {
                    crate::stores::downloads::sync::show_auto_download_enabled(&feed_url).await
                }
            }
        });
        let mut busy = use_signal(|| false);
        let is_on = (*enabled.read()).unwrap_or(false);
        let title_text = if is_on {
            "Auto-download ON — tap to stop downloading new episodes"
        } else {
            "Auto-download OFF — tap to download new episodes automatically"
        };
        let is_busy = *busy.read();
        let icon_html = if is_on {
            icons::CHECK_CIRCLE
        } else {
            icons::DOWNLOAD
        };
        let btn_class = if is_on {
            format!(
                "p-2 rounded-full transition text-primary hover:bg-muted {}",
                props.class
            )
        } else {
            format!("p-2 hover:bg-muted rounded-full transition {}", props.class)
        };
        rsx! {
            button {
                class: "{btn_class}",
                title: "{title_text}",
                disabled: is_busy,
                onclick: {
                    let feed_url = props.feed_url.clone();
                    let title = props.title.clone();
                    let image = props.image.clone();
                    move |_| {
                        if *busy.read() {
                            return;
                        }
                        busy.set(true);
                        let feed_url = feed_url.clone();
                        let title = title.clone();
                        let image = image.clone();
                        let next = !is_on;
                        spawn(async move {
                            let result = crate::stores::downloads::sync::toggle_show_auto_download(
                                &feed_url,
                                title.as_deref(),
                                image.as_deref(),
                                next,
                            )
                            .await;
                            if let Err(e) = result {
                                log::error!("Failed to toggle auto-download: {}", e);
                            }
                            enabled.restart();
                            busy.set(false);
                        });
                    }
                },
                dangerous_inner_html: icon_html,
            }
        }
    }
}
