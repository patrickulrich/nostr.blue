use dioxus::prelude::*;

use crate::components::blobbi::core::builders::build_record_event;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::blobbi_visual::BlobbiVisual;

#[component]
pub fn PhotoModal(blobbi: BlobbiCompanion, on_close: EventHandler<()>) -> Element {
    let mut captured = use_signal(|| false);
    let mut caption = use_signal(String::new);
    let mut uploading = use_signal(|| false);

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-card border border-border rounded-2xl p-6 w-[90vw] max-w-md shadow-xl",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-bold", "📸 Photo" }
                    button {
                        class: "p-1 hover:bg-accent rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                if !captured() {
                    div { class: "text-center space-y-4",
                        div {
                            class: "flex justify-center",
                            id: "blobbi-photo-target",
                            div { class: "bg-[#fafafa] rounded-sm shadow-lg",
                                style: "padding: 12px 12px 64px 12px; width: 280px;",

                                div { class: "relative bg-gradient-to-br from-indigo-100 via-purple-50 to-violet-100 rounded-sm overflow-hidden",
                                    style: "height: 220px;",

                                    div { class: "absolute inset-0",
                                        style: "background: radial-gradient(ellipse at center, transparent 50%, rgba(0,0,0,0.08) 100%);",
                                    }

                                    div { class: "flex items-center justify-center h-full",
                                        BlobbiVisual { blobbi: blobbi.clone(), size: Some("160".to_string()) }
                                    }
                                }

                                div { class: "mt-2 text-center",
                                    p { class: "text-sm text-gray-700 font-bold",
                                        style: "font-family: 'Permanent Marker', cursive;",
                                        "{blobbi.display_name()}"
                                    }
                                    p { class: "text-[10px] text-gray-400 mt-0.5",
                                        "{current_date_string()}"
                                    }
                                }
                            }
                        }

                        p { class: "text-xs text-muted-foreground",
                            "Frame your Blobbi for the perfect shot"
                        }

                        button {
                            class: "px-6 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-80 transition",
                            onclick: move |_| captured.set(true),
                            "📸 Take Photo"
                        }
                    }
                } else {
                    div { class: "text-center space-y-4",
                        div {
                            class: "relative inline-block bg-gradient-to-b from-sky-100 to-sky-50 rounded-xl p-4",
                            BlobbiVisual { blobbi: blobbi.clone(), size: Some("200".to_string()) }
                            div { class: "absolute -bottom-1 -right-1 w-8 h-8 bg-card rounded-full border-2 border-border flex items-center justify-center",
                                span { class: "text-sm", "✓" }
                            }
                        }

                        div {
                            label { class: "text-xs text-muted-foreground block mb-1", "Caption" }
                            input {
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-sm",
                                r#type: "text",
                                placeholder: "Write a caption...",
                                value: "{caption}",
                                oninput: move |e| caption.set(e.value()),
                            }
                        }

                        div { class: "flex gap-2",
                            button {
                                class: "flex-1 py-2 bg-muted rounded-lg text-sm hover:bg-accent transition",
                                disabled: uploading(),
                                onclick: move |_| captured.set(false),
                                "Retake"
                            }
                            button {
                                class: "flex-1 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-80 transition",
                                disabled: uploading(),
                                onclick: {
                                    let blobbi = blobbi.clone();
                                    move |_| {
                                        let blobbi = blobbi.clone();
                                        let cap = caption();
                                        uploading.set(true);
                                        spawn(async move {
                                            let content = if cap.is_empty() {
                                                format!("Photo of {}", blobbi.display_name())
                                            } else {
                                                cap
                                            };

                                            let img_url: Option<String> = capture_blobbi_image().await.ok();

                                            let mut media_tags: Vec<(&str, String)> = vec![];
                                            if let Some(ref url) = img_url {
                                                media_tags.push(("image", url.clone()));
                                            }

                                            let event = build_record_event(
                                                &blobbi.d,
                                                "memory",
                                                blobbi.generation,
                                                media_tags,
                                                content,
                                            );
                                            if let Ok(signed) = crate::stores::publish_queue::signing::sign_event_builder(event).await {
                                                crate::stores::publish_queue::enqueue(
                                                    signed,
                                                    crate::stores::publish_queue::types::QueueEventType::Other("blobbi".to_string()),
                                                    None,
                                                    std::collections::HashMap::new(),
                                                ).await;
                                                crate::components::blobbi::actions::mission_tracker::track_photo();
                                            }
                                            uploading.set(false);
                                        });
                                    }
                                },
                                if uploading() { "Uploading..." } else { "Share" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "web")]
#[allow(dead_code)]
async fn capture_blobbi_image() -> Result<String, String> {
    let _ = load_html_to_image().await;

    let js = r#"
        (async function() {
            var el = document.getElementById('blobbi-photo-target');
            if (!el) return null;
            if (typeof htmlToImage === 'undefined') return null;
            try {
                var dataUrl = await htmlToImage.toPng(el, {
                    pixelRatio: 2,
                    backgroundColor: '#ffffff',
                });
                return dataUrl;
            } catch(e) {
                return null;
            }
        })()
    "#;

    let result = document::eval(js).await.map_err(|e| e.to_string())?;
    let data_url = result.as_str().unwrap_or("").to_string();

    if data_url.is_empty() {
        return Err("Failed to capture image".to_string());
    }

    let blob = data_url_to_blob(&data_url).await?;
    crate::stores::media::blossom_store::upload_image(blob, "image/png".to_string(), 90, None)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(feature = "web")]
#[allow(dead_code)]
async fn data_url_to_blob(data_url: &str) -> Result<Vec<u8>, String> {
    let base64_part = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or("Not a base64 PNG")?;
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(base64_part)
        .map_err(|e| e.to_string())
}

#[cfg(feature = "web")]
#[allow(dead_code)]
async fn load_html_to_image() -> Result<(), String> {
    let check = document::eval("return typeof htmlToImage !== 'undefined'").await;
    if check.map(|v| v.as_bool().unwrap_or(false)).unwrap_or(false) {
        return Ok(());
    }
    let script = r#"
        var s = document.createElement('script');
        s.src = 'https://cdn.jsdelivr.net/npm/html-to-image@1.11.11/dist/html-to-image.js';
        document.head.appendChild(s);
        return new Promise(function(resolve) { s.onload = function() { resolve(true); }; });
    "#;
    let _ = document::eval(script).await;
    Ok(())
}

#[cfg(not(feature = "web"))]
#[allow(dead_code)]
async fn capture_blobbi_image() -> Result<String, String> {
    Err("Photo capture not available on this platform".to_string())
}

fn current_date_string() -> String {
    let secs = crate::platform::timestamp::now_secs();
    let days = secs / 86400;
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let year = 1970 + (days / 365);
    let day_of_year = days % 365;
    let month_idx = ((day_of_year * 12) / 365) as usize;
    let month = months.get(month_idx).unwrap_or(&"Jan");
    let approx_day = (day_of_year % 30) + 1;
    format!("{} {}, {}", month, approx_day, year)
}
