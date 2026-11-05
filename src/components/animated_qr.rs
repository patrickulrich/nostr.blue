//! NUT-16 animated QR display for Cashu tokens.
//!
//! Tokens that fit a single QR frame render as a static code of the plain
//! `cashuA…`/`cashuB…` string (universally scannable, including by wallets
//! without NUT-16 support). Larger tokens are fountain-encoded as
//! `ur:bytes` fragments (UR payload = CBOR byte string of the serialized
//! token — the de-facto standard cashu.me and the cdk reference
//! implementation use) and cycled frame by frame until the receiver
//! completes. Frame parameters follow the reference implementations:
//! ~150 ms per frame, fragment size tuned for scanner reliability.

#[cfg(feature = "cashu")]
use dioxus::prelude::*;
#[cfg(feature = "cashu")]
use std::str::FromStr as _;

/// Default milliseconds a fragment stays on screen before advancing.
/// cashu.me's default; fast enough for fountain codes to complete quickly,
/// slow enough for handheld scanners to lock on.
#[cfg(feature = "cashu")]
const FRAME_INTERVAL_MS: u32 = 150;

/// Render one QR code as an SVG string (same style as the invoice QR).
#[cfg(feature = "cashu")]
pub fn generate_qr_svg(data: &str) -> String {
    match qrcode::QrCode::new(data) {
        Ok(code) => {
            use qrcode::render::svg;
            code.render::<svg::Color>()
                .min_dimensions(220, 220)
                .dark_color(svg::Color("#000000"))
                .light_color(svg::Color("#ffffff"))
                .build()
        }
        Err(_) => "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"220\" height=\"220\" viewBox=\"0 0 220 220\"><rect width=\"220\" height=\"220\" fill=\"#ffffff\"/><text x=\"110\" y=\"110\" text-anchor=\"middle\" fill=\"#666666\" font-size=\"12\">QR too dense</text></svg>".to_string(),
    }
}

/// NUT-16 animated QR for a Cashu token.
///
/// Falls back to a single static frame when the token fits one QR. Shows
/// fragment progress (frame i of n); the fountain stream loops indefinitely,
/// so receivers can join mid-animation. On any encode failure the signals
/// stay empty — the caller's token string and copy button remain the
/// always-available channel.
#[cfg(feature = "cashu")]
#[component]
pub fn AnimatedQr(token: String) -> Element {
    let mut current_frame = use_signal(String::new);
    let mut frame_index = use_signal(|| 0usize);
    let mut fragment_count = use_signal(|| 0usize);
    let mut generation = use_signal(|| 0u64);

    use_effect(use_reactive(&token, move |token: String| {
        // Retire any previous animation loop, then start a fresh one.
        let my_gen = generation() + 1;
        generation.set(my_gen);
        current_frame.set(String::new());
        frame_index.set(0);
        fragment_count.set(0);

        spawn(async move {
            #[cfg(feature = "cashu")]
            {
                let parsed = match cdk::nuts::Token::from_str(&token) {
                    Ok(t) => t,
                    Err(_) => return,
                };                let mut encoder = match cdk::nuts::TokenUrEncoder::new(
                    &parsed,
                    cdk::nuts::nut16::DEFAULT_MAX_FRAGMENT_LENGTH,
                ) {
                    Ok(e) => e,
                    Err(_) => return,
                };
                let count = encoder.fragment_count();
                fragment_count.set(count);

                if count <= 1 {
                    // Plain single-frame token: display the raw string, not
                    // the UR form — scannable by every wallet, NUT-16 or not.
                    current_frame.set(token.clone());
                    frame_index.set(1);
                    return;
                }

                loop {
                    if generation() != my_gen {
                        break;
                    }
                    let part = match encoder.next_part() {
                        Ok(p) => p,
                        Err(_) => break,
                    };
                    let idx = encoder.current_index();
                    current_frame.set(part);
                    frame_index.set(idx);
                    crate::platform::timer::sleep_ms(FRAME_INTERVAL_MS).await;
                }
            }
            #[cfg(not(feature = "cashu"))]
            {
                let _ = token;
            }
        });
    }));

    let frame = current_frame();
    let svg = use_memo(use_reactive(&frame, |frame: String| generate_qr_svg(&frame)));

    rsx! {
        div { class: "flex flex-col items-center gap-2",
            div {
                class: "bg-white rounded-lg p-3 border border-border",
                dangerous_inner_html: "{svg}",
            }
            if fragment_count() > 1 {
                div { class: "text-xs text-muted-foreground",
                    "frame {frame_index()} / {fragment_count()} — keep scanning until it completes"
                }
            }
        }
    }
}
