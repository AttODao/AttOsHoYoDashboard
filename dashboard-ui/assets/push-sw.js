// Minimal service worker for Web Push: shows the payload and focuses the dashboard.
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
