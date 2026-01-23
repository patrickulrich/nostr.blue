// Service Worker Registration with Update Notification
if ('serviceWorker' in navigator) {
    window.addEventListener('load', () => {
        navigator.serviceWorker.register('/sw.js')
            .then(reg => {
                console.log('[PWA] Service Worker registered:', reg.scope);

                // Check for updates periodically
                setInterval(() => {
                    reg.update().catch(err => {
                        console.error('[PWA] Service Worker update check failed:', err);
                    });
                }, 60 * 60 * 1000); // Check every hour

                // Listen for update found
                reg.addEventListener('updatefound', () => {
                    const newWorker = reg.installing;
                    if (newWorker) {
                        newWorker.addEventListener('statechange', () => {
                            // New service worker installed and ready to activate
                            if (newWorker.state === 'installed' && navigator.serviceWorker.controller) {
                                console.log('[PWA] New version available');
                                // Dispatch custom event for the app to handle
                                window.dispatchEvent(new CustomEvent('sw-update-available'));
                            }
                        });
                    }
                });
            })
            .catch(err => console.error('[PWA] Service Worker registration failed:', err));

        // Auto-reload when new service worker takes control
        navigator.serviceWorker.addEventListener('controllerchange', () => {
            console.log('[PWA] Controller changed, reloading for update...');
            window.location.reload();
        });
    });
}
