// SafiData Owner PWA Secure Service Worker (Sections 9.6 and 9.8)
// Strictly forbids caching sensitive owner data, audit logs, or auth tokens.

const CACHE_NAME = "safi-owner-pwa-v1";
const STATIC_ASSETS = [
  "/",
  "/index.html"
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => {
      return cache.addAll(STATIC_ASSETS);
    })
  );
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((keys) => {
      return Promise.all(
        keys.filter((key) => key !== CACHE_NAME).map((key) => caches.delete(key))
      );
    })
  );
  self.clients.claim();
});

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);

  // NEVER cache sensitive owner API routes (/v1/owner/*) or auth tokens
  if (url.pathname.startsWith("/v1/owner/") || url.pathname.startsWith("/v1/auth/")) {
    event.respondWith(fetch(event.request));
    return;
  }

  event.respondWith(
    caches.match(event.request).then((cachedResponse) => {
      if (cachedResponse) {
        return cachedResponse;
      }
      return fetch(event.request);
    })
  );
});

// Emergency Push Notification handler
self.addEventListener("push", (event) => {
  const data = event.data ? event.data.json() : { title: "تنبيه طارئ من صافي", body: "تم رصد نشاط غير معتاد أو طلب صيانة طارئة." };

  const options = {
    body: data.body,
    icon: "/favicon.ico",
    badge: "/favicon.ico",
    dir: "rtl",
    lang: "ar",
    vibrate: [200, 100, 200]
  };

  event.waitUntil(
    self.registration.showNotification(data.title, options)
  );
});
