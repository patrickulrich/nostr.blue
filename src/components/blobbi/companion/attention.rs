use dioxus::prelude::*;

#[cfg(feature = "web")]
pub fn use_ui_attention() -> Signal<Option<(f32, f32)>> {
    let attention_target: Signal<Option<(f32, f32)>> = use_signal(|| None);

    use_future(move || {
        let mut attention_target = attention_target;
        async move {
            let _ = document::eval(
                r#"
                (function() {
                    if (window.__blobbiAttentionObserver) return;

                    const selector = '[data-radix-dialog-content],[data-radix-popover-content],[role="dialog"],[data-state="open"][data-side],[data-vaul-drawer],[role="tabpanel"][data-state="active"],[data-slot="tabpanel"][data-state="active"]';

                    window.__blobbiAttentionObserver = new MutationObserver(function(mutations) {
                        for (const mutation of mutations) {
                            for (const node of mutation.addedNodes) {
                                if (node instanceof Element) {
                                    const el = node.matches(selector) ? node : node.querySelector(selector);
                                    if (el) {
                                        const rect = el.getBoundingClientRect();
                                        window.__blobbiAttentionX = rect.left + rect.width / 2;
                                        window.__blobbiAttentionY = rect.top + rect.height / 2;
                                        window.__blobbiAttentionPriority = el.matches('[role="dialog"]') ? 'high' : 'normal';
                                    }
                                }
                            }
                            if (mutation.type === 'attributes' && mutation.target instanceof Element) {
                                if (mutation.attributeName === 'data-state') {
                                    const target = mutation.target;
                                    const state = target.getAttribute('data-state');
                                    if (state === 'active' || state === 'open') {
                                        if (target.matches(selector)) {
                                            const rect = target.getBoundingClientRect();
                                            window.__blobbiAttentionX = rect.left + rect.width / 2;
                                            window.__blobbiAttentionY = rect.top + rect.height / 2;
                                            window.__blobbiAttentionPriority = target.matches('[role="dialog"]') ? 'high' : 'normal';
                                        }
                                    }
                                }
                            }
                        }
                    });

                    window.__blobbiAttentionObserver.observe(document.body, {
                        childList: true,
                        subtree: true,
                        attributes: true,
                        attributeFilter: ['data-state']
                    });
                })();
                "#,
            ).await;

            loop {
                crate::platform::timer::sleep_ms(200).await;

                let result = document::eval(
                    r#"
                    (function() {
                        if (window.__blobbiAttentionX !== undefined) {
                            const x = window.__blobbiAttentionX;
                            const y = window.__blobbiAttentionY;
                            const p = window.__blobbiAttentionPriority || 'normal';
                            delete window.__blobbiAttentionX;
                            delete window.__blobbiAttentionY;
                            delete window.__blobbiAttentionPriority;
                            return JSON.stringify({x: x, y: y, priority: p});
                        }
                        return null;
                    })()
                    "#,
                ).await;

                if let Ok(v) = result {
                    if let Some(s) = v.as_str() {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                            if let (Some(x), Some(y)) = (parsed["x"].as_f64(), parsed["y"].as_f64()) {
                                let priority = parsed["priority"].as_str().unwrap_or("normal");
                                let ms = if priority == "high" { 5000 } else { 3000 };
                                attention_target.set(Some((x as f32, y as f32)));

                                let mut at = attention_target;
                                spawn(async move {
                                    crate::platform::timer::sleep_ms(ms).await;
                                    at.set(None);
                                });
                            }
                        }
                    }
                }
            }
        }
    });

    attention_target
}

#[cfg(not(feature = "web"))]
pub fn use_ui_attention() -> Signal<Option<(f32, f32)>> {
    use_signal(|| None)
}
