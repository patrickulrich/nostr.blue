#[cfg(feature = "web")]
use dioxus::prelude::*;

#[cfg(feature = "web")]
const EYE_JS: &str = r#"
(function() {
    if (window.__blobbiEyeTrackerInstalled) return;
    window.__blobbiEyeTrackerInstalled = true;
    window.__blobbiMouseX = 0;
    window.__blobbiMouseY = 0;

    window.addEventListener('mousemove', function(e) {
        window.__blobbiMouseX = e.clientX;
        window.__blobbiMouseY = e.clientY;
    }, { capture: true, passive: true });

    var BLINK_MIN = 2000;
    var BLINK_MAX = 5000;
    var BLINK_CLOSE = 80;
    var BLINK_CLOSED = 100;
    var BLINK_OPEN = 120;
    var BLINK_AMOUNT = 0.95;
    var DOUBLE_CHANCE = 0.2;
    var MAX_MOVE = 2;
    var VERT_SCALE = 0.7;

    var blinkState = {
        phase: 'open',
        phaseStart: performance.now(),
        nextBlink: performance.now() + BLINK_MIN + Math.random() * (BLINK_MAX - BLINK_MIN),
        doubleBlink: false
    };

    function nextBlinkInterval() {
        return BLINK_MIN + Math.random() * (BLINK_MAX - BLINK_MIN);
    }

    function updateBlink(now) {
        var elapsed = now - blinkState.phaseStart;
        switch (blinkState.phase) {
            case 'open':
                if (now >= blinkState.nextBlink) {
                    blinkState.phase = 'closing';
                    blinkState.phaseStart = now;
                    blinkState.doubleBlink = Math.random() < DOUBLE_CHANCE;
                }
                return 0;
            case 'closing':
                var p = Math.min(1, elapsed / BLINK_CLOSE);
                return p * p * BLINK_AMOUNT;
            case 'closed':
                if (elapsed >= BLINK_CLOSED) {
                    blinkState.phase = 'opening';
                    blinkState.phaseStart = now;
                }
                return BLINK_AMOUNT;
            case 'opening':
                var p2 = Math.min(1, elapsed / BLINK_OPEN);
                var eased = 1 - (1 - p2) * (1 - p2);
                if (p2 >= 1) {
                    if (blinkState.doubleBlink) {
                        blinkState.phase = 'closing';
                        blinkState.doubleBlink = false;
                    } else {
                        blinkState.phase = 'open';
                        blinkState.nextBlink = now + nextBlinkInterval();
                    }
                    blinkState.phaseStart = now;
                    return 0;
                }
                return BLINK_AMOUNT * (1 - eased);
        }
        return 0;
    }

    function animate() {
        var now = performance.now();
        var blinkProgress = updateBlink(now);

        var containers = document.querySelectorAll('.blobbi-eye-container');
        containers.forEach(function(container) {
            var rect = container.getBoundingClientRect();
            var cx = rect.left + rect.width / 2;
            var cy = rect.top + rect.height / 2;
            var dx = window.__blobbiMouseX - cx;
            var dy = window.__blobbiMouseY - cy;
            var angle = Math.atan2(dy, dx);
            var ex = Math.cos(angle) * MAX_MOVE;
            var ey = Math.sin(angle) * MAX_MOVE * VERT_SCALE;

            var gazes = container.querySelectorAll('.blobbi-eye-gaze');
            gazes.forEach(function(g) {
                g.setAttribute('transform', 'translate(' + ex + ',' + ey + ')');
            });

            var clips = container.querySelectorAll('.blobbi-blink-clip-rect');
            clips.forEach(function(clip) {
                var top = parseFloat(clip.getAttribute('data-clip-top') || '0');
                var fullH = parseFloat(clip.getAttribute('data-clip-height') || '20');
                var offset = fullH * blinkProgress;
                clip.setAttribute('y', (top + offset).toString());
                clip.setAttribute('height', Math.max(0.1, fullH - offset).toString());
            });

            var eyelids = container.querySelectorAll('.blobbi-eyelid');
            eyelids.forEach(function(el) {
                el.setAttribute('opacity', blinkProgress > 0.1 ? Math.min(1, blinkProgress * 1.5).toString() : '0');
            });
        });

        requestAnimationFrame(animate);
    }

    requestAnimationFrame(animate);
})();
"#;

#[cfg(feature = "web")]
pub fn install_eye_tracker() {
    static INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if INSTALLED
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        return;
    }

    spawn(async move {
        let _ = document::eval(EYE_JS).await;
    });
}
