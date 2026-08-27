import { describe, expect, it } from "vitest";
import {
	getOpenPreferredEditorShortcutKeys,
	getCommandPaletteShortcutKeys,
	getLegacyCommandPaletteShortcutKeys,
	getFocusComposerShortcutKeys,
	getToggleTerminalShortcutKeys,
	getInspectorCodeModeShortcutKeys,
	getInspectorGitModeShortcutKeys,
	getQuickOpenShortcutKeys,
	getWorkspaceSearchShortcutKeys,
	isInspectorCodeModeShortcut,
	isInspectorGitModeShortcut,
	isOpenPreferredEditorShortcut,
	isCommandPaletteShortcut,
	isFocusComposerShortcut,
	isQuickOpenShortcut,
	isWorkspaceSearchShortcut,
	isToggleTerminalShortcut,
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

	it("matches Quick Open (Cmd/Ctrl+P) per platform", () => {
		expect(
			isQuickOpenShortcut(
				{
					key: "p",
					metaKey: true,
					ctrlKey: false,
					altKey: false,
					shiftKey: false,
					defaultPrevented: false,
				},
				"MacIntel",
			),
		).toBe(true);
		expect(
			isQuickOpenShortcut(
				{
					key: "p",
					metaKey: false,
					ctrlKey: true,
					altKey: false,
					shiftKey: false,
					defaultPrevented: false,
				},
				"Linux x86_64",
			),
		).toBe(true);
		// Shift+Cmd+P is a different command surface and must not trigger Quick Open.
		expect(
			isQuickOpenShortcut(
				{
					key: "p",
					metaKey: true,
					ctrlKey: false,
					altKey: false,
					shiftKey: true,
					defaultPrevented: false,
				},
				"MacIntel",
			),
		).toBe(false);
		expect(getQuickOpenShortcutKeys("MacIntel")).toEqual(["Cmd", "P"]);
		expect(getQuickOpenShortcutKeys("Win32")).toEqual(["Ctrl", "P"]);
	});

	it("matches workspace search (Cmd/Ctrl+Shift+F) and requires Shift", () => {
		expect(
			isWorkspaceSearchShortcut(
				{
					key: "f",
					metaKey: true,
					ctrlKey: false,
					altKey: false,
					shiftKey: true,
					defaultPrevented: false,
				},
				"MacIntel",
			),
		).toBe(true);
		expect(
			isWorkspaceSearchShortcut(
				{
					key: "f",
					metaKey: false,
					ctrlKey: true,
					altKey: false,
					shiftKey: true,
					defaultPrevented: false,
				},
				"Linux x86_64",
			),
		).toBe(true);
		// Without Shift this is the in-file find, not workspace search.
		expect(
			isWorkspaceSearchShortcut(
				{
					key: "f",
					metaKey: true,
					ctrlKey: false,
					altKey: false,
					shiftKey: false,
					defaultPrevented: false,
				},
				"MacIntel",
			),
		).toBe(false);
		expect(getWorkspaceSearchShortcutKeys("MacIntel")).toEqual([
			"Cmd",
			"Shift",
			"F",
		]);
		expect(getWorkspaceSearchShortcutKeys("Win32")).toEqual([
			"Ctrl",
			"Shift",
			"F",
		]);
	});

	it("matches inspector mode shortcuts and resolves platform labels", () => {
		expect(
			isInspectorGitModeShortcut(
				{
					key: "g",
					metaKey: true,
					ctrlKey: false,
					altKey: false,
					shiftKey: true,
					defaultPrevented: false,
				},
				"MacIntel",
			),
		).toBe(true);
		expect(
			isInspectorCodeModeShortcut(
				{
					key: "e",
					metaKey: false,
					ctrlKey: true,
					altKey: false,
					shiftKey: true,
					defaultPrevented: false,
				},
				"Linux x86_64",
			),
		).toBe(true);
		expect(
			isInspectorCodeModeShortcut(
				{
					key: "e",
					metaKey: false,
					ctrlKey: true,
					altKey: false,
					shiftKey: false,
					defaultPrevented: false,
				},
				"Linux x86_64",
			),
		).toBe(false);
		expect(getInspectorGitModeShortcutKeys("MacIntel")).toEqual([
			"Cmd",
			"Shift",
			"G",
		]);
		expect(getInspectorCodeModeShortcutKeys("Win32")).toEqual([
			"Ctrl",
			"Shift",
			"E",
		]);
	});

	it("matches command palette, composer focus, and terminal shortcuts", () => {
		const macEvent = (key: string, shiftKey: boolean) => ({
			key,
			metaKey: true,
			ctrlKey: false,
			altKey: false,
			shiftKey,
			defaultPrevented: false,
		});
		expect(isCommandPaletteShortcut(macEvent("k", false), "MacIntel")).toBe(true);
		expect(isCommandPaletteShortcut(macEvent("p", true), "MacIntel")).toBe(true);
		expect(
			isCommandPaletteShortcut(
				{
					key: "k",
					metaKey: false,
					ctrlKey: true,
					altKey: false,
					shiftKey: false,
					defaultPrevented: false,
				},
				"Win32",
			),
		).toBe(true);
		expect(isCommandPaletteShortcut(macEvent("k", true), "MacIntel")).toBe(false);
		expect(
			isCommandPaletteShortcut(
				{ ...macEvent("k", false), defaultPrevented: true },
				"MacIntel",
			),
		).toBe(false);
		expect(isFocusComposerShortcut(macEvent("l", true), "MacIntel")).toBe(true);
		expect(isToggleTerminalShortcut(macEvent("j", false), "MacIntel")).toBe(true);
		expect(isToggleTerminalShortcut(macEvent("j", true), "MacIntel")).toBe(false);
		expect(getCommandPaletteShortcutKeys("Win32")).toEqual(["Ctrl", "K"]);
		expect(getLegacyCommandPaletteShortcutKeys("Win32")).toEqual([
			"Ctrl",
			"Shift",
			"P",
		]);
		expect(getFocusComposerShortcutKeys("MacIntel")).toEqual(["Cmd", "Shift", "L"]);
		expect(getToggleTerminalShortcutKeys("Linux")).toEqual(["Ctrl", "J"]);
	});
});
