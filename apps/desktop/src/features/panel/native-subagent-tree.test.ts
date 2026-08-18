import { describe, expect, it } from "vitest";
import type { NativeSubagentAnnotation } from "./native-subagent-tree";
import {
	nativeSubagentControlAvailability,
	nativeSubagentDisplayStatus,
	parseNativeSubagentPath,
	projectNativeSubagentTree,
} from "./native-subagent-tree";

function subagent(
	id: string,
	path?: string,
	name?: string,
): NativeSubagentAnnotation {
	return {
		type: "native-subagent",
		id,
		path,
		name,
		status: "running",
	};
}

describe("native subagent tree", () => {
	it("builds nested branches from canonical root paths", () => {
		const scout = subagent("scout", "/root/scout");
		const api = subagent("api", "/root/scout/api");
		const worker = subagent("worker", "/root/worker");
		const projection = projectNativeSubagentTree([scout, api, worker]);

		expect(projection.ungrouped).toEqual([]);
		expect(projection.hierarchicalCount).toBe(3);
		expect(projection.roots).toEqual([
			expect.objectContaining({
				key: "root/scout",
				label: "scout",
				annotation: scout,
				children: [
					expect.objectContaining({
						key: "root/scout/api",
						annotation: api,
					}),
				],
			}),
			expect.objectContaining({
				key: "root/worker",
				annotation: worker,
			}),
		]);
	});

	it("keeps legacy cards ungrouped when hierarchy is absent or malformed", () => {
		const named = subagent("named", undefined, "Reviewer");
		const missing = subagent("missing");
		const malformed = subagent("malformed", "/root/../worker");
		const projection = projectNativeSubagentTree([named, missing, malformed]);

		expect(projection.roots).toEqual([]);
		expect(projection.ungrouped).toEqual([named, missing, malformed]);
	});

	it("keeps the primary Codex thread out of the subagent cards", () => {
		const root = subagent("root-thread", "/root", "/root");
		const child = subagent("child-thread", "/root/reviewer", "/root/reviewer");
		const projection = projectNativeSubagentTree([root, child]);

		expect(projection.hierarchicalCount).toBe(1);
		expect(projection.ungrouped).toEqual([]);
		expect(projection.roots[0]).toMatchObject({
			key: "root/reviewer",
			annotation: child,
		});
	});

	it("recovers the tree for historical events whose path was stored as name", () => {
		const historical = subagent("historical", undefined, "root/reviewer");
		const projection = projectNativeSubagentTree([historical]);

		expect(projection.roots[0]).toMatchObject({
			key: "root/reviewer",
			annotation: historical,
		});
		expect(projection.ungrouped).toEqual([]);
	});

	it("bounds and validates canonical paths", () => {
		expect(parseNativeSubagentPath("/root/scout/api")).toEqual([
			"root",
			"scout",
			"api",
		]);
		expect(parseNativeSubagentPath("other/scout")).toBeNull();
		expect(parseNativeSubagentPath("root//scout")).toBeNull();
		expect(parseNativeSubagentPath("root/./scout")).toBeNull();
		expect(parseNativeSubagentPath("root/scout\nchild")).toBeNull();
	});

	it("offers supervision only for a live child with an addressable thread", () => {
		const running = subagent("running", "/root/worker");
		running.agentThreadId = "thread-worker";
		expect(
			nativeSubagentControlAvailability(running, {
				sessionId: "session-1",
				parentStreaming: true,
				supportsSteering: true,
				supportsInterrupt: true,
			}),
		).toEqual({ canSteer: true, canInterrupt: true });

		running.status = "completed";
		expect(
			nativeSubagentControlAvailability(running, {
				sessionId: "session-1",
				parentStreaming: true,
				supportsSteering: true,
				supportsInterrupt: true,
			}),
		).toEqual({ canSteer: false, canInterrupt: false });
	});

	it("renders stale historical running states as neutrally settled", () => {
		const running = subagent("running", "/root/worker");
		expect(nativeSubagentDisplayStatus(running, true)).toBe("running");
		expect(nativeSubagentDisplayStatus(running, false)).toBe("settled");

		running.status = "completed";
		expect(nativeSubagentDisplayStatus(running, false)).toBe("completed");
	});
});
