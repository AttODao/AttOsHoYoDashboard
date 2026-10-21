// Minimal service worker for Web Push: shows the payload and focuses the dashboard.
// Install/activate hand control to the newest worker immediately so a re-register
// after an update does not stall in the "waiting" state (which looks, on a page
// that reloads soon after, like the subscribe call never resolves).
self.addEventListener('install', (event) => {
  event.waitUntil(self.skipWaiting());
});
self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim());
});
self.addEventListener('push', (event) => {
  if (!event.data) return;
  let payload;
  try { payload = event.data.json(); } catch (_) { payload = { title: 'AHD', body: event.data.text() }; }
  event.waitUntil(self.registration.showNotification(payload.title || 'AHD', {
    body: payload.body || '',
    tag: payload.tag,
    data: { url: payload.url || '/dashboard' },
    icon: '/assets/dashboard-icon.png'
  }));
});
self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const url = (event.notification.data && event.notification.data.url) || '/dashboard';
  event.waitUntil(self.clients.matchAll({ type: 'window' }).then((clients) => {
    for (const client of clients) {
      if (client.url.endsWith(url)) { return client.focus(); }
    }
    return self.clients.openWindow(url);
  }));
});
