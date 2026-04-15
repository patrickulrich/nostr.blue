use dioxus::prelude::*;

#[component]
pub fn SingEffect() -> Element {
    let mut visible = use_signal(|| true);

    rsx! {
        if visible() {
            div {
                class: "fixed inset-0 z-[200] pointer-events-none flex items-center justify-center",
                div {
                    class: "animate-[ping_1s_ease-out_forwards]",
                    onclick: move |_| visible.set(false),

                    div { class: "relative",
                        div { class: "text-6xl animate-[bounce_0.5s_ease-in-out_infinite]",
                            "🎤"
                        }
                        div { class: "absolute -top-4 left-2 text-2xl animate-[bounce_0.6s_ease-in-out_infinite_0.1s]",
                            "♪"
                        }
                        div { class: "absolute -top-6 left-10 text-xl animate-[bounce_0.7s_ease-in-out_infinite_0.2s]",
                            "♫"
                        }
                        div { class: "absolute -top-3 -left-4 text-xl animate-[bounce_0.8s_ease-in-out_infinite_0.3s]",
                            "♬"
                        }
                        div { class: "absolute -top-8 left-6 text-lg animate-[bounce_0.5s_ease-in-out_infinite_0.4s]",
                            "🎵"
                        }
                        div { class: "absolute -top-5 -left-2 text-lg animate-[bounce_0.6s_ease-in-out_infinite_0.5s]",
                            "🎶"
                        }
                    }
                }
            }
        }
    }
}
