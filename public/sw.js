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

// Install - cache static assets
self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => {
      console.log('[SW] Caching static assets');
      return cache.addAll(STATIC_ASSETS);
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
          }
          return response;
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
});
