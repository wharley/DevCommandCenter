/* Replaced with the build's exact asset list and digest by Vite. No API or token caching. */
const PRECACHE = /* DCC_PRECACHE */ [];
const CACHE = "dcc-shell-/* DCC_VERSION */";
self.addEventListener("install", (event) => {
	event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(PRECACHE)));
});
self.addEventListener("activate", (event) => {
	event.waitUntil(
		(async () => {
			for (const name of await caches.keys())
				if (name.startsWith("dcc-shell-") && name !== CACHE)
					await caches.delete(name);
			// Retire subscriptions left by 0.1.65 while preserving the offline shell.
			try {
				await (
					await self.registration.pushManager?.getSubscription()
				)?.unsubscribe();
			} catch {
				/* Subscription cleanup must not prevent an offline-shell update. */
			}
			await self.clients.claim();
		})(),
	);
});
self.addEventListener("message", (event) => {
	if (event.data?.type === "ACTIVATE_UPDATE") self.skipWaiting();
});
self.addEventListener("fetch", (event) => {
	const request = event.request;
	const url = new URL(request.url);
	if (
		request.method !== "GET" ||
		url.origin !== self.location.origin ||
		!url.pathname.startsWith("/m/")
	)
		return;
	if (request.mode === "navigate") {
		event.respondWith(
			(async () => {
				const controller = new AbortController();
				const timeout = setTimeout(() => controller.abort(), 5000);
				try {
					const response = await fetch(request, { signal: controller.signal });
					if (response.ok) return response;
					return (await caches.match("/m/index.html")) || response;
				} catch {
					return (await caches.match("/m/index.html")) || Response.error();
				} finally {
					clearTimeout(timeout);
				}
			})(),
		);
	} else if (PRECACHE.includes(url.pathname)) {
		event.respondWith(
			caches.match(url.pathname).then((cached) => cached || fetch(request)),
		);
	}
});
