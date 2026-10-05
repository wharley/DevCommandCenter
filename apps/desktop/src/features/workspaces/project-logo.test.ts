import { describe, expect, it } from "vitest";
import { projectIconValue, resolveProjectIcon } from "@dcc/mascots";
import { squareCrop } from "./project-logo";

describe("project logo", () => {
	it("center-crops the largest square", () => {
		expect(squareCrop(200, 100)).toEqual({ x: 50, y: 0, side: 100 });
		expect(squareCrop(64, 128)).toEqual({ x: 0, y: 32, side: 64 });
		expect(squareCrop(48, 48)).toEqual({ x: 0, y: 0, side: 48 });
	});

	it("renders the uploaded logo only when it is the pick", () => {
		const logo = "data:image/png;base64,iVBORw0KGgo=";
		expect(projectIconValue({ icon: "logo", logo })).toBe(logo);
		expect(projectIconValue({ icon: "capivara", logo })).toBe("capivara");
		// A stale "logo" pick without an image falls back to the auto mascot.
		expect(projectIconValue({ icon: "logo", logo: null })).toBe("logo");
		expect(resolveProjectIcon("logo", "/repo").kind).toBe("mascot");
		expect(resolveProjectIcon(logo, "/repo")).toEqual({ kind: "logo", src: logo });
		// Only PNG data URIs are treated as images.
		expect(resolveProjectIcon("data:image/svg+xml;base64,PHN2Zz4=", "/repo").kind).toBe("mascot");
	});
});
