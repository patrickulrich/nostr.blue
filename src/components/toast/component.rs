use dioxus::prelude::*;
use dioxus_primitives::toast::{self, ToastOptions, ToastProviderProps};
use std::time::Duration;

pub fn show_queued_toast(toast_api: dioxus_primitives::toast::Toasts, event_label: &str) {
    toast_api.info(
        format!("{} queued", event_label),
        ToastOptions::new()
            .description("View publish queue →")
            .duration(Duration::from_secs(4)),
    );
}

#[component]
pub fn ToastProvider(props: ToastProviderProps) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: asset!("./style.css") }
        toast::ToastProvider {
            default_duration: props.default_duration,
            max_toasts: props.max_toasts,
            render_toast: props.render_toast,
            {props.children}
        }
    }
}
