import { describe, expect, it } from "vitest";
import {
	dataUrlByteLength,
	fitImageZoom,
	formatImageBytes,
	isViewableImagePath,
	stepImageZoom,
	workspaceAbsolutePath,
} from "./image-file-view.logic";

describe("image file view", () => {
	it("shows raster images and leaves svg and text to the editor", () => {
		expect(isViewableImagePath("e2e/shots/configuracoes-painel.png")).toBe(
			true,
		);
		expect(isViewableImagePath("foto.JPEG")).toBe(true);
		expect(isViewableImagePath("anim.gif")).toBe(true);
		expect(isViewableImagePath("icon.svg")).toBe(false);
		expect(isViewableImagePath("README.md")).toBe(false);
		expect(isViewableImagePath("png")).toBe(false);
	});

	it("joins the tab path onto the workspace root", () => {
		expect(workspaceAbsolutePath("/repo/", "a/b.png")).toBe("/repo/a/b.png");
		expect(workspaceAbsolutePath("/repo", "./b.png")).toBe("/repo/b.png");
		expect(workspaceAbsolutePath("/repo", "/tmp/x.png")).toBe("/tmp/x.png");
	});

	it("fits large images and keeps small ones at 100%", () => {
		expect(
			fitImageZoom(
				{ width: 2000, height: 1000 },
				{ width: 1000, height: 1000 },
			),
		).toBe(0.5);
		expect(
			fitImageZoom({ width: 200, height: 100 }, { width: 1000, height: 1000 }),
		).toBe(1);
		expect(
			fitImageZoom({ width: 0, height: 0 }, { width: 1000, height: 1000 }),
		).toBe(1);
	});

	it("steps through preset zoom levels", () => {
		expect(stepImageZoom(1, 1)).toBe(1.25);
		expect(stepImageZoom(1, -1)).toBe(0.75);
		expect(stepImageZoom(0.43, 1)).toBe(0.5);
		expect(stepImageZoom(16, 1)).toBe(16);
		expect(stepImageZoom(0.05, -1)).toBe(0.05);
	});

	it("reports file size from the data url", () => {
		expect(dataUrlByteLength("data:image/png;base64,AAAA")).toBe(3);
		expect(dataUrlByteLength("data:image/png;base64,AAA=")).toBe(2);
		expect(formatImageBytes(512)).toBe("512 B");
		expect(formatImageBytes(2048)).toBe("2.0 KB");
	});
});
