use crate::services::nip05::{self, Nip05Status};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct Nip05BadgeProps {
    pub pubkey: String,
    pub nip05: String,
}

#[component]
pub fn Nip05Badge(props: Nip05BadgeProps) -> Element {
    let mut status = use_signal(|| Nip05Status::Unknown);
    let pubkey_for_effect = props.pubkey.clone();
    let nip05_for_effect = props.nip05.clone();

    use_effect(use_reactive(
        (&props.pubkey, &props.nip05),
        move |(pubkey, nip05)| {
            let current = nip05::get_nip05_status(&pubkey, &nip05);
            status.set(current.clone());
            if matches!(current, Nip05Status::Unknown) {
                nip05::verify_nip05(&pubkey, &nip05);
            }
        },
    ));

    use_future(move || {
        let pk = pubkey_for_effect.clone();
        let nip = nip05_for_effect.clone();
        async move {
            loop {
                crate::platform::timer::sleep_ms(200).await;
                let current = nip05::get_nip05_status(&pk, &nip);
                if *status.read() != current {
                    status.set(current.clone());
                }
                if matches!(&current, Nip05Status::Verified | Nip05Status::Impersonator) {
                    break;
                }
            }
        }
    });

    let s = status.read().clone();
    match s {
        Nip05Status::Verified => rsx! {
            span {
                class: "inline-flex items-center gap-1 text-green-500",
                title: "NIP-05 verified",
                svg {
                    class: "w-4 h-4",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M22 11.08V12a10 10 0 1 1-5.93-9.14" }
                    polyline { points: "22 4 12 14.01 9 11.01" }
                }
            }
        },
        Nip05Status::Impersonator => rsx! {
            span {
                class: "inline-flex items-center gap-1 text-red-500",
                title: "NIP-05 verification failed: pubkey mismatch",
                svg {
                    class: "w-4 h-4",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "12", cy: "12", r: "10" }
                    line { x1: "15", y1: "9", x2: "9", y2: "15" }
                    line { x1: "9", y1: "9", x2: "15", y2: "15" }
                }
            }
        },
        Nip05Status::Error => {
            let pk = props.pubkey.clone();
            let nip = props.nip05.clone();
            rsx! {
                span {
                    class: "inline-flex items-center gap-1 text-orange-500 cursor-pointer",
                    title: "NIP-05 verification failed. Click to retry.",
                    onclick: move |_| {
                        nip05::retry_nip05(&pk, &nip);
                    },
                    svg {
                        class: "w-4 h-4",
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" }
                        line { x1: "12", y1: "9", x2: "12", y2: "13" }
                        line { x1: "12", y1: "17", x2: "12.01", y2: "17" }
                    }
                }
            }
        }
        Nip05Status::Verifying => rsx! {
            span {
                class: "inline-flex items-center",
                span { class: "inline-block w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin text-muted-foreground" }
            }
        },
        Nip05Status::Unknown => rsx! {},
    }
}
