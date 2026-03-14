use super::GalleryIndicator;
use crate::stores::media;
use dioxus::html::input_data::keyboard_types::Key;
use dioxus::prelude::*;

fn clamp_zoom(zoom: f64) -> f64 {
    zoom.clamp(0.5, 5.0)
}

fn clamp_pan_component(value: f64, zoom: f64, container_extent: f64) -> f64 {
    if zoom <= 1.0 {
        0.0
    } else {
        let max_offset = ((zoom - 1.0) * container_extent * 0.5).max(0.0);
        value.clamp(-max_offset, max_offset)
    }
}

fn contain_fit_extent(intrinsic: (f64, f64), viewport: (f64, f64)) -> (f64, f64) {
    let (intrinsic_w, intrinsic_h) = intrinsic;
    let (viewport_w, viewport_h) = viewport;
    if intrinsic_w <= 0.0 || intrinsic_h <= 0.0 || viewport_w <= 0.0 || viewport_h <= 0.0 {
        return viewport;
    }

    let scale = (viewport_w / intrinsic_w).min(viewport_h / intrinsic_h);
    (intrinsic_w * scale, intrinsic_h * scale)
}

fn clamp_pan(x: f64, y: f64, zoom: f64, intrinsic: (f64, f64), viewport: (f64, f64)) -> (f64, f64) {
    let displayed = contain_fit_extent(intrinsic, viewport);
    (
        clamp_pan_component(x, zoom, displayed.0),
        clamp_pan_component(y, zoom, displayed.1),
    )
}

fn distance_between(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    (dx * dx + dy * dy).sqrt()
}

