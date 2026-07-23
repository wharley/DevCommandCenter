import { describe, expect, it } from "vitest";
import type { CoreEvent, SessionEventRecord } from "@dcc/contracts";
import {
	mergeSessionThreadEvents,
	projectWorkspaceMessages,
} from "./session-thread-history.logic";

function sessionTurnStarted(
	sessionId: string,
	turnId: string,
	prompt: string,
	occurredAt = "2026-05-01T12:00:00Z",
	planMode?: boolean,
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-started`,
		sessionId,
		sequence: 1,
		occurredAt,
		kind: {
			type: "turn_started",
			turnId,
			prompt,
			planMode: planMode ?? null,
		},
	};
}

function sessionTurnDelta(
	sessionId: string,
	turnId: string,
	content: string,
): CoreEvent {
	return {
		sessionTurnDelta: {
			session_id: sessionId,
			turn_id: turnId,
			content,
		},
	};
}

function sessionTurnCompleted(
	sessionId: string,
	turnId: string,
	occurredAt = "2026-05-01T12:00:05Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-completed`,
		sessionId,
		sequence: 2,
		occurredAt,
		kind: {
			type: "turn_completed",
			turnId,
		},
	};
}

function sessionTurnDeltaRecord(
	sessionId: string,
	turnId: string,
	content: string,
	sequence = 2,
	occurredAt = "2026-05-01T12:00:01Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-delta-${sequence}`,
		sessionId,
		sequence,
		occurredAt,
		kind: {
			type: "turn_delta",
			turnId,
			content,
		},
	};
}

function sessionTurnReasoningStarted(
	sessionId: string,
	turnId: string,
	reasoningId: string,
	label = "Thinking",
	sequence = 2,
	occurredAt = "2026-05-01T12:00:01Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${reasoningId}-reasoning-started`,
		sessionId,
		sequence,
		occurredAt,
		kind: {
			type: "turn_reasoning_started",
			turnId,
			reasoningId,
			label,
		},
	};
}

function sessionTurnReasoningDelta(
	sessionId: string,
	turnId: string,
	reasoningId: string,
	content: string,
	sequence = 3,
	occurredAt = "2026-05-01T12:00:02Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${reasoningId}-reasoning-delta-${sequence}`,
		sessionId,
		sequence,
		occurredAt,
		kind: {
			type: "turn_reasoning_delta",
			turnId,
			reasoningId,
			content,
		},
	};
}

function sessionTurnReasoningCompleted(
	sessionId: string,
	turnId: string,
	reasoningId: string,
	sequence = 4,
	occurredAt = "2026-05-01T12:00:03Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${reasoningId}-reasoning-completed`,
		sessionId,
		sequence,
		occurredAt,
		kind: {
			type: "turn_reasoning_completed",
			turnId,
			reasoningId,
		},
	};
}

function sessionTurnToolCallStarted(
	sessionId: string,
	turnId: string,
	toolCallId: string,
	action: string,
	command?: string,
	file?: string,
	sequence = 2,
	occurredAt = "2026-05-01T12:00:01Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${toolCallId}-tool-started`,
		sessionId,
		sequence,
		occurredAt,
		kind: {
			type: "turn_tool_call_started",
			turnId,
			toolCallId,
			action,
			command: command ?? null,
			file: file ?? null,
		},
	};
}

function sessionTurnToolCallDelta(
	sessionId: string,
	turnId: string,
	toolCallId: string,
	content: string,
	sequence = 3,
	occurredAt = "2026-05-01T12:00:02Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${toolCallId}-tool-delta-${sequence}`,
		sessionId,
		sequence,
		occurredAt,
		kind: {
			type: "turn_tool_call_delta",
			turnId,
			toolCallId,
			content,
		},
	};
}

function sessionTurnToolCallCompleted(
	sessionId: string,
	turnId: string,
	toolCallId: string,
	sequence = 4,
	occurredAt = "2026-05-01T12:00:03Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${toolCallId}-tool-completed`,
		sessionId,
		sequence,
		occurredAt,
		kind: {
			type: "turn_tool_call_completed",
			turnId,
			toolCallId,
		},
	};
}

