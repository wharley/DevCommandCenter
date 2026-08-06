import { describe, expect, it } from "vitest";
import { FileContentLru } from "./file-content-lru";

describe("FileContentLru", () => {
	it("evicts least recently used entries by count", () => {
		const cache = new FileContentLru(2, 1_000);
		cache.set("/repo/a", "a");
		cache.set("/repo/b", "b");
		expect(cache.get("/repo/a")).toBe("a");
		cache.set("/repo/c", "c");
		expect(cache.get("/repo/b")).toBeUndefined();
		expect(cache.stats()).toEqual({ entries: 2, bytes: 4 });
	});

	it("enforces the byte budget and purges only the requested root", () => {
		const cache = new FileContentLru(10, 12);
		cache.set("/a/one", "1111");
		cache.set("/b/two", "22");
		cache.set("/a/three", "3333");
		expect(cache.get("/a/one")).toBeUndefined();
		cache.purgeRoot("/a");
		expect(cache.get("/a/three")).toBeUndefined();
		expect(cache.get("/b/two")).toBe("22");
	});

	it("normalizes Windows separators and respects directory boundaries", () => {
		const cache = new FileContentLru(10, 1_000);
		cache.set("C:\\repo\\a\\one.ts", "one");
		cache.set("C:\\repo\\ab\\two.ts", "two");
		cache.purgeRoot("C:\\repo\\a\\");
		expect(cache.get("C:\\repo\\a\\one.ts")).toBeUndefined();
		expect(cache.get("C:\\repo\\ab\\two.ts")).toBe("two");
	});
});
