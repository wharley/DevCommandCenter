import { describe, expect, it } from "vitest";
import { formatDiskBytes, workspaceDiskUsageIds } from "./workspace-disk-usage";
import type { WorkspaceSummary } from "./types";

function workspace(
	id: string,
	memberWorkspaceIds?: string[],
): WorkspaceSummary {
	return {
		id,
		name: id,
		branch: id,
		status: "completed",
		memberWorkspaceIds,
	};
}

describe("workspaceDiskUsageIds", () => {
	it("expands bundle members and removes duplicate ids", () => {
		expect(
			workspaceDiskUsageIds([
				workspace("standalone"),
				workspace("bundle", ["member-a", "member-b"]),
				workspace("duplicate", ["member-b"]),
			]),
		).toEqual(["standalone", "member-a", "member-b"]);
	});
});

describe("formatDiskBytes", () => {
	it("formats compact binary units using the requested locale", () => {
		expect(formatDiskBytes(1_288_490_188, "pt-BR")).toBe("1,2 GB");
		expect(formatDiskBytes(512 * 1024, "en-US")).toBe("512 KB");
	});
});
