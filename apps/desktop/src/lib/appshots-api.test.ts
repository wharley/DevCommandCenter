import { afterEach, describe, expect, it } from "vitest";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import {
	appshotErrorKey,
	getAppshotStatus,
	shortcutFromEvent,
} from "./appshots-api";

describe("Appshots capability and shortcuts", () => {
	afterEach(() => {
		clearMocks();
		Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
	});
	it("identifies an outdated binary instead of showing an opaque native error", async () => {
		mockIPC(() => Promise.reject("Command appshots_status not found"));
		await expect(getAppshotStatus()).rejects.toThrow("backendOutdated");
	});
	it("reports unsupported on the web without invoking native APIs", async () => {
		expect((await getAppshotStatus()).supported).toBe(false);
	});
	it("uses physical keys for shifted digits and requires a global modifier", () => {
		expect(
			shortcutFromEvent({
				code: "Digit9",
				metaKey: true,
				ctrlKey: false,
				altKey: false,
				shiftKey: true,
			}),
		).toBe("Super+Shift+Digit9");
		expect(
			shortcutFromEvent({
				code: "KeyA",
				metaKey: false,
				ctrlKey: false,
				altKey: false,
				shiftKey: true,
			}),
		).toBeNull();
		expect(
			shortcutFromEvent({
				code: "MetaLeft",
				metaKey: true,
				ctrlKey: false,
				altKey: false,
				shiftKey: false,
			}),
		).toBeNull();
	});
	it("keeps arbitrary native errors out of product copy", () => {
		expect(appshotErrorKey("permission")).toBe("appshots.errors.permission");
		expect(appshotErrorKey(new Error("private OS error"))).toBe(
			"appshots.errors.unavailable",
		);
	});
});
