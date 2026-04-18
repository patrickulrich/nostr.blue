use crate::stores::media::LightboxImage;
use dioxus::prelude::*;
use url::Url;

fn redact_url_for_logging(input: &str) -> String {
    if let Ok(mut url) = Url::parse(input) {
        if !matches!(url.scheme(), "http" | "https") {
            return "[redacted]".to_string();
        }
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_query(None);
        url.set_fragment(None);
        return url.to_string();
    }

    let sanitized = input.split(['?', '#']).next().unwrap_or(input);
    const MAX_FALLBACK_LEN: usize = 120;
    let truncated_chars = sanitized
        .chars()
        .take(MAX_FALLBACK_LEN + 1)
        .collect::<Vec<_>>();
    let mut truncated = truncated_chars
        .iter()
        .take(MAX_FALLBACK_LEN)
        .collect::<String>();
    if truncated_chars.len() > MAX_FALLBACK_LEN {
        truncated.push('…');
    }
    truncated
}

#[derive(Props, Clone, PartialEq)]
pub struct ImageGridProps {
    pub images: Vec<LightboxImage>,
    pub on_open: EventHandler<usize>,
}

#[component]
pub fn ImageGrid(props: ImageGridProps) -> Element {
    if props.images.is_empty() {
        return rsx! {};
    }

    let visible_count = props.images.len().min(4);
    let hidden_count = props.images.len().saturating_sub(4);
    let visible_images = &props.images[..visible_count];

    rsx! {
        match props.images.len() {
            1 => rsx! {
                div { class: "my-2",
                    ImageTile {
                        image: visible_images[0].clone(),
                        index: 0,
                        class: String::new(),
                        natural: true,
                        overlay_text: None,
                        on_open: props.on_open,
                    }
                }
            },
            2 => rsx! {
                div { class: "my-2 grid grid-cols-2 gap-2",
                    for (index, image) in visible_images.iter().cloned().enumerate() {
                        ImageTile {
                            key: "grid-2-{index}",
                            image,
                            index,
                            class: "aspect-square w-full".to_string(),
                            overlay_text: None,
                            on_open: props.on_open,
                        }
                    }
                }
            },
            3 => rsx! {
                div { class: "my-2 space-y-2",
                    ImageTile {
                        image: visible_images[0].clone(),
                        index: 0,
                        class: "aspect-[16/9] w-full".to_string(),
                        overlay_text: None,
                        on_open: props.on_open,
                    }
                    div { class: "grid grid-cols-2 gap-2",
                        for (offset, image) in visible_images[1..].iter().cloned().enumerate() {
                            ImageTile {
                                key: "grid-3-{offset}",
                                image,
                                index: offset + 1,
                                class: "aspect-square w-full".to_string(),
                                overlay_text: None,
                                on_open: props.on_open,
                            }
                        }
                    }
                }
            },
            _ => rsx! {
                div { class: "my-2 grid grid-cols-2 gap-2",
                    for (index, image) in visible_images.iter().cloned().enumerate() {
                        ImageTile {
                            key: "grid-many-{index}",
                            image,
                            index,
                            class: "aspect-square w-full".to_string(),
                            overlay_text: if hidden_count > 0 && index == visible_count - 1 {
                                Some(format!("+{}", hidden_count))
                            } else {
                                None
                            },
                            on_open: props.on_open,
                        }
                    }
                }
            },
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ImageTileProps {
    image: LightboxImage,
    index: usize,
    class: String,
    #[props(default)]
    overlay_text: Option<String>,
    #[props(default)]
    natural: bool,
    on_open: EventHandler<usize>,
}

#[component]
fn ImageTile(props: ImageTileProps) -> Element {
    let alt_text = props
        .image
        .alt
        .clone()
        .map(|alt| alt.trim().to_string())
        .filter(|alt| !alt.is_empty())
        .unwrap_or_else(|| format!("Image {}", props.index + 1));
    let url_for_error = redact_url_for_logging(&props.image.url);
    let (outer_class, img_class) = if props.natural {
        (
            "group relative overflow-hidden rounded-lg border border-border cursor-pointer"
                .to_string(),
            "max-w-full h-auto transition duration-200 group-hover:opacity-90".to_string(),
        )
    } else {
        (
            format!(
                "group relative overflow-hidden rounded-lg border border-border bg-muted {}",
                props.class
            ),
            "h-full w-full object-cover transition duration-200 group-hover:scale-[1.02]"
                .to_string(),
        )
    };

    rsx! {
        button {
            r#type: "button",
            class: "{outer_class}",
            onclick: move |evt: MouseEvent| {
                evt.stop_propagation();
                props.on_open.call(props.index);
            },
            img {
                src: "{props.image.url}",
                alt: "{alt_text}",
                class: "{img_class}",
                loading: "lazy",
                onerror: move |_| {
                    log::warn!("Failed to load image: {}", url_for_error);
                },
            }
            if let Some(overlay) = props.overlay_text.clone() {
                div { class: "absolute inset-0 flex items-center justify-center bg-black/55 text-2xl font-semibold text-white",
                    "{overlay}"
                }
            }
        }
    }
}