#[component]
pub fn MediaLightbox() -> Element {
    let state = media::LIGHTBOX_STATE.read().clone();
    let mut zoom_level = use_signal(|| 1.0f64);
    let mut pan_offset = use_signal(|| (0.0f64, 0.0f64));
    let mut is_dragging = use_signal(|| false);
    let mut drag_origin = use_signal(|| (0.0f64, 0.0f64));
    let mut swipe_start = use_signal(|| None::<(f64, f64)>);
    let mut pinch_start_distance = use_signal(|| None::<f64>);
    let mut pinch_start_zoom = use_signal(|| 1.0f64);
    let mut image_intrinsic_size = use_signal(|| (1200.0f64, 800.0f64));
    #[cfg(feature = "web")]
    let mut viewport_size = use_signal(|| (1200.0f64, 800.0f64));
    #[cfg(feature = "native")]
    let viewport_size = use_signal(|| (1200.0f64, 800.0f64));

    use_effect(use_reactive(
        &(state.is_open, state.current_index),
        move |_| {
            zoom_level.set(1.0);
            pan_offset.set((0.0, 0.0));
            is_dragging.set(false);
            swipe_start.set(None);
            pinch_start_distance.set(None);
            image_intrinsic_size.set((1200.0, 800.0));
        },
    ));

    if !state.is_open || state.images.is_empty() {
        return rsx! {};
    }

    let current_index = state
        .current_index
        .min(state.images.len().saturating_sub(1));
    let current_image = state.images[current_index].clone();
    let current_alt = current_image
        .alt
        .clone()
        .unwrap_or_else(|| format!("Image {}", current_index + 1));

    let previous_url = current_index
        .checked_sub(1)
        .and_then(|idx| state.images.get(idx))
        .map(|image| image.url.clone());
    let next_url = state
        .images
        .get(current_index + 1)
        .map(|image| image.url.clone());

    let transform = format!(
        "transform: translate({:.2}px, {:.2}px) scale({:.3});",
        pan_offset.read().0,
        pan_offset.read().1,
        *zoom_level.read(),
    );

    let navigate_to = move |index: usize| {
        media::set_index(index);
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[120] bg-black/90 backdrop-blur-sm flex items-center justify-center p-4",
            onclick: move |_| media::close_lightbox(),
            div {
                class: "relative flex h-full w-full max-w-7xl flex-col items-center justify-center",
                role: "dialog",
                aria_modal: "true",
                aria_label: "Image viewer",
                tabindex: "-1",
                onclick: move |evt| evt.stop_propagation(),
                onmounted: move |evt| async move {
                    let _ = evt.set_focus(true).await;
                    #[cfg(feature = "web")]
                    {
                        if let Some(element) = evt.data().downcast::<web_sys::HtmlElement>() {
                            let rect = element.get_bounding_client_rect();
                            viewport_size.set((rect.width(), rect.height()));
                        }
                    }
                },
                onkeydown: move |evt: KeyboardEvent| {
                    match evt.key() {
                        Key::Escape => {
                            evt.stop_propagation();
                            media::close_lightbox();
                        }
                        Key::ArrowLeft => {
                            evt.stop_propagation();
                            evt.prevent_default();
                            media::prev_image();
                        }
                        Key::ArrowRight => {
                            evt.stop_propagation();
                            evt.prevent_default();
                            media::next_image();
                        }
                        Key::Home => {
                            evt.stop_propagation();
                            evt.prevent_default();
                            navigate_to(0);
                        }
                        Key::End => {
                            evt.stop_propagation();
                            evt.prevent_default();
                            navigate_to(state.images.len().saturating_sub(1));
                        }
                        _ => {}
                    }
                },
                button {
                    class: "absolute right-0 top-0 z-10 rounded-full bg-black/60 px-3 py-2 text-xl text-white hover:bg-black/80 transition",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        media::close_lightbox();
                    },
                    "aria-label": "Close image viewer",
                    "×"
                }
                if current_index > 0 {
                    button {
                        class: "absolute left-0 top-1/2 z-10 -translate-y-1/2 rounded-full bg-black/60 px-4 py-3 text-2xl text-white hover:bg-black/80 transition",
                        onclick: move |evt: MouseEvent| {
                            evt.stop_propagation();
                            media::prev_image();
                        },
                        "aria-label": "Previous image",
                        "‹"
                    }
                }
                if current_index + 1 < state.images.len() {
                    button {
                        class: "absolute right-0 top-1/2 z-10 -translate-y-1/2 rounded-full bg-black/60 px-4 py-3 text-2xl text-white hover:bg-black/80 transition",
                        onclick: move |evt: MouseEvent| {
                            evt.stop_propagation();
                            media::next_image();
                        },
                        "aria-label": "Next image",
                        "›"
                    }
                }
                div {
                    class: "flex min-h-0 flex-1 w-full items-center justify-center overflow-hidden",
                    onwheel: move |evt: WheelEvent| {
                        evt.stop_propagation();
                        evt.prevent_default();
                        let delta_y = evt.delta().strip_units().y;
                        let next_zoom = clamp_zoom(*zoom_level.read() - delta_y * 0.0015);
                        zoom_level.set(next_zoom);
                        let (pan_x, pan_y) = clamp_pan(
                            pan_offset.read().0,
                            pan_offset.read().1,
                            next_zoom,
                            *image_intrinsic_size.read(),
                            *viewport_size.read(),
                        );
                        pan_offset.set((pan_x, pan_y));
                    },
                    ondoubleclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let next_zoom = if *zoom_level.read() > 1.0 { 1.0 } else { 2.0 };
                        zoom_level.set(next_zoom);
                        let (pan_x, pan_y) = clamp_pan(
                            0.0,
                            0.0,
                            next_zoom,
                            *image_intrinsic_size.read(),
                            *viewport_size.read(),
                        );
                        pan_offset.set((pan_x, pan_y));
                    },
                    onpointerdown: move |evt: PointerEvent| {
                        evt.stop_propagation();
                        if *zoom_level.read() <= 1.0 {
                            return;
                        }
                        is_dragging.set(true);
                        let coords = evt.client_coordinates();
                        drag_origin.set((
                            coords.x - pan_offset.read().0,
                            coords.y - pan_offset.read().1,
                        ));
                    },
                    onpointermove: move |evt: PointerEvent| {
                        if !*is_dragging.read() {
                            return;
                        }
                        evt.stop_propagation();
                        let coords = evt.client_coordinates();
                        let next_pan = (
                            coords.x - drag_origin.read().0,
                            coords.y - drag_origin.read().1,
                        );
                        pan_offset.set(clamp_pan(
                            next_pan.0,
                            next_pan.1,
                            *zoom_level.read(),
                            *image_intrinsic_size.read(),
                            *viewport_size.read(),
                        ));
                    },
                    onpointerup: move |_| {
                        is_dragging.set(false);
                    },
                    onpointercancel: move |_| {
                        is_dragging.set(false);
                    },
                    ontouchstart: move |evt| {
                        evt.stop_propagation();
                        let touches = evt.touches();
                        match touches.len() {
                            1 => {
                                let coords = touches[0].client_coordinates();
                                swipe_start.set(Some((coords.x, coords.y)));
                                pinch_start_distance.set(None);
                            }
                            2 => {
                                let first = touches[0].client_coordinates();
                                let second = touches[1].client_coordinates();
                                pinch_start_distance
                                    .set(Some(distance_between((first.x, first.y), (second.x, second.y))));
                                pinch_start_zoom.set(*zoom_level.read());
                            }
                            _ => {}
                        }
                    },
                    ontouchmove: move |evt| {
                        let touches = evt.touches();
                        if touches.len() == 2 {
                            evt.stop_propagation();
                            evt.prevent_default();
                            let first = touches[0].client_coordinates();
                            let second = touches[1].client_coordinates();
                            let distance = distance_between((first.x, first.y), (second.x, second.y));
                            if let Some(start_distance) = *pinch_start_distance.read() {
                                if start_distance > 0.0 {
                                    let next_zoom =
                                        clamp_zoom(*pinch_start_zoom.read() * (distance / start_distance));
                                    let current_pan = *pan_offset.read();
                                    let viewport = *viewport_size.read();
                                    zoom_level.set(next_zoom);
                                    pan_offset.set(clamp_pan(
                                        current_pan.0,
                                        current_pan.1,
                                        next_zoom,
                                        *image_intrinsic_size.read(),
                                        viewport,
                                    ));
                                }
                            }
                        } else if touches.len() == 1 && *zoom_level.read() > 1.0 {
                            evt.stop_propagation();
                            evt.prevent_default();
                            let coords = touches[0].client_coordinates();
                            let swipe_origin = {
                                let swipe_origin = swipe_start.read();
                                *swipe_origin
                            };
                            let (start_x, start_y) = if let Some((start_x, start_y)) = swipe_origin {
                                (start_x, start_y)
                            } else {
                                swipe_start.set(Some((coords.x, coords.y)));
                                (coords.x, coords.y)
                            };
                            let delta_x = coords.x - start_x;
                            let delta_y = coords.y - start_y;
                            let current_pan = *pan_offset.read();
                            let new_pan = (current_pan.0 + delta_x, current_pan.1 + delta_y);
                            pan_offset.set(clamp_pan(
                                new_pan.0,
                                new_pan.1,
                                *zoom_level.read(),
                                *image_intrinsic_size.read(),
                                *viewport_size.read(),
                            ));
                            swipe_start.set(Some((coords.x, coords.y)));
                        } else if touches.len() == 1 && *zoom_level.read() <= 1.0 {
                            let swipe_origin = *swipe_start.read();
                            if let Some((start_x, start_y)) = swipe_origin {
                                let coords = touches[0].client_coordinates();
                                let delta_x = coords.x - start_x;
                                let delta_y = coords.y - start_y;
                                if delta_x.abs() > 60.0 && delta_y.abs() < 80.0 {
                                    evt.stop_propagation();
                                    evt.prevent_default();
                                    if delta_x > 0.0 {
                                        media::prev_image();
                                    } else {
                                        media::next_image();
                                    }
                                    swipe_start.set(None);
                                }
                            }
                        }
                    },
                    ontouchend: move |_| {
                        swipe_start.set(None);
                        pinch_start_distance.set(None);
                    },
                    img {
                        src: "{current_image.url}",
                        alt: "{current_alt}",
                        class: "max-h-full max-w-full select-none object-contain transition-transform duration-100 ease-out",
                        draggable: "false",
                        style: "{transform}",
                        onmounted: move |_evt| async move {
                            #[cfg(feature = "web")]
                            {
                                if let Some(element) = _evt.data().downcast::<web_sys::HtmlElement>() {
                                    let rect = element.get_bounding_client_rect();
                                    if rect.width() > 0.0 && rect.height() > 0.0 {
                                        viewport_size.set((rect.width(), rect.height()));
                                    }
                                    let natural_width = js_sys::Reflect::get(
                                        element.as_ref(),
                                        &wasm_bindgen::JsValue::from_str("naturalWidth"),
                                    )
                                    .ok()
                                    .and_then(|value| value.as_f64())
                                    .unwrap_or(0.0);
                                    let natural_height = js_sys::Reflect::get(
                                        element.as_ref(),
                                        &wasm_bindgen::JsValue::from_str("naturalHeight"),
                                    )
                                    .ok()
                                    .and_then(|value| value.as_f64())
                                    .unwrap_or(0.0);
                                    if natural_width > 0.0 && natural_height > 0.0 {
                                        image_intrinsic_size.set((natural_width, natural_height));
                                    }
                                }
                            }
                        },
                    }
                }
                div { class: "mt-4 flex flex-col items-center gap-3 text-white",
                    div { class: "text-sm text-white/80",
                        "{current_index + 1} / {state.images.len()}"
                    }
                    GalleryIndicator {
                        current_index,
                        count: state.images.len(),
                        on_select: media::set_index,
                    }
                }
                if let Some(url) = previous_url {
                    img { class: "hidden", src: "{url}", alt: "", aria_hidden: "true" }
                }
                if let Some(url) = next_url {
                    img { class: "hidden", src: "{url}", alt: "", aria_hidden: "true" }
                }
            }
        }
    }
}
