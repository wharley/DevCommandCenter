import { afterEach, describe, expect, it, vi } from "vitest";
import {
	dispatchWorkspaceDiffAnnotation,
	subscribeWorkspaceDiffAnnotation,
	type WorkspaceDiffAnnotationCommand,
} from "./workspace-diff-annotation-command";

describe("workspace diff annotation commands", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("dispatches an annotation to its workspace and supports unsubscribe", () => {
		vi.stubGlobal("window", new EventTarget());
		const received: WorkspaceDiffAnnotationCommand[] = [];
		const unsubscribe = subscribeWorkspaceDiffAnnotation((command) =>
			received.push(command),
		);
		const command: WorkspaceDiffAnnotationCommand = {
			workspaceId: "workspace-1",
			targetSessionId: "session-2",
			pending: {
				request: {
					path: "src/app.ts",
					side: "modified",
					startLine: 7,
					endLine: 8,
					snippet: "const ready = true;",
				},
				anchor: { top: 120, left: 240 },
			},
		};

		dispatchWorkspaceDiffAnnotation(command);
		unsubscribe();
		dispatchWorkspaceDiffAnnotation({ ...command, workspaceId: "workspace-2" });

		expect(received).toEqual([command]);
	});
});
