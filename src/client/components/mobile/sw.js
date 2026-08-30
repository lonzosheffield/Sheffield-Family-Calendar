// Sheffield Family Hub service worker — PLAN v2 D6, task T2.2.
//
// Declared in docs/NON_RUST.md; served from Rust at /sw.js via include_str!
// (src/client/components/mobile/pwa.rs) so it is always same-origin and
// root-scoped. Per-platform behaviour: docs/PWA.md.
//
// Three caching rules, and nothing else:
//   * app shell + static assets -> network-first, cached fallback
//   * server functions (/api/*) -> network-first, never a stale mutation
//   * /uploads + screensaver     -> cache-first (content-addressed photos)
//
// Mutations are NOT retried here: Background Sync does not exist on iOS, so
// the offline queue is a pure-Rust struct in localStorage instead
// (src/client/components/mobile/queue.rs).

const VERSION = 'familyhub-v1';
const SHELL = VERSION + '-shell';
const MEDIA = VERSION + '-media';
const KEEP = [SHELL, MEDIA];

const SHELL_URLS = [
  '/m',
  '/manifest.webmanifest',
  '/icons/icon-192.png',
  '/icons/icon-512.png',
  '/icons/icon-192-maskable.png',
  '/icons/icon-512-maskable.png',
  // D4.1: the poster faces, same-origin and build-time-fixed (never change
  // per-install), so they belong in the app shell like the icons above.
  '/fonts/nunito-600-latin.woff2',
  '/fonts/nunito-800-latin.woff2',
  '/fonts/baloo2-800-latin.woff2',
];

self.addEventListener('install', (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(SHELL);
    // One failed precache entry must not abort the whole install.
    await Promise.all(SHELL_URLS.map((url) =>
      cache.add(new Request(url, { cache: 'reload' })).catch(() => undefined)));
    await self.skipWaiting();
  })());
});

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    const names = await caches.keys();
    await Promise.all(names.filter((n) => KEEP.indexOf(n) === -1).map((n) => caches.delete(n)));
    await self.clients.claim();
  })());
});

function isServerFn(url) {
  return url.pathname.indexOf('/api/') === 0;
}

function isMedia(url) {
  return url.pathname.indexOf('/uploads/') === 0
    || url.pathname.indexOf('/assets/screensaver/') === 0;
}

async function networkFirst(request, cacheName) {
  const cacheable = request.method === 'GET';
  const cache = await caches.open(cacheName);
  try {
    const response = await fetch(request);
    if (cacheable && response && response.ok) {
      await cache.put(request, response.clone());
    }
    return response;
  } catch (err) {
    if (cacheable) {
      const cached = await cache.match(request);
      if (cached) return cached;
      if (request.mode === 'navigate') {
        const shell = await caches.open(SHELL);
        const fallback = await shell.match('/m');
        if (fallback) return fallback;
      }
    }
    throw err;
  }
}

async function cacheFirst(request, cacheName) {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response && response.ok) {
    await cache.put(request, response.clone());
  }
  return response;
}

self.addEventListener('fetch', (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (url.pathname === '/ws') return;
  if (isServerFn(url)) {
    event.respondWith(networkFirst(request, SHELL));
    return;
  }
  if (request.method !== 'GET') return;
  if (isMedia(url)) {
    event.respondWith(cacheFirst(request, MEDIA));
    return;
  }
  event.respondWith(networkFirst(request, SHELL));
});
