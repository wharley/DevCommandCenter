import { describe, expect, it } from "vitest";
import type { WorkspaceMessage } from "./thread-projection";
import { collectPendingPermissionRequests } from "./pending-permissions";

function assistantMessage(
	annotations: NonNullable<WorkspaceMessage["annotations"]>,
): WorkspaceMessage {
	return {
		id: "assistant-1",
		role: "assistant",
		label: "Assistant",
		content: "",
		annotations,
	};
}

describe("collectPendingPermissionRequests", () => {
	it("returns only unresolved live approvals", () => {
		const result = collectPendingPermissionRequests([
			assistantMessage([
				{
					type: "approval",
					id: "pending",
					toolName: "Bash",
					command: "npm test",
					streaming: true,
				},
				{
					type: "approval",
					id: "allowed",
					toolName: "Write",
					behavior: "allow",
					streaming: false,
				},
				{
					type: "user-input",
					id: "question",
					questions: [],
					answers: [],
					streaming: true,
				},
			]),
		]);

		expect(result).toEqual([
			expect.objectContaining({
				id: "pending",
				toolName: "Bash",
			}),
		]);
	});

	it("keeps request order and ignores duplicate event projections", () => {
		const duplicate = {
			type: "approval" as const,
			id: "permission-1",
			toolName: "Bash",
			streaming: true,
		};

		const result = collectPendingPermissionRequests([
			assistantMessage([duplicate]),
			{
				...assistantMessage([
					duplicate,
					{
						type: "approval",
						id: "permission-2",
						toolName: "Write",
						streaming: true,
					},
				]),
				id: "assistant-2",
			},
		]);

		expect(result.map((request) => request.id)).toEqual([
			"permission-1",
			"permission-2",
		]);
	});
});
