import { describe, expect, it } from "vitest";
import { projectColorId, resolveProjectIcon } from "./project-identity";
import {
	autoProjectMascot,
	MASCOT_GRID,
	PROJECT_MASCOT_IDS,
	projectMascotPaths,
} from "@dcc/mascots";

describe("project visual identity", () => {
	it("keeps explicit picks", () => {
		expect(resolveProjectIcon("rocket", "/repo")).toEqual({ kind: "icon", id: "rocket" });
		expect(resolveProjectIcon("folder", "/repo")).toEqual({ kind: "icon", id: "folder" });
		expect(resolveProjectIcon("capivara", "/repo")).toEqual({ kind: "mascot", id: "capivara" });
		expect(projectColorId("violet", "/repo")).toBe("violet");
		expect(projectColorId("slate", "/repo")).toBe("slate");
	});

	it("derives a stable mascot and color from the seed when unset", () => {
		const icon = resolveProjectIcon(null, "/Users/dev/projetos/dcc");
		expect(icon).toEqual(resolveProjectIcon(undefined, "/Users/dev/projetos/dcc"));
		expect(icon.kind).toBe("mascot");
		expect(resolveProjectIcon("custom-svg", "/Users/dev/projetos/dcc")).toEqual(icon);

		const color = projectColorId(null, "/Users/dev/projetos/dcc");
		expect(color).toBe(projectColorId("transparent", "/Users/dev/projetos/dcc"));
		expect(["slate", "amber"]).not.toContain(color);
	});

	it("spreads different projects across the fauna", () => {
		const picks = new Set(
			Array.from({ length: 40 }, (_, index) => autoProjectMascot(`/repos/project-${index}`)),
		);
		expect(picks.size).toBeGreaterThanOrEqual(6);
	});

	it("falls back without a seed", () => {
		expect(resolveProjectIcon(null)).toEqual({ kind: "mascot", id: "polvo" });
		expect(projectColorId(null)).toBe("slate");
	});

	it("builds non-empty sprite paths inside the grid for every frame", () => {
		for (const id of PROJECT_MASCOT_IDS) {
			const paths = projectMascotPaths(id);
			for (const frame of [paths.rest, paths.move]) {
				expect(frame.body.length).toBeGreaterThan(0);
				const coords = [...`${frame.body}${frame.accent}`.matchAll(/M(\d+) (\d+)h(\d+)/g)];
				for (const [, x, y, width] of coords) {
					expect(Number(x) + Number(width)).toBeLessThanOrEqual(MASCOT_GRID);
					expect(Number(y)).toBeLessThan(MASCOT_GRID);
				}
			}
		}
	});
});
