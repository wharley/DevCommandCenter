const MAX_CACHE_ENTRIES = 256;
const cache = new Map<string, { src: string | null; expiresAt: number }>();
const pending = new Map<string, Promise<string | null>>();

/** Only the public hostname is sent to the icon provider, never paths or tokens. */
export function siteFaviconSource(value: string): string | null {
	try {
		const url = new URL(value);
		const host = url.hostname.replace(/\.$/, "");
		if (
			!["http:", "https:"].includes(url.protocol) ||
			!host.includes(".") ||
			/^[\d.]+$/.test(host) ||
			host.includes(":") ||
			/\.(localhost|local|internal|test|invalid)$/.test(host)
		)
			return null;
		return `https://www.google.com/s2/favicons?domain=${encodeURIComponent(host)}&sz=32`;
	} catch {
		return null;
	}
}

/** Shared by editor decorations and React UI; concurrent links reuse one load. */
export function loadSiteFavicon(value: string): Promise<string | null> {
	const src = siteFaviconSource(value);
	if (!src) return Promise.resolve(null);
	const cached = cache.get(src);
	if (cached && cached.expiresAt > Date.now())
		return Promise.resolve(cached.src);
	const existing = pending.get(src);
	if (existing) return existing;
	const request = new Promise<string | null>((resolve) => {
		const image = new Image();
		const finish = (result: string | null) => {
			clearTimeout(timer);
			image.onload = image.onerror = null;
			if (!result) image.removeAttribute("src");
			resolve(result);
		};
		const timer = setTimeout(() => finish(null), 5_000);
		image.referrerPolicy = "no-referrer";
		image.onload = () => finish(src);
		image.onerror = () => finish(null);
		image.src = src;
	}).then((result) => {
		pending.delete(src);
		cache.delete(src);
		cache.set(src, {
			src: result,
			expiresAt: Date.now() + (result ? 86_400_000 : 60_000),
		});
		while (cache.size > MAX_CACHE_ENTRIES)
			cache.delete(cache.keys().next().value!);
		return result;
	});
	pending.set(src, request);
	return request;
}
