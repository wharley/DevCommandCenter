import { describe, expect, it } from "vitest";
import {
	detectLocalDevServerUrls,
	formatDevServerAddress,
	scanDevServerOutput,
} from "./dev-server-detection";

describe("dev server detection", () => {
	it("detects colored localhost output from common dev servers", () => {
		expect(
			detectLocalDevServerUrls(
				"\x1b[32m➜\x1b[0m Local: http://localhost:5173/\r\n",
			),
		).toEqual(["http://localhost:5173/"]);
	});

	it("accepts IPv4 and IPv6 loopback URLs", () => {
		expect(
			detectLocalDevServerUrls(
				"API http://127.0.0.1:8000/docs UI https://[::1]:3000/app",
			),
		).toEqual([
			"http://127.0.0.1:8000/docs",
			"https://[::1]:3000/app",
		]);
	});

	it("turns bind-all server addresses into navigable loopback URLs", () => {
		expect(
			detectLocalDevServerUrls(
				"Listening at http://0.0.0.0:4321 and http://[::]:8080",
			),
		).toEqual([
			"http://127.0.0.1:4321/",
			"http://[::1]:8080/",
		]);
	});

	it("ignores remote, LAN, malformed, and non-HTTP addresses", () => {
		expect(
			detectLocalDevServerUrls(
				"https://example.com http://192.168.1.10:5173 ftp://localhost:21 http://127.999.0.1:9 http://localhost:3000.example.com",
			),
		).toEqual([]);
	});

	it("recognizes a URL split across PTY chunks without repeating old matches", () => {
		const first = scanDevServerOutput("", "ready at http://local");
		expect(first.urls).toEqual([]);

		const second = scanDevServerOutput(first.tail, "host:5173/\r\n");
		expect(second.urls).toEqual(["http://localhost:5173/"]);

		const third = scanDevServerOutput(second.tail, "compiled successfully\r\n");
		expect(third.urls).toEqual([]);
	});

	it("formats a compact address for the terminal action", () => {
		expect(formatDevServerAddress("http://localhost:5173/app")).toBe(
			"localhost:5173",
		);
	});
});
