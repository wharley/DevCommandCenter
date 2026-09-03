import { describe, expect, it } from "vitest";
import { resolveHumanBrowserAddress } from "./browser-address";

describe("resolveHumanBrowserAddress", () => {
	it("keeps explicit web URLs unchanged", () => {
		expect(resolveHumanBrowserAddress("  https://example.com/docs?q=1  ")).toBe(
			"https://example.com/docs?q=1",
		);
		expect(resolveHumanBrowserAddress("http://localhost:5173/app")).toBe(
			"http://localhost:5173/app",
		);
	});

	it("adds HTTP to local development addresses", () => {
		expect(resolveHumanBrowserAddress("localhost:5173")).toBe("http://localhost:5173");
		expect(resolveHumanBrowserAddress("127.0.0.1:3000/path")).toBe(
			"http://127.0.0.1:3000/path",
		);
		expect(resolveHumanBrowserAddress("[::1]:8080")).toBe("http://[::1]:8080");
	});

	it("adds HTTPS to domain names and protocol-relative URLs", () => {
		expect(resolveHumanBrowserAddress("google.com")).toBe("https://google.com");
		expect(resolveHumanBrowserAddress("docs.example.com/guide")).toBe(
			"https://docs.example.com/guide",
		);
		expect(resolveHumanBrowserAddress("//example.com/path")).toBe(
			"https://example.com/path",
		);
	});

	it("turns words and phrases into encoded searches", () => {
		expect(resolveHumanBrowserAddress("google")).toBe(
			"https://www.google.com/search?q=google",
		);
		expect(resolveHumanBrowserAddress("como testar React & Vite")).toBe(
			"https://www.google.com/search?q=como%20testar%20React%20%26%20Vite",
		);
	});

	it("leaves explicit unsupported schemes for the backend to reject", () => {
		expect(resolveHumanBrowserAddress("javascript:alert(1)")).toBe("javascript:alert(1)");
		expect(resolveHumanBrowserAddress("ftp://example.com/file")).toBe(
			"ftp://example.com/file",
		);
	});

	it("does not leak filesystem-looking input to search", () => {
		expect(resolveHumanBrowserAddress("./private/file.txt")).toBe("./private/file.txt");
		expect(resolveHumanBrowserAddress("/Users/person/private.txt")).toBe(
			"/Users/person/private.txt",
		);
		expect(resolveHumanBrowserAddress("")).toBe("");
	});
});
