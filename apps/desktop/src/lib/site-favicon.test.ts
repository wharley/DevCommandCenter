import { afterEach, beforeEach, expect, it, vi } from "vitest";

let images: HTMLImageElement[];
beforeEach(() => {
	vi.resetModules();
	vi.useFakeTimers();
	images = [];
	vi.stubGlobal(
		"Image",
		class {
			constructor() {
				const image = document.createElement("img");
				images.push(image);
				return image;
			}
		},
	);
});
afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

it("uses any public site's hostname without leaking credentials, paths or query strings", async () => {
	const { siteFaviconSource } = await import("./site-favicon");
	expect(
		siteFaviconSource(
			"https://user:secret@GitHub.com/team/private?token=secret#section",
		),
	).toBe("https://www.google.com/s2/favicons?domain=github.com&sz=32");
	expect(siteFaviconSource("https://synara.dev/docs")).toContain(
		"domain=synara.dev&",
	);
	expect(siteFaviconSource("https://github.com.example.org")).toContain(
		"domain=github.com.example.org&",
	);
	for (const value of [
		"invalid",
		"file:///tmp/file",
		"javascript:alert(1)",
		"https://localhost",
		"http://127.0.0.1",
		"https://192.168.1.10",
		"https://[::1]",
		"http://dev.internal",
		"http://dev.local",
	]) {
		expect(siteFaviconSource(value)).toBeNull();
	}
});

it("deduplicates concurrent requests and caches successful icons by host", async () => {
	const { loadSiteFavicon } = await import("./site-favicon");
	const first = loadSiteFavicon("https://example.com/a");
	const second = loadSiteFavicon("https://example.com/b");
	expect(second).toBe(first);
	expect(images).toHaveLength(1);
	expect(images[0].referrerPolicy).toBe("no-referrer");
	images[0].dispatchEvent(new Event("load"));
	const src = await first;
	expect(src).toContain("domain=example.com&");
	expect(await loadSiteFavicon("https://example.com/c")).toBe(src);
	expect(images).toHaveLength(1);
});

it("falls back on failures and timeouts, and retries after the negative cache expires", async () => {
	const { loadSiteFavicon } = await import("./site-favicon");
	const failed = loadSiteFavicon("https://example.com");
	images[0].dispatchEvent(new Event("error"));
	expect(await failed).toBeNull();
	expect(await loadSiteFavicon("https://example.com/path")).toBeNull();
	expect(images).toHaveLength(1);
	await vi.advanceTimersByTimeAsync(60_001);
	const retry = loadSiteFavicon("https://example.com");
	expect(images).toHaveLength(2);
	await vi.advanceTimersByTimeAsync(5_000);
	expect(await retry).toBeNull();
});
