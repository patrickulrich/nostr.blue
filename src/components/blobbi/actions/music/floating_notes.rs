use dioxus::prelude::*;

#[derive(Clone, Copy)]
struct NoteConfig {
    note: &'static str,
    duration: f64,
    left: f64,
    delay: f64,
}

const NOTE_CONFIGS: &[NoteConfig] = &[
    NoteConfig { note: "\u{266A}", duration: 2.0, left: 10.0, delay: 0.0 },
    NoteConfig { note: "\u{266B}", duration: 2.5, left: 30.0, delay: 0.4 },
    NoteConfig { note: "\u{266C}", duration: 3.0, left: 50.0, delay: 0.8 },
    NoteConfig { note: "\u{2669}", duration: 2.2, left: 70.0, delay: 1.2 },
    NoteConfig { note: "\u{266A}", duration: 2.8, left: 85.0, delay: 0.6 },
    NoteConfig { note: "\u{266B}", duration: 3.2, left: 20.0, delay: 1.0 },
];

#[component]
pub fn FloatingMusicNotes(active: bool) -> Element {
    if !active {
        return rsx! {};
    }

    rsx! {
        div { class: "absolute inset-0 overflow-hidden pointer-events-none",
            style { {BLOBBI_NOTE_CSS} }

            for (i, cfg) in NOTE_CONFIGS.iter().enumerate() {
                div {
                    key: "{i}",
                    class: "blobbi-floating-note absolute text-xs text-muted-foreground/50",
                    style: "left: {cfg.left}%; bottom: 0; animation-duration: {cfg.duration}s; animation-delay: {cfg.delay}s;",
                    "{cfg.note}"
                }
            }
        }
    }
}

const BLOBBI_NOTE_CSS: &str = r#"
@keyframes blobbi-note-float {
    0%   { transform: translateY(0) rotate(0deg); opacity: 0.7; }
    100% { transform: translateY(-40px) rotate(20deg); opacity: 0; }
}
.blobbi-floating-note {
    animation: blobbi-note-float ease-in-out infinite;
}
"#;
