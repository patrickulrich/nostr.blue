use dioxus::prelude::*;

#[component]
pub fn EggVisual(base_color: String) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 120 160",
            width: "120",
            height: "160",
            class: "animate-[blobbi-incubation-glow_2s_ease-in-out_infinite]",
            defs {
                radialGradient { id: "egg-gradient", cx: "40%", cy: "35%", r: "60%",
                    stop { offset: "0%", stop_color: "{lighten_color(&base_color, 30)}" }
                    stop { offset: "100%", stop_color: "{base_color}" }
                }
                radialGradient { id: "egg-shine", cx: "35%", cy: "30%", r: "25%",
                    stop { offset: "0%", stop_color: "rgba(255,255,255,0.4)" }
                    stop { offset: "100%", stop_color: "rgba(255,255,255,0)" }
                }
            }
            ellipse {
                cx: "60",
                cy: "85",
                rx: "48",
                ry: "62",
                fill: "url(#egg-gradient)",
                stroke: "rgba(0,0,0,0.1)",
                stroke_width: "1",
            }
            ellipse {
                cx: "60",
                cy: "85",
                rx: "48",
                ry: "62",
                fill: "url(#egg-shine)",
            }
            ellipse {
                cx: "60",
                cy: "85",
                rx: "48",
                ry: "62",
                fill: "none",
                stroke: "rgba(255,255,255,0.15)",
                stroke_width: "0.5",
                transform: "rotate(-15 60 85)",
            }
            animate {
                attribute_name: "ry",
                values: "62;61;62",
                dur: "3s",
                repeat_count: "indefinite",
            }
        }
    }
}

fn lighten_color(hex: &str, percent: u32) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return "#ffffff".to_string();
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);

    let factor = percent as f32 / 100.0;
    let r = ((r as f32 + (255.0 - r as f32) * factor).min(255.0)) as u8;
    let g = ((g as f32 + (255.0 - g as f32) * factor).min(255.0)) as u8;
    let b = ((b as f32 + (255.0 - b as f32) * factor).min(255.0)) as u8;

    format!("#{:02x}{:02x}{:02x}", r, g, b)
}
