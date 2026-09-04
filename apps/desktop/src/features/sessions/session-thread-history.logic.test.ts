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
	model?: string,
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
			model: model ?? null,
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
	sequence = 2,
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-completed`,
		sessionId,
		sequence,
		occurredAt,
		kind: {
			type: "turn_completed",
			turnId,
		},
	};
}

function assistantMessageStarted(
	sessionId: string,
	turnId: string,
	messageId: string,
	phase: "commentary" | "final_answer" | "unknown",
	sequence: number,
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${messageId}-started`,
		sessionId,
		sequence,
		occurredAt: `2026-05-01T12:00:0${sequence}Z`,
		kind: {
			type: "turn_assistant_message_started",
			turnId,
			messageId,
			phase,
		},
	};
}

function assistantMessageDelta(
	sessionId: string,
	turnId: string,
	messageId: string,
	content: string,
	sequence: number,
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${messageId}-delta-${sequence}`,
		sessionId,
		sequence,
		occurredAt: `2026-05-01T12:00:0${sequence}Z`,
		kind: {
			type: "turn_assistant_message_delta",
			turnId,
			messageId,
			content,
		},
	};
}

function assistantMessageCompleted(
	sessionId: string,
	turnId: string,
	messageId: string,
	phase: "commentary" | "final_answer" | "unknown",
	content: string | null,
	sequence: number,
): SessionEventRecord {
	return {
		eventId: `evt-${sessionId}-${turnId}-${messageId}-completed`,
		sessionId,
		sequence,
		occurredAt: `2026-05-01T12:00:0${sequence}Z`,
		kind: {
			type: "turn_assistant_message_completed",
			turnId,
			messageId,
			phase,
			content,
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
				turnId: "turn-1",
				label: "User",
				content: "Alpha",
				createdAt: "2026-05-01T12:00:00Z",
				planMode: false,
			},
		]);
	});

	it("carries evidence metadata from the turn record to the user message", () => {
		const record = sessionTurnStarted("session-a", "turn-1", "Why?");
		const evidence = {
			stage: "investigate" as const,
			items: [
				{ source: "browser" as const, trust: "remote_untrusted" as const, chars: 900, truncated: false },
			],
		};
		const withEvidence: SessionEventRecord = {
			...record,
			kind: { ...record.kind, evidence } as SessionEventRecord["kind"],
		};
		const [message] = projectWorkspaceMessages([withEvidence], [], "session-a");
		expect(message.role).toBe("user");
		expect(message.evidence).toEqual(evidence);
		const [plain] = projectWorkspaceMessages([record], [], "session-a");
		expect("evidence" in plain).toBe(false);
		expect("retryOfTurnId" in plain).toBe(false);
		const retried: SessionEventRecord = {
			...record,
			kind: { ...record.kind, retryOfTurnId: "turn-0" } as SessionEventRecord["kind"],
		};
		const [retry] = projectWorkspaceMessages([retried], [], "session-a");
		expect(retry.retryOfTurnId).toBe("turn-0");
		expect(retry.turnId).toBe("turn-1");
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
				turnId: "turn-1",
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

	it("folds live commentary into activity while streaming one final answer row", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted(
					"session-a",
					"turn-1",
					"Alpha",
					"2026-05-01T12:00:00Z",
					false,
					"claude-fable-5",
				),
				assistantMessageStarted("session-a", "turn-1", "comment-1", "commentary", 2),
				assistantMessageDelta("session-a", "turn-1", "comment-1", "Vou investigar.", 3),
				assistantMessageCompleted(
					"session-a",
					"turn-1",
					"comment-1",
					"commentary",
					"Vou investigar.",
					4,
				),
				assistantMessageStarted("session-a", "turn-1", "final-1", "final_answer", 5),
				assistantMessageDelta("session-a", "turn-1", "final-1", "Resolvido.", 6),
			],
			[],
			"session-a",
		);

		expect(messages.filter((message) => message.role === "assistant")).toEqual([
			expect.objectContaining({
				id: "assistant-session-a-turn-1",
				model: "claude-fable-5",
				content: "Resolvido.",
				assistantPhase: "final_answer",
				streaming: true,
				annotations: [
					expect.objectContaining({
						type: "commentary",
						content: "Vou investigar.",
						streaming: false,
					}),
				],
			}),
		]);
	});

	it("keeps phase-less live provider text inside one activity row until settle", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Alpha"),
				assistantMessageStarted("session-a", "turn-1", "segment-0", "unknown", 2),
				assistantMessageDelta("session-a", "turn-1", "segment-0", "Vou investigar.", 3),
				assistantMessageCompleted(
					"session-a",
					"turn-1",
					"segment-0",
					"unknown",
					"Vou investigar.",
					4,
				),
				assistantMessageStarted("session-a", "turn-1", "segment-1", "unknown", 5),
				assistantMessageDelta("session-a", "turn-1", "segment-1", "Executando os testes.", 6),
			],
			[],
			"session-a",
		);

		expect(messages.filter((message) => message.role === "assistant")).toEqual([
			expect.objectContaining({
				id: "assistant-session-a-turn-1",
				content: "",
				streaming: true,
				annotations: [
					expect.objectContaining({
						type: "commentary",
						content: "Vou investigar.",
					}),
					expect.objectContaining({
						type: "commentary",
						content: "Executando os testes.",
						streaming: true,
					}),
				],
			}),
		]);
	});

	it("reconciles a terminal snapshot with the sole active item when provider IDs diverge", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Alpha"),
				assistantMessageStarted("session-a", "turn-1", "stream-envelope", "unknown", 2),
				assistantMessageDelta("session-a", "turn-1", "stream-envelope", "Toda a ", 3),
				assistantMessageDelta("session-a", "turn-1", "stream-envelope", "mensagem", 4),
				assistantMessageCompleted(
					"session-a",
					"turn-1",
					"msg-authoritative",
					"unknown",
					"Toda a mensagem, incluindo o final sem perder palavras.",
					5,
				),
				// A buffered delta can arrive under the original stream envelope
				// after the authoritative snapshot closed that same lifecycle.
				assistantMessageDelta(
					"session-a",
					"turn-1",
					"stream-envelope",
					" mensagem atrasada que não deve aparecer",
					6,
				),
			],
			[],
			"session-a",
		);

		expect(messages.filter((message) => message.role === "assistant")).toEqual([
			expect.objectContaining({
				id: "assistant-session-a-turn-1",
				content: "",
				assistantPhase: "unknown",
				streaming: true,
				annotations: [
					expect.objectContaining({
						type: "commentary",
						content: "Toda a mensagem, incluindo o final sem perder palavras.",
					}),
				],
			}),
		]);
	});

	it("lets an authoritative snapshot replace incomplete streamed text under a divergent ID", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Alpha"),
				assistantMessageStarted("session-a", "turn-1", "stream-envelope", "unknown", 2),
				assistantMessageDelta(
					"session-a",
					"turn-1",
					"stream-envelope",
					"Typecheck falh",
					3,
				),
				assistantMessageCompleted(
					"session-a",
					"turn-1",
					"msg-authoritative",
					"unknown",
					"Typecheck passou sem erros.",
					4,
				),
			],
			[],
			"session-a",
		);

		expect(messages.filter((message) => message.role === "assistant")).toEqual([
			expect.objectContaining({
				content: "",
				streaming: true,
				annotations: [
					expect.objectContaining({
						type: "commentary",
						content: "Typecheck passou sem erros.",
					}),
				],
			}),
		]);
	});

	it("folds live commentary when a terminal item has its own native start", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Alpha"),
				assistantMessageStarted("session-a", "turn-1", "comment-1", "commentary", 2),
				assistantMessageDelta("session-a", "turn-1", "comment-1", "Ainda trabalhando.", 3),
				assistantMessageStarted("session-a", "turn-1", "final-1", "final_answer", 4),
				assistantMessageDelta("session-a", "turn-1", "final-1", "Resolvido.", 5),
				assistantMessageCompleted(
					"session-a",
					"turn-1",
					"final-1",
					"final_answer",
					"Resolvido.",
					6,
				),
			],
			[],
			"session-a",
		);

		expect(messages.filter((message) => message.role === "assistant")).toEqual([
			expect.objectContaining({
				id: "assistant-session-a-turn-1",
				content: "Resolvido.",
				assistantPhase: "final_answer",
				streaming: true,
				annotations: [
					expect.objectContaining({
						type: "commentary",
						content: "Ainda trabalhando.",
					}),
				],
			}),
		]);
	});

	it("folds ambiguous phase-less items into one live activity row", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Alpha"),
				assistantMessageStarted("session-a", "turn-1", "stream-1", "unknown", 2),
				assistantMessageDelta("session-a", "turn-1", "stream-1", "Primeiro", 3),
				assistantMessageStarted("session-a", "turn-1", "stream-2", "unknown", 4),
				assistantMessageDelta("session-a", "turn-1", "stream-2", "Segundo", 5),
				assistantMessageCompleted(
					"session-a",
					"turn-1",
					"terminal-3",
					"unknown",
					"Terceiro",
					6,
				),
			],
			[],
			"session-a",
		);

		expect(messages.filter((message) => message.role === "assistant")).toEqual([
			expect.objectContaining({
				id: "assistant-session-a-turn-1",
				content: "",
				streaming: true,
				annotations: [
					expect.objectContaining({ type: "commentary", content: "Primeiro" }),
					expect.objectContaining({ type: "commentary", content: "Segundo" }),
					expect.objectContaining({ type: "commentary", content: "Terceiro" }),
				],
			}),
		]);
	});

	it("folds settled commentary and keeps the authoritative final answer visible", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Alpha"),
				assistantMessageStarted("session-a", "turn-1", "comment-1", "commentary", 2),
				assistantMessageDelta("session-a", "turn-1", "comment-1", "Vou investigar.", 3),
				assistantMessageCompleted(
					"session-a",
					"turn-1",
					"comment-1",
					"commentary",
					"Vou investigar com cuidado.",
					4,
				),
				assistantMessageStarted("session-a", "turn-1", "final-1", "final_answer", 5),
				assistantMessageDelta("session-a", "turn-1", "final-1", "Parcial", 6),
				assistantMessageCompleted(
					"session-a",
					"turn-1",
					"final-1",
					"final_answer",
					"Resposta final autoritativa.",
					7,
				),
				sessionTurnCompleted("session-a", "turn-1", "2026-05-01T12:00:08Z", 8),
			],
			[],
			"session-a",
		);

		expect(messages.filter((message) => message.role === "assistant")).toEqual([
			expect.objectContaining({
				content: "Resposta final autoritativa.",
				assistantPhase: "final_answer",
				streaming: false,
				annotations: [
					expect.objectContaining({
						type: "commentary",
						content: "Vou investigar com cuidado.",
					}),
				],
			}),
		]);
	});

	it("uses the last completed assistant item when a provider has no native phase", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Alpha"),
				assistantMessageStarted("session-a", "turn-1", "segment-0", "unknown", 2),
				assistantMessageDelta("session-a", "turn-1", "segment-0", "Vou ler os arquivos.", 3),
				assistantMessageCompleted("session-a", "turn-1", "segment-0", "unknown", null, 4),
				assistantMessageStarted("session-a", "turn-1", "segment-1", "unknown", 5),
				assistantMessageDelta("session-a", "turn-1", "segment-1", "Encontrei a causa.", 6),
				assistantMessageCompleted("session-a", "turn-1", "segment-1", "unknown", null, 7),
				sessionTurnCompleted("session-a", "turn-1", "2026-05-01T12:00:08Z", 8),
			],
			[],
			"session-a",
		);

		expect(messages.filter((message) => message.role === "assistant")).toEqual([
			expect.objectContaining({
				content: "Encontrei a causa.",
				assistantPhase: "unknown",
				annotations: [
					expect.objectContaining({
						type: "commentary",
						content: "Vou ler os arquivos.",
					}),
				],
			}),
		]);
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

	it("ignores buffered assistant deltas after an authoritative item snapshot", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Alpha"),
				assistantMessageStarted("session-a", "turn-1", "message-1", "unknown", 2),
				assistantMessageCompleted(
					"session-a",
					"turn-1",
					"message-1",
					"unknown",
					"Texto autoritativo sem duplicação.",
					3,
				),
			],
			[
				{
					sessionTurnAssistantMessageStarted: {
						session_id: "session-a",
						turn_id: "turn-1",
						message_id: "message-1",
						phase: "unknown",
					},
				},
				{
					sessionTurnAssistantMessageDelta: {
						session_id: "session-a",
						turn_id: "turn-1",
						message_id: "message-1",
						content: "Texto autoritativo ",
					},
				},
				{
					sessionTurnAssistantMessageDelta: {
						session_id: "session-a",
						turn_id: "turn-1",
						message_id: "message-1",
						content: "sem duplicação.",
					},
				},
			],
			"session-a",
		);

		expect(messages.filter((message) => message.role === "assistant")).toEqual([
			expect.objectContaining({
				id: "assistant-session-a-turn-1",
				content: "",
				annotations: [
					expect.objectContaining({
						type: "commentary",
						content: "Texto autoritativo sem duplicação.",
					}),
				],
			}),
		]);
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
				turnId: "turn-1",
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

	it("shows the fork origin on the session start with a link to the source thread", () => {
		const record: SessionEventRecord = {
			eventId: "evt-start",
			sessionId: "session-b",
			sequence: 1,
			occurredAt: "2026-05-01T12:00:00Z",
			kind: {
				type: "session_started",
				workspaceId: "ws",
				projectId: "proj",
				providerId: "codex",
				model: null,
				forkedFrom: { sessionId: "session-a", turnId: "turn-3" },
			},
		};
		const [started] = projectWorkspaceMessages([record], [], "session-b");
		expect(started.role).toBe("system");
		expect(started.label).toBe("session.forked");
		expect(started.content).toContain("Forked from an earlier thread");
		expect(started.action).toEqual({
			type: "open-session",
			sessionId: "session-a",
			label: "Open source thread",
		});
	});

	it("projects an automatic objective pause as a system message", () => {
		const record: SessionEventRecord = {
			eventId: "evt-pause",
			sessionId: "session-a",
			sequence: 2,
			occurredAt: "2026-05-01T12:01:00Z",
			kind: {
				type: "objective_paused",
				reason: "consecutive_failures",
				consecutiveFailures: 3,
				turnsUsed: 5,
			},
		};
		const messages = projectWorkspaceMessages(
			[sessionTurnStarted("session-a", "turn-1", "Alpha"), record],
			[],
			"session-a",
		);
		const system = messages.find((message) => message.role === "system");
		expect(system?.label).toBe("session.objective.paused");
		expect(system?.content).toContain("consecutive failures reached the limit (3)");
		expect(system?.content).toContain("resume");
	});

	it("gives a turn aborted before any output a placeholder incomplete message", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Alpha"),
				sessionTurnAborted("session-a", "turn-1", "Interrupted by a DCC restart"),
			],
			[],
			"session-a",
		);
		expect(messages.map((message) => message.role)).toEqual(["user", "assistant"]);
		const placeholder = messages[1];
		expect(placeholder.id).toBe("assistant-session-a-turn-1");
		expect(placeholder.content).toBe("");
		expect(placeholder.streaming).toBe(false);
		expect(placeholder.status).toEqual({
			type: "incomplete",
			reason: "Interrupted by a DCC restart",
		});
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
				turnId: "turn-1",
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
				turnId: "turn-1",
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
				turnId: "turn-1",
				label: "User",
				content: "Draft prompt",
				createdAt: "2026-05-01T12:00:00Z",
				planMode: false,
			},
		]);
	});

	it("deduplicates a started turn replayed live with explicit null metadata", () => {
		const history = [sessionTurnStarted("session-a", "turn-1", "Draft prompt")];
		const live: CoreEvent[] = [
			{
				sessionTurnStarted: {
					session_id: "session-a",
					turn_id: "turn-1",
					prompt: "Draft prompt",
					plan_mode: null,
					model: null,
					evidence: null,
					retry_of_turn_id: null,
				},
			},
		];

		expect(
			projectWorkspaceMessages(history, live, "session-a", "Draft prompt"),
		).toEqual([
			{
				id: "user-session-a-turn-1",
				role: "user",
				turnId: "turn-1",
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

	it("keeps live model events for the selected session", () => {
		const liveEvents: CoreEvent[] = [
			{
				sessionTurnNativeSubagentModelRequested: {
					session_id: "session-a",
					turn_id: "turn-1",
					correlation_id: "thread-child-1",
					model: "gpt-5.6-luna",
				},
			},
			{
				sessionTurnNativeSubagentModelConfirmed: {
					session_id: "session-a",
					turn_id: "turn-1",
					correlation_id: "thread-child-1",
					model: "gpt-5.6-luna",
				},
			},
			{
				sessionTurnModelEffective: {
					session_id: "session-a",
					turn_id: "turn-1",
					model: "gpt-5.6-terra",
				},
			},
		];

		expect(
			mergeSessionThreadEvents([], liveEvents, "session-a").map(({ event }) => event),
		).toEqual(liveEvents);
		expect(mergeSessionThreadEvents([], liveEvents, "session-b")).toEqual([]);
	});

	it("projects structured native subagent activity without creating a DCC delegation", () => {
		const activity: SessionEventRecord = {
			eventId: "event-native-subagent",
			sessionId: "session-a",
			sequence: 2,
			occurredAt: "2026-05-01T12:00:01Z",
			kind: {
				type: "turn_native_subagent_activity",
				turnId: "turn-1",
				id: "agent-call-1",
				agentId: "agent-1",
				agentThreadId: "thread-child-1",
				path: "/root/terra",
				name: "Terra",
				role: "explorer",
				model: null,
				status: "running",
			},
		};

		const messages = projectWorkspaceMessages(
			[sessionTurnStarted("session-a", "turn-1", "Investigate") as SessionEventRecord, activity],
			[],
			"session-a",
		);
		expect(messages).toHaveLength(2);
		expect(messages[1]).toMatchObject({
			role: "assistant",
			annotations: [
				{
					type: "native-subagent",
					id: "agent-call-1",
					name: "Terra",
					path: "/root/terra",
					role: "explorer",
					model: undefined,
					status: "running",
				},
			],
		});
	});

	it("keeps one structured subagent card when the assistant bucket changes", () => {
		const subagent = (
			sequence: number,
			status: "running" | "completed",
		): SessionEventRecord => ({
			eventId: `event-native-subagent-${status}`,
			sessionId: "session-a",
			sequence,
			occurredAt: `2026-05-01T12:00:0${sequence}Z`,
			kind: {
				type: "turn_native_subagent_activity",
				turnId: "turn-1",
				id: status === "running" ? "spawn-call" : "child-status",
				agentId: "agent-1",
				agentThreadId: "thread-child-1",
				path: "/root/reviewer",
				name: status === "running" ? "Reviewer" : null,
				role: status === "running" ? "reviewer" : null,
				model: status === "running" ? "gpt-5.6-terra" : null,
				status,
			},
		});
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Investigate"),
				assistantMessageStarted("session-a", "turn-1", "comment-1", "commentary", 2),
				assistantMessageDelta("session-a", "turn-1", "comment-1", "Delegando review.", 3),
				subagent(4, "running"),
				assistantMessageStarted("session-a", "turn-1", "final-1", "final_answer", 5),
				assistantMessageDelta("session-a", "turn-1", "final-1", "Review concluído.", 6),
				subagent(7, "completed"),
			],
			[],
			"session-a",
		);

		const assistant = messages.find((message) => message.role === "assistant");
		expect(messages.filter((message) => message.role === "assistant")).toHaveLength(1);
		expect(assistant).toMatchObject({
			id: "assistant-session-a-turn-1",
			content: "Review concluído.",
		});
		expect(assistant?.annotations?.filter((item) => item.type === "native-subagent")).toEqual([
			expect.objectContaining({
				id: "spawn-call",
				agentId: "agent-1",
				agentThreadId: "thread-child-1",
				name: "Reviewer",
				role: "reviewer",
				model: "gpt-5.6-terra",
				status: "completed",
			}),
		]);
	});

	it("keeps the confirmed parent model on the assistant messages for that turn", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted(
					"session-a",
					"turn-1",
					"Investigate",
					"2026-05-01T12:00:00Z",
					false,
					"gpt-5.6-sol",
				),
			],
			[sessionTurnDelta("session-a", "turn-1", "Working")],
			"session-a",
		);

		expect(messages[1]).toMatchObject({
			role: "assistant",
			model: "gpt-5.6-sol",
		});
	});

	it("keeps confirmed native subagent metadata when a later status event is sparse", () => {
		const started: SessionEventRecord = {
			eventId: "event-native-subagent-started",
			sessionId: "session-a",
			sequence: 2,
			occurredAt: "2026-05-01T12:00:01Z",
			kind: {
				type: "turn_native_subagent_activity",
				turnId: "turn-1",
				id: "agent-1",
				agentId: "agent-1",
				agentThreadId: "thread-child-1",
				name: "Terra",
				path: "/root/reviewer",
				role: "worker",
				model: "gpt-5.6-terra",
				status: "running",
			},
		};
		const completed: SessionEventRecord = {
			eventId: "event-native-subagent-completed",
			sessionId: "session-a",
			sequence: 3,
			occurredAt: "2026-05-01T12:00:02Z",
			kind: {
				type: "turn_native_subagent_activity",
				turnId: "turn-1",
				id: "agent-1",
				agentId: null,
				agentThreadId: null,
				path: null,
				name: null,
				role: null,
				model: null,
				status: "completed",
			},
		};

		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Investigate") as SessionEventRecord,
				started,
				completed,
			],
			[],
			"session-a",
		);

		expect(messages[1]?.annotations).toContainEqual(
			expect.objectContaining({
				type: "native-subagent",
				id: "agent-1",
				name: "Terra",
				path: "/root/reviewer",
				role: "worker",
				model: "gpt-5.6-terra",
				status: "completed",
			}),
		);
	});

	it("reconciles native subagent status events with different ids by thread", () => {
		const started: SessionEventRecord = {
			eventId: "event-native-subagent-started-by-thread",
			sessionId: "session-a",
			sequence: 2,
			occurredAt: "2026-05-01T12:00:01Z",
			kind: {
				type: "turn_native_subagent_activity",
				turnId: "turn-1",
				id: "codex-native:state-key",
				agentId: null,
				agentThreadId: "thread-child-1",
				path: null,
				name: "Luna",
				role: "worker",
				model: "gpt-5.5",
				status: "running",
			},
		};
		const completed: SessionEventRecord = {
			eventId: "event-native-subagent-completed-by-thread",
			sessionId: "session-a",
			sequence: 3,
			occurredAt: "2026-05-01T12:00:02Z",
			kind: {
				type: "turn_native_subagent_activity",
				turnId: "turn-1",
				id: "codex-native:thread-child-1",
				agentId: null,
				agentThreadId: "thread-child-1",
				path: null,
				name: null,
				role: null,
				model: null,
				status: "completed",
			},
		};

		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Investigate") as SessionEventRecord,
				started,
				completed,
			],
			[],
			"session-a",
		);

		expect(messages[1]?.annotations).toEqual([
			expect.objectContaining({
				type: "native-subagent",
				id: "codex-native:state-key",
				name: "Luna",
				model: "gpt-5.5",
				status: "completed",
			}),
		]);
	});

	it("keeps a requested child model separate until it is confirmed", () => {
		const messages = projectWorkspaceMessages(
			[
				sessionTurnStarted("session-a", "turn-1", "Investigate"),
				{
					eventId: "activity",
					sessionId: "session-a",
					sequence: 2,
					occurredAt: "2026-05-01T12:00:01Z",
					kind: { type: "turn_native_subagent_activity", turnId: "turn-1", id: "child", agentId: null, agentThreadId: "thread-luna", path: "/root/luna", name: "/root/luna", role: null, model: null, status: "running" },
				},
				{
					eventId: "requested",
					sessionId: "session-a",
					sequence: 3,
					occurredAt: "2026-05-01T12:00:02Z",
					kind: { type: "turn_native_subagent_model_requested", turnId: "turn-1", correlationId: "thread-luna", model: "gpt-5.6-luna" },
				},
			], [], "session-a",
		);
		expect(messages[1]?.annotations?.[0]).toMatchObject({ requestedModel: "gpt-5.6-luna", model: undefined });
	});

	it("applies a child confirmation that arrives before its activity", () => {
		const messages = projectWorkspaceMessages([
			sessionTurnStarted("session-a", "turn-1", "Investigate"),
			{ eventId: "confirmed", sessionId: "session-a", sequence: 2, occurredAt: "2026-05-01T12:00:01Z", kind: { type: "turn_native_subagent_model_confirmed", turnId: "turn-1", correlationId: "thread-luna", model: "gpt-5.6-luna" } },
			{ eventId: "activity", sessionId: "session-a", sequence: 3, occurredAt: "2026-05-01T12:00:02Z", kind: { type: "turn_native_subagent_activity", turnId: "turn-1", id: "child", agentId: null, agentThreadId: "thread-luna", path: "/root/luna", name: "/root/luna", role: null, model: null, status: "running" } },
		], [], "session-a");
		expect(messages[1]?.annotations?.[0]).toMatchObject({ model: "gpt-5.6-luna" });
	});

	it("correlates Claude requested and confirmed models by Agent tool id", () => {
		const messages = projectWorkspaceMessages([
			sessionTurnStarted("session-a", "turn-1", "Review"),
			{ eventId: "requested", sessionId: "session-a", sequence: 2, occurredAt: "2026-05-01T12:00:01Z", kind: { type: "turn_native_subagent_model_requested", turnId: "turn-1", correlationId: "toolu_agent", model: "opus" } },
			{ eventId: "confirmed", sessionId: "session-a", sequence: 3, occurredAt: "2026-05-01T12:00:02Z", kind: { type: "turn_native_subagent_model_confirmed", turnId: "turn-1", correlationId: "toolu_agent", model: "claude-opus-4-1" } },
			{ eventId: "activity", sessionId: "session-a", sequence: 4, occurredAt: "2026-05-01T12:00:03Z", kind: { type: "turn_native_subagent_activity", turnId: "turn-1", id: "toolu_agent", agentId: "agent-1", agentThreadId: null, path: null, name: "Reviewer", role: "reviewer", model: null, status: "running" } },
		], [], "session-a");
		expect(messages[1]?.annotations?.[0]).toMatchObject({
			id: "toolu_agent",
			requestedModel: "opus",
			model: "claude-opus-4-1",
		});
	});

	it("uses the effective parent model after a reroute", () => {
		const messages = projectWorkspaceMessages([
			sessionTurnStarted("session-a", "turn-1", "Analyze", "2026-05-01T12:00:00Z", false, "gpt-5.6-sol"),
			sessionTurnDeltaRecord("session-a", "turn-1", "answer"),
			{ eventId: "effective", sessionId: "session-a", sequence: 2, occurredAt: "2026-05-01T12:00:01Z", kind: { type: "turn_model_effective", turnId: "turn-1", model: "gpt-5.6-luna" } },
		], [], "session-a");
		expect(messages.find((message) => message.role === "assistant")?.model).toBe("gpt-5.6-luna");
	});
});
