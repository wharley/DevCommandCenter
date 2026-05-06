import { describe, expect, it } from "vitest";
import {
	getOpenPreferredEditorShortcutKeys,
	isOpenPreferredEditorShortcut,
} from "./shortcut-utils";

describe("shortcut-utils", () => {
	it("matches Cmd+O on macOS", () => {
		expect(
			isOpenPreferredEditorShortcut(
				{
					key: "o",
					metaKey: true,
					ctrlKey: false,
					altKey: false,
					shiftKey: false,
					defaultPrevented: false,
				},
				"MacIntel",
			),
		).toBe(true);
	});

	it("matches Ctrl+O on non-macOS platforms", () => {
		expect(
			isOpenPreferredEditorShortcut(
				{
					key: "o",
					metaKey: false,
					ctrlKey: true,
					altKey: false,
					shiftKey: false,
					defaultPrevented: false,
				},
				"Linux x86_64",
			),
		).toBe(true);
	});

	it("rejects shifted or alternate variants", () => {
		expect(
			isOpenPreferredEditorShortcut(
				{
					key: "o",
					metaKey: true,
					ctrlKey: false,
					altKey: false,
					shiftKey: true,
					defaultPrevented: false,
				},
				"MacIntel",
			),
		).toBe(false);
		expect(
			isOpenPreferredEditorShortcut(
				{
					key: "o",
					metaKey: false,
					ctrlKey: true,
					altKey: true,
					shiftKey: false,
					defaultPrevented: false,
				},
				"Linux x86_64",
			),
		).toBe(false);
	});

	it("resolves platform-specific labels", () => {
		expect(getOpenPreferredEditorShortcutKeys("MacIntel")).toEqual(["Cmd", "O"]);
		expect(getOpenPreferredEditorShortcutKeys("Win32")).toEqual(["Ctrl", "O"]);
	});
});
