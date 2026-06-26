// nostr.blue Service Worker
const CACHE_NAME = 'nostr-blue-v1';
const STATIC_ASSETS = [
  '/',
  '/index.html',  // Required for SPA offline fallback routing
  '/voice-recorder.js',
  '/manifest.json'
  // Note: Hashed CSS (/assets/tailwind-[hash].css) is cached on first fetch via
  // stale-while-revalidate strategy in the fetch handler. The hash changes on each
  // build, so we cannot hardcode it here. For true offline-first, consider using
  // a build-time generated precache manifest (e.g., workbox-precaching).
];

// Install - cache static assets + discover hashed assets
self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then(async (cache) => {
      console.log('[SW] Caching static assets');
      await cache.addAll(STATIC_ASSETS);

      // Discover and cache hashed assets from index.html
      // Dioxus uses asset!() macro which hashes CSS/JS at build time
      try {
        const response = await fetch('/index.html');
        const html = await response.text();

        // Find all /assets/ CSS, JS, and WASM references
        const assetRegex = /\/assets\/[^"'\s]+\.(css|js|wasm)/g;
        const matches = html.match(assetRegex) || [];

        // Also check root document (Dioxus may inject assets differently)
        const docResponse = await fetch('/');
        const docHtml = await docResponse.text();
        const docMatches = docHtml.match(assetRegex) || [];

        const allAssets = [...new Set([...matches, ...docMatches])];

        if (allAssets.length > 0) {
          console.log('[SW] Discovered hashed assets:', allAssets);
          await cache.addAll(allAssets);
        }
      } catch (err) {
        console.warn('[SW] Failed to discover hashed assets:', err);
      }
    }).then(() => {
      // skipWaiting must be inside waitUntil to ensure caching completes first
      self.skipWaiting();
    })
  );
});

// Activate - clean old caches
self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(
        keys
          .filter((k) => k !== CACHE_NAME)
          .map((k) => {
            console.log('[SW] Removing old cache:', k);
            return caches.delete(k);
          })
      )
    ).then(() => self.clients.claim())
  );
});

// Fetch - network first for navigation, cache first for assets
self.addEventListener('fetch', (event) => {
  // Skip non-GET requests
  if (event.request.method !== 'GET') return;

  // Skip cross-origin requests (relay connections, APIs, etc.)
  if (!event.request.url.startsWith(self.location.origin)) return;

  // For critical assets (CSS/JS in /assets/), use network-first with cache fallback
  // This ensures fresh installs can work offline after first visit
  if (event.request.url.includes('/assets/') &&
      (event.request.url.endsWith('.css') || event.request.url.endsWith('.js'))) {
    event.respondWith(
      fetch(event.request)
        .then((response) => {
          if (response.ok) {
            const clone = response.clone();
            caches.open(CACHE_NAME).then((cache) => cache.put(event.request, clone));
            return response;
          }
          // Non-OK response: try cache first
          return caches.match(event.request).then((cached) =>
            cached || response  // Return cached if available, otherwise the error response
          );
        })
        .catch(() =>
          caches.match(event.request).then((cached) =>
            cached || new Response('Asset unavailable offline', {
              status: 503,
              headers: { 'Content-Type': 'text/plain' }
            })
          )
        )
    );
    return;
  }

  // For navigation requests, use network-first with offline fallback
  if (event.request.mode === 'navigate') {
    event.respondWith(
      fetch(event.request)
        .then((response) => {
          // Cache successful navigation responses
          if (response.ok) {
            const clone = response.clone();
            caches.open(CACHE_NAME).then((cache) => {
              cache.put(event.request, clone);
            });
          }
          return response;
        })
        .catch(() => {
          // Offline - return cached index.html for SPA routing
          // Defensive fallback in case index.html isn't cached
          return caches.match('/index.html').then(response => {
            return response || new Response('Offline - please check your connection', {
              status: 503,
              headers: { 'Content-Type': 'text/plain' }
            });
          });
        })
    );
    return;
  }

  // For static assets, use cache-first with network fallback
  event.respondWith(
    caches.match(event.request).then((cached) => {
      if (cached) {
        // Return cached version and update cache in background (stale-while-revalidate)
        fetch(event.request)
          .then((response) => {
            if (response.ok) {
              const clone = response.clone();
              caches.open(CACHE_NAME).then((cache) => {
                cache.put(event.request, clone);
              });
            }
          })
          .catch((err) => {
            console.warn('[SW] Background cache update failed:', err);
          });
        return cached;
      }

      // Not cached - fetch from network
      return fetch(event.request)
        .then((response) => {
          if (response.ok) {
            const clone = response.clone();
            caches.open(CACHE_NAME).then((cache) => {
              cache.put(event.request, clone);
            });
          }
          return response;
        })
        .catch((err) => {
          console.warn('[SW] Network fetch failed for uncached asset:', event.request.url, err);
          // Return a basic error response instead of letting promise reject
          return new Response('Resource unavailable offline', {
            status: 503,
            headers: { 'Content-Type': 'text/plain' }
          });
        });
    })
  );
});

// Handle messages from the app
self.addEventListener('message', (event) => {
  if (event.data === 'skipWaiting') {
    self.skipWaiting();
  }
  // Phase 3: wake signal from the Mostro push server or from another tab.
  // When received, trigger a backfill of active trades so the toast drainer
  // picks up any events that arrived while the tab was backgrounded.
  if (event.data && event.data.type === 'mostro-wake') {
    console.log('[SW] Received mostro-wake, forwarding to clients');
    // The actual backfill is handled by the app listening for this event
    // via navigator.serviceWorker.addEventListener('message', ...). We just
    // rebroadcast to ensure all open tabs pick it up.
    event.source && event.source.postMessage({ type: 'mostro-wake' });
  }
});

// Push event handler — fires when the push server delivers a notification.
// The Mostro push server sends an empty payload (silent wake); the app is
// expected to fetch new events itself. We show a generic notification so
// the user knows to open the app, and postMessage any controlled clients
// to trigger a background backfill.
self.addEventListener('push', (event) => {
  let payload = { title: 'Mostro', body: 'New trade update', tag: 'mostro' };
  try {
    if (event.data) {
      const json = event.data.json();
      if (json && (json.title || json.body)) payload = json;
    }
  } catch (e) {
    try { payload.body = event.data.text(); } catch {}
  }

  // Wake any open tabs to trigger backfill.
  clients.matchAll({ type: 'window', includeUncontrolled: true }).then((clientList) => {
    for (const client of clientList) {
      client.postMessage({ type: 'mostro-wake' });
    }
  });

  event.waitUntil(
    self.registration.showNotification(payload.title, {
      body: payload.body,
      icon: '/icons/192.png',
      badge: '/icons/badge-72.png',
      tag: payload.tag || 'mostro',
      data: payload.data || {},
      requireInteraction: false,
    })
  );
});

// Notification click — focus or open the app to the trades page.
self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const targetUrl = (event.notification.data && event.notification.data.url)
    || '/#/p2p/my-trades';
  event.waitUntil(
    clients.matchAll({ type: 'window', includeUncontrolled: true }).then((clientList) => {
      for (const client of clientList) {
        if (client.url.includes(self.location.origin) && 'focus' in client) {
          return client.focus();
        }
      }
      if (clients.openWindow) return clients.openWindow(targetUrl);
    })
  );
});
