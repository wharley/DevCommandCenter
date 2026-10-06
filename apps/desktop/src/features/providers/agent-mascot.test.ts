import { describe, expect, it } from "vitest";
import {
	AGENT_MASCOT_KEYS,
	agentMascotPaths,
	agentMascotRows,
	MASCOT_GRID,
} from "@dcc/mascots";
import { resolveIconKey } from "./provider-icons";

describe("agent mascots", () => {
	it("draws every pose on the 10×10 grid", () => {
		for (const state of ["idle", "working", "waiting"] as const) {
			for (const rows of agentMascotRows(state)) {
				expect(rows).toHaveLength(MASCOT_GRID);
				for (const row of rows) {
					expect(row).toMatch(/^[.#+]{10}$/);
				}
			}
		}
	});

	// Row 3 is the eye line: two eyes when facing you, one near the front
	// (right) when walking toward the model label.
	const eyes = (rows: readonly string[]) =>
		[...rows[3]!.slice(2, 8)].flatMap((pixel, x) => (pixel === "." ? [x] : []));

	it("faces you when the turn is over or when it needs you", () => {
		for (const state of ["idle", "waiting"] as const) {
			for (const rows of agentMascotRows(state)) {
				expect(eyes(rows)).toEqual([1, 4]);
			}
		}
	});

	it("turns right, toward the model label, while working", () => {
		for (const rows of agentMascotRows("working")) {
			expect(eyes(rows)).toEqual([4]);
		}
	});

	it("gives each provider its own antenna", () => {
		const antennas = new Set(
			AGENT_MASCOT_KEYS.map((key) => agentMascotPaths(key, "idle").rest.accent),
		);
		expect(antennas.size).toBe(AGENT_MASCOT_KEYS.length);
	});

	it("only moves when working or waiting", () => {
		const idle = agentMascotPaths("claude", "idle");
		expect(idle.move).toEqual(idle.rest);
		for (const state of ["working", "waiting"] as const) {
			const paths = agentMascotPaths("claude", state);
			expect(paths.move).not.toEqual(paths.rest);
		}
	});

	it("maps every brand mark to a robot", () => {
		for (const provider of ["claude_code", "codex", "gemini", "cursor", "droid", "grok"]) {
			expect(AGENT_MASCOT_KEYS).toContain(resolveIconKey(provider));
		}
	});
});
