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
self.addEventListener("push", (event) => {
	let data = {};
	try {
		data = event.data?.json() || {};
	} catch {
		/* Show a generic visible notification. */
	}
	event.waitUntil(
		self.registration.showNotification(data.title || "DCC", {
			body: data.body || "Há uma atualização nas suas tarefas.",
			icon: "/m/icons/icon-192.png",
			badge: "/m/icons/icon-192.png",
			tag: data.tag || "dcc-update",
			data: { url: data.url || "/m/" },
		}),
	);
});
self.addEventListener("notificationclick", (event) => {
	event.notification.close();
	const candidate = new URL(
		event.notification.data?.url || "/m/",
		self.location.origin,
	);
	const url =
		candidate.origin === self.location.origin &&
		candidate.pathname.startsWith("/m/")
			? candidate.href
			: new URL("/m/", self.location.origin).href;
	event.waitUntil(
		(async () => {
			const windows = await self.clients.matchAll({
				type: "window",
				includeUncontrolled: true,
			});
			const existing = windows.find((client) =>
				client.url.startsWith(new URL("/m/", self.location.origin).href),
			);
			if (existing) {
				await existing.navigate(url);
				return existing.focus();
			}
			return self.clients.openWindow(url);
		})(),
	);
});
