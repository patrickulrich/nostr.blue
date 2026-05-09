use dioxus::prelude::*;

#[component]
pub fn TypewriterText(text: String, speed_ms: Option<u64>) -> Element {
    let speed = speed_ms.unwrap_or(35);
    let mut displayed = use_signal(String::new);
    let mut started = use_signal(|| false);

    if !started() && !text.is_empty() {
        started.set(true);
        spawn(async move {
            for ch in text.chars() {
                let mut s = displayed();
                s.push(ch);
                displayed.set(s);
                crate::platform::timer::sleep_ms(speed as u32).await;
            }
        });
    }

    rsx! {
        span { "{displayed}" }
    }
}
