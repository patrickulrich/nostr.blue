#[cfg(feature = "web")]
use dioxus::prelude::*;

#[cfg(feature = "web")]
const TYPING_JS: &str = r#"
(function() {
    if (window.__blobbiTypingTracker) return;
    window.__blobbiTypingTracker = true;
    window.__blobbiTypingX = null;
    window.__blobbiTypingY = null;
    window.__blobbiIsTyping = false;

    var IDLE_TIMEOUT = 4000;
    var idleTimer = null;

    var OVERLAY_SEL = '[data-radix-dialog-content],[data-radix-popover-content],[role="dialog"],[data-vaul-drawer]';
    var INPUT_SEL = 'input[type="text"],input[type="search"],input:not([type]),textarea,[contenteditable="true"],[role="textbox"]';

    function isInsideOverlay(el) {
        var node = el;
        while (node && node !== document.body) {
            if (node.matches && node.matches(OVERLAY_SEL)) return true;
            node = node.parentElement;
        }
        return false;
    }

    function clearIdle() {
        if (idleTimer) clearTimeout(idleTimer);
        idleTimer = setTimeout(function() {
            window.__blobbiIsTyping = false;
            window.__blobbiTypingX = null;
            window.__blobbiTypingY = null;
        }, IDLE_TIMEOUT);
    }

    function computeCaret(el) {
        if (el.isContentEditable || el.getAttribute('contenteditable') === 'true') {
            var sel = window.getSelection();
            if (sel && sel.rangeCount > 0) {
                var range = sel.getRangeAt(0);
                var rects = range.getClientRects();
                if (rects.length > 0) {
                    var r = rects[0];
                    window.__blobbiTypingX = r.right;
                    window.__blobbiTypingY = r.top + r.height / 2;
                    return;
                }
            }
        }

        if (typeof el.selectionStart === 'number') {
            var text = el.value.substring(0, el.selectionStart);
            var span = document.createElement('span');
            var cs = window.getComputedStyle(el);
            span.style.font = cs.font;
            span.style.fontSize = cs.fontSize;
            span.style.fontFamily = cs.fontFamily;
            span.style.letterSpacing = cs.letterSpacing;
            span.style.whiteSpace = 'pre-wrap';
            span.style.wordWrap = 'break-word';
            span.style.position = 'absolute';
            span.style.visibility = 'hidden';
            span.style.left = '-9999px';
            span.style.width = cs.width;
            span.style.padding = cs.padding;
            span.style.border = cs.border;
            span.style.boxSizing = cs.boxSizing;
            span.textContent = text;
            document.body.appendChild(span);

            var lines = text.split('\n');
            var lineHeight = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.2;
            var yOffset = (lines.length - 1) * lineHeight;

            var rect = el.getBoundingClientRect();
            var spanRect = span.getBoundingClientRect();
            window.__blobbiTypingX = rect.left + (spanRect.width - rect.left + rect.width) / 2;
            window.__blobbiTypingX = Math.min(window.__blobbiTypingX, rect.right - 10);
            window.__blobbiTypingY = rect.top + yOffset + lineHeight / 2;

            document.body.removeChild(span);
            return;
        }

        var rect = el.getBoundingClientRect();
        window.__blobbiTypingX = rect.right - 20;
        window.__blobbiTypingY = rect.top + rect.height / 2;
    }

    function handleTyping(el) {
        if (!el || !el.matches(INPUT_SEL)) return;
        if (!isInsideOverlay(el)) return;
        window.__blobbiIsTyping = true;
        computeCaret(el);
        clearIdle();
    }

    document.addEventListener('focusin', function(e) {
        var el = e.target;
        if (el && el.matches && el.matches(INPUT_SEL) && isInsideOverlay(el)) {
            window.__blobbiIsTyping = true;
            computeCaret(el);
            clearIdle();
        }
    }, true);

    document.addEventListener('input', function(e) {
        handleTyping(e.target);
    }, true);

    document.addEventListener('keydown', function(e) {
        if (e.target && e.target.matches && e.target.matches(INPUT_SEL)) {
            if (e.key.length === 1 || e.key === 'Backspace' || e.key === 'Delete' || e.key === 'Enter') {
                handleTyping(e.target);
            }
        }
    }, true);

    document.addEventListener('selectionchange', function() {
        var el = document.activeElement;
        if (el && el.matches && el.matches(INPUT_SEL) && isInsideOverlay(el) && window.__blobbiIsTyping) {
            computeCaret(el);
        }
    }, true);
})();
"#;

#[cfg(feature = "web")]
pub fn use_typing_attention() -> Signal<Option<(f32, f32)>> {
    let typing_target: Signal<Option<(f32, f32)>> = use_signal(|| None);

    use_future(move || {
        let mut typing_target = typing_target;
        async move {
            let _ = document::eval(TYPING_JS).await;

            loop {
                crate::platform::timer::sleep_ms(150).await;

                let result = document::eval(
                    r#"
                    (function() {
                        if (window.__blobbiIsTyping && window.__blobbiTypingX !== null) {
                            var x = window.__blobbiTypingX;
                            var y = window.__blobbiTypingY;
                            return JSON.stringify({x: x, y: y});
                        }
                        return null;
                    })()
                    "#,
                ).await;

                if let Ok(v) = result {
                    if let Some(s) = v.as_str() {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                            if let (Some(x), Some(y)) = (parsed["x"].as_f64(), parsed["y"].as_f64())
                            {
                                typing_target.set(Some((x as f32, y as f32)));
                                continue;
                            }
                        }
                    }
                }

                typing_target.set(None);
            }
        }
    });

    typing_target
}

#[cfg(not(feature = "web"))]
pub fn use_typing_attention() -> dioxus::prelude::Signal<Option<(f32, f32)>> {
    dioxus::prelude::use_signal(|| None)
}
