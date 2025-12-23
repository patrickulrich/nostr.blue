//! ImageCarousel component - product image gallery

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ImageCarouselProps {
    pub images: Vec<String>,
    #[props(default)]
    pub alt: Option<String>,
}

/// Image carousel with thumbnails
#[component]
pub fn ImageCarousel(props: ImageCarouselProps) -> Element {
    let mut current_index = use_signal(|| 0usize);
    let images = props.images.clone();
    let alt_text = props.alt.clone().unwrap_or_default();

    // Reset index when images prop changes to avoid stale index
    // Capture images by move so the closure recomputes when props change
    {
        let images_for_effect = images.clone();
        use_effect(move || {
            let image_count = images_for_effect.len();
            let current = *current_index.peek();
            if image_count == 0 {
                current_index.set(0);
            } else if current >= image_count {
                current_index.set(image_count - 1);
            }
        });
    }

    if images.is_empty() {
        return rsx! {
            div { class: "aspect-square bg-muted rounded-lg flex items-center justify-center",
                div { class: "text-6xl text-muted-foreground", "📦" }
            }
        };
    }

    let current = *current_index.read();
    let image_count = images.len();
    let current_image = images.get(current).cloned().unwrap_or_default();

    rsx! {
        div { class: "space-y-3",
            // Main image
            div { class: "aspect-square bg-muted rounded-lg overflow-hidden relative",
                img {
                    src: "{current_image}",
                    class: "w-full h-full object-contain",
                    alt: "{alt_text}"
                }

                // Navigation arrows (if multiple images)
                if image_count > 1 {
                    // Previous
                    button {
                        class: "absolute left-2 top-1/2 -translate-y-1/2 w-10 h-10 bg-black/50 hover:bg-black/70 text-white rounded-full flex items-center justify-center transition",
                        onclick: move |_| {
                            let idx = *current_index.read();
                            current_index.set((idx + image_count - 1) % image_count);
                        },
                        "‹"
                    }

                    // Next
                    button {
                        class: "absolute right-2 top-1/2 -translate-y-1/2 w-10 h-10 bg-black/50 hover:bg-black/70 text-white rounded-full flex items-center justify-center transition",
                        onclick: move |_| {
                            let idx = *current_index.read();
                            current_index.set((idx + 1) % image_count);
                        },
                        "›"
                    }

                    // Indicator dots
                    div { class: "absolute bottom-2 left-1/2 -translate-x-1/2 flex gap-1.5",
                        for i in 0..image_count {
                            button {
                                key: "{i}",
                                class: if i == current {
                                    "w-2 h-2 rounded-full bg-white"
                                } else {
                                    "w-2 h-2 rounded-full bg-white/50 hover:bg-white/75 transition"
                                },
                                onclick: move |_| current_index.set(i)
                            }
                        }
                    }
                }
            }

            // Thumbnails (if multiple images)
            if image_count > 1 {
                div { class: "flex gap-2 overflow-x-auto pb-2",
                    for (i, img) in images.iter().enumerate() {
                        button {
                            key: "{i}",
                            class: if i == current {
                                "w-16 h-16 flex-shrink-0 rounded-lg overflow-hidden ring-2 ring-blue-500"
                            } else {
                                "w-16 h-16 flex-shrink-0 rounded-lg overflow-hidden opacity-70 hover:opacity-100 transition"
                            },
                            onclick: move |_| current_index.set(i),
                            img {
                                src: "{img}",
                                class: "w-full h-full object-cover"
                            }
                        }
                    }
                }
            }
        }
    }
}