function sessionTurnAborted(
	sessionId: string,
	turnId: string,
	reason: string | null,
	occurredAt = "2026-05-01T12:00:05Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-aborted`,
		sessionId,
		sequence: 2,
		occurredAt,
		kind: {
			type: "turn_aborted",
			turnId,
			reason,
		},
	};
}

function sessionTurnPermissionRequested(
	sessionId: string,
	turnId: string,
	requestId: string,
	toolName: string,
	command?: string,
	file?: string,
	sequence = 2,
	occurredAt = "2026-05-01T12:00:01Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${requestId}-permission-requested`,
		sessionId,
		sequence,
		occurredAt,
		kind: {
			type: "turn_permission_requested",
			turnId,
			requestId,
			toolName,
			title: "Run command",
			description: "Agent requests approval before continuing.",
			command: command ?? null,
			file: file ?? null,
		},
	};
}

function sessionTurnPermissionResolved(
	sessionId: string,
	turnId: string,
	requestId: string,
	behavior: string,
	sequence = 3,
	occurredAt = "2026-05-01T12:00:02Z",
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${requestId}-permission-resolved`,
		sessionId,
		sequence,
		occurredAt,
		kind: {
			type: "turn_permission_resolved",
			turnId,
			requestId,
			behavior,
		},
	};
}

describe("projectWorkspaceMessages", () => {
	it("does not project another workspace's events when no session is selected", () => {
		expect(
			projectWorkspaceMessages(
				[
					sessionTurnStarted(
						"session-old",
						"turn-plan",
						"Create a plan",
						"2026-05-01T12:00:00Z",
						true,
					),
					sessionTurnDeltaRecord(
						"session-old",
						"turn-plan",
						"# Old workspace plan\n\n## Steps\n- [ ] Keep this isolated",
					),
					sessionTurnCompleted("session-old", "turn-plan"),
				],
				[
					sessionTurnDelta(
						"session-old",
						"turn-plan",
						"# Live plan from the old workspace",
					),
				],
				null,
			),
		).toEqual([]);
	});

	it("filters history records to the current session and preserves timestamps", () => {
		expect(
			projectWorkspaceMessages(
				[
					sessionTurnStarted("session-a", "turn-1", "Alpha"),
					sessionTurnStarted("session-b", "turn-2", "Beta"),
				],
				[],
				"session-a",
			),
		).toEqual([
			{
				id: "user-session-a-turn-1",
				role: "user",
				label: "User",
				content: "Alpha",
				createdAt: "2026-05-01T12:00:00Z",
				planMode: false,
			},
		]);
	});

	it("merges live assistant deltas into a streamed assistant message", () => {
		expect(
			projectWorkspaceMessages(
				[sessionTurnStarted("session-a", "turn-1", "Alpha")],
				[
					sessionTurnDelta("session-a", "turn-1", "Hello"),
					sessionTurnDelta("session-a", "turn-1", " world"),
				],
				"session-a",
			),
		).toEqual([
			{
				id: "user-session-a-turn-1",
				role: "user",
				label: "User",
				content: "Alpha",
				createdAt: "2026-05-01T12:00:00Z",
				planMode: false,
			},
			{
				id: "assistant-session-a-turn-1",
				role: "assistant",
				label: "Assistant",
				content: "Hello world",
				streaming: true,
				createdAt: "2026-05-01T12:00:00Z",
			},
		]);
	});

	it("preserves repeated identical streaming deltas", () => {
		expect(
			projectWorkspaceMessages(
				[sessionTurnStarted("session-a", "turn-1", "Alpha")],
				[
					sessionTurnDelta("session-a", "turn-1", "Vou"),
					sessionTurnDelta("session-a", "turn-1", " "),
					sessionTurnDelta("session-a", "turn-1", "rastrear"),
					sessionTurnDelta("session-a", "turn-1", " "),
					sessionTurnDelta("session-a", "turn-1", "onde"),
					sessionTurnDelta("session-a", "turn-1", " "),
					sessionTurnDelta("session-a", "turn-1", "o"),
				],
				"session-a",
			)[1],
		).toMatchObject({
			role: "assistant",
			content: "Vou rastrear onde o",
		});
	});

	it("deduplicates live deltas already present in history by occurrence", () => {
		expect(
			projectWorkspaceMessages(
				[
					sessionTurnStarted("session-a", "turn-1", "Alpha"),
					sessionTurnDeltaRecord("session-a", "turn-1", "Vou", 2),
					sessionTurnDeltaRecord("session-a", "turn-1", " ", 3),
					sessionTurnDeltaRecord("session-a", "turn-1", "rastrear", 4),
					sessionTurnDeltaRecord("session-a", "turn-1", " ", 5),
					sessionTurnDeltaRecord("session-a", "turn-1", "onde", 6),
				],
				[
					sessionTurnDelta("session-a", "turn-1", "Vou"),
					sessionTurnDelta("session-a", "turn-1", " "),
					sessionTurnDelta("session-a", "turn-1", "rastrear"),
					sessionTurnDelta("session-a", "turn-1", " "),
					sessionTurnDelta("session-a", "turn-1", "onde"),
				],
				"session-a",
			)[1],
		).toMatchObject({
			role: "assistant",
			content: "Vou rastrear onde",
		});
	});

	it("projects reasoning and tool call annotations into the assistant message", () => {
		expect(
			projectWorkspaceMessages(
				[
					sessionTurnStarted("session-a", "turn-1", "Alpha"),
					sessionTurnReasoningStarted("session-a", "turn-1", "reasoning-1"),
					sessionTurnReasoningDelta("session-a", "turn-1", "reasoning-1", "Thinking through the repo."),
					sessionTurnReasoningCompleted("session-a", "turn-1", "reasoning-1"),
					sessionTurnToolCallStarted(
						"session-a",
						"turn-1",
						"tool-call-1",
						"Read file",
						"cat src/main.ts",
						"src/main.ts",
					),
					sessionTurnToolCallDelta("session-a", "turn-1", "tool-call-1", "Reading the file."),
					sessionTurnToolCallCompleted("session-a", "turn-1", "tool-call-1"),
					sessionTurnDeltaRecord("session-a", "turn-1", "Hello from assistant"),
					sessionTurnCompleted("session-a", "turn-1"),
				],
				[],
				"session-a",
			),
		).toEqual([
			{
				id: "user-session-a-turn-1",
				role: "user",
				label: "User",
				content: "Alpha",
				createdAt: "2026-05-01T12:00:00Z",
				planMode: false,
			},
			{
				id: "assistant-session-a-turn-1",
				role: "assistant",
				label: "Assistant",
				content: "Hello from assistant",
				streaming: false,
				createdAt: "2026-05-01T12:00:00Z",
				annotations: [
					{
						type: "reasoning",
						id: "reasoning-1",
						label: "Thinking",
						content: "Thinking through the repo.",
						streaming: false,
						createdAt: "2026-05-01T12:00:01Z",
					},
					{
						type: "tool-call",
						id: "tool-call-1",
						action: "Read file",
						command: "cat src/main.ts",
						file: "src/main.ts",
						content: "Reading the file.",
						streaming: false,
						createdAt: "2026-05-01T12:00:01Z",
					},
				],
			},
		]);
	});

	it("extracts a structured plan from assistant content when planMode is true", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Plan request", "2026-05-01T12:00:00Z", true),
				sessionTurnCompleted("session-a", "turn-1"),
			],
			[
				sessionTurnDelta(
					"session-a",
					"turn-1",
					"# Mission Plan\n\nUpdate the UI.\n\n## Steps\n- [x] Inspect the existing chat flow\n- [ ] Add the plan card\n- [ ] Run npx tsc\n",
				),
			],
			"session-a",
		);

		expect(messages[1]).toMatchObject({
			role: "assistant",
			plan: {
				title: "Mission Plan",
				summary: "Update the UI.",
				isPlanLike: true,
				steps: [
					{
						text: "Inspect the existing chat flow",
						status: "completed",
					},
					{
						text: "Add the plan card",
						status: "pending",
					},
					{
						text: "Run npx tsc",
						status: "pending",
					},
				],
			},
		});
		expect(messages[1].plan?.approvedPrompts).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					command: "npx tsc",
					source: "plan",
				}),
			]),
		);
	});

	it("marks aborted assistant messages as incomplete", () => {
		expect(
			projectWorkspaceMessages(
				[
					sessionTurnStarted("session-a", "turn-1", "Alpha"),
					sessionTurnAborted("session-a", "turn-1", "Stopped"),
				],
				[sessionTurnDelta("session-a", "turn-1", "Hello")],
				"session-a",
			),
		).toEqual([
			{
				id: "user-session-a-turn-1",
				role: "user",
				label: "User",
				content: "Alpha",
				createdAt: "2026-05-01T12:00:00Z",
				planMode: false,
			},
			{
				id: "assistant-session-a-turn-1",
				role: "assistant",
				label: "Assistant",
				content: "Hello",
				streaming: false,
				status: {
					type: "incomplete",
					reason: "Stopped",
				},
				createdAt: "2026-05-01T12:00:00Z",
			},
		]);
	});

	it("projects approval requests and resolutions into assistant annotations", () => {
		expect(
			projectWorkspaceMessages(
				[
					sessionTurnStarted("session-a", "turn-1", "Alpha"),
					sessionTurnPermissionRequested(
						"session-a",
						"turn-1",
						"perm-1",
						"Bash",
						"npm test",
						"package.json",
					),
					sessionTurnPermissionResolved(
						"session-a",
						"turn-1",
						"perm-1",
						"allow",
					),
					sessionTurnDeltaRecord("session-a", "turn-1", "Continuing after approval."),
					sessionTurnCompleted("session-a", "turn-1"),
				],
				[],
				"session-a",
			),
		).toEqual([
			{
				id: "user-session-a-turn-1",
				role: "user",
				label: "User",
				content: "Alpha",
				createdAt: "2026-05-01T12:00:00Z",
				planMode: false,
			},
			{
				id: "assistant-session-a-turn-1",
				role: "assistant",
				label: "Assistant",
				content: "Continuing after approval.",
				streaming: false,
				createdAt: "2026-05-01T12:00:00Z",
				annotations: [
					{
						type: "approval",
						id: "perm-1",
						toolName: "Bash",
						title: "Run command",
						description: "Agent requests approval before continuing.",
						command: "npm test",
						file: "package.json",
						behavior: "allow",
						streaming: false,
						createdAt: "2026-05-01T12:00:01Z",
					},
				],
			},
		]);
	});

	it("appends a pending prompt when the runtime has not emitted it yet", () => {
		expect(projectWorkspaceMessages([], [], "session-a", "Draft prompt")).toEqual([
			{
				id: "pending-user-session-a",
				role: "user",
				label: "User",
				content: "Draft prompt",
			},
		]);
	});

	it("does not duplicate a pending prompt already present in history", () => {
		expect(
			projectWorkspaceMessages(
				[sessionTurnStarted("session-a", "turn-1", "Draft prompt")],
				[],
				"session-a",
				"Draft prompt",
			),
		).toEqual([
			{
				id: "user-session-a-turn-1",
				role: "user",
				label: "User",
				content: "Draft prompt",
				createdAt: "2026-05-01T12:00:00Z",
				planMode: false,
			},
		]);
	});

	it("reconstructs a persisted plan approval as a session event", () => {
		const approval: SessionEventRecord = {
			eventId: "event-plan-approved",
			sessionId: "session-a",
			sequence: 3,
			occurredAt: "2026-05-01T12:00:06Z",
			kind: {
				type: "plan_approved",
				planMessageId: "assistant-session-a-turn-1",
				planVersion: 1,
				planHash: "fnv1a32:12345678",
			},
		};

		expect(mergeSessionThreadEvents([approval], [], "session-a")).toEqual([
			expect.objectContaining({
				event: {
					sessionPlanApproved: {
						session_id: "session-a",
						plan_message_id: "assistant-session-a-turn-1",
						plan_version: 1,
						plan_hash: "fnv1a32:12345678",
					},
				},
				occurredAt: "2026-05-01T12:00:06Z",
			}),
		]);
	});
});
