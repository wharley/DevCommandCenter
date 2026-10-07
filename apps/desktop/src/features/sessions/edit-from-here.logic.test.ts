import { describe, expect, it } from "vitest";
import type { CoreEvent, SessionEventRecord } from "@dcc/contracts";
import {
	buildExecuteConversationRewindInput,
	editFromHereActions,
	editFromHereContinuation,
	editFromHereFiles,
	interpretConversationRewind,
	type EditFromHerePlan,
} from "./edit-from-here.logic";
import { projectWorkspaceMessages } from "./session-thread-history.logic";

const SESSION = "session-a";
let sequence = 0;

function record(kind: SessionEventRecord["kind"]): SessionEventRecord {
	sequence += 1;
	return {
		eventId: `evt-${sequence}`,
		sessionId: SESSION,
		sequence,
		occurredAt: `2026-10-07T10:00:${String(sequence).padStart(2, "0")}Z`,
		kind,
	};
}

function turn(turnId: string, prompt: string, answer: string): SessionEventRecord[] {
	return [
		record({ type: "turn_started", turnId, prompt, planMode: null, model: null }),
		record({
			type: "turn_assistant_message_completed",
			turnId,
			messageId: `msg-${turnId}`,
			phase: "final_answer",
			content: answer,
		}),
		record({ type: "turn_completed", turnId }),
	];
}

function rewound(anchorTurnId: string, removedTurnIds: string[], restoredTurnIds: string[] = []) {
	return record({
		type: "conversation_rewound",
		anchorTurnId,
		removedTurnIds,
		providerContext: "native",
		restoredTurnIds,
	});
}

function visible(history: SessionEventRecord[], live: CoreEvent[] = []) {
	return projectWorkspaceMessages(history, live, SESSION).map((message) =>
		message.role === "system" ? `system:${message.label}` : `${message.role}:${message.content}`,
	);
}

describe("conversation rewind in the timeline", () => {
	it("drops the last turn and keeps everything before it", () => {
		sequence = 0;
		const history = [
			...turn("t1", "first", "one"),
			...turn("t2", "second", "two"),
			rewound("t2", ["t2"]),
		];
		expect(visible(history)).toEqual([
			"user:first",
			"assistant:one",
			"system:session.conversation.rewound",
		]);
	});

	it("drops a middle turn and every later one, including their late live events, and shows new turns", () => {
		sequence = 0;
		const history = [
			...turn("t1", "first", "one"),
			...turn("t2", "second @/tmp/shot.png", "two"),
			...turn("t3", "third", "three"),
			rewound("t2", ["t2", "t3"], ["t3", "t2"]),
			...turn("t4", "second, edited", "two again"),
		];
		const lateLiveDelta: CoreEvent = {
			sessionTurnDelta: { session_id: SESSION, turn_id: "t3", content: "straggler" },
		};
		expect(visible(history, [lateLiveDelta])).toEqual([
			"user:first",
			"assistant:one",
			"system:session.conversation.rewound",
			"user:second, edited",
			"assistant:two again",
		]);
		const note = projectWorkspaceMessages(history, [], SESSION).find(
			(message) => message.role === "system",
		);
		expect(note?.content).toContain("2 turns left the conversation");
		expect(note?.content).toContain("files restored for 2 turns");
	});

	it("applies a rewind that arrives live, before history is reloaded", () => {
		sequence = 0;
		const history = [...turn("t1", "first", "one"), ...turn("t2", "second", "two")];
		const live: CoreEvent = {
			sessionConversationRewound: {
				session_id: SESSION,
				anchor_turn_id: "t2",
				removed_turn_ids: ["t2"],
				provider_context: "fresh",
				restored_turn_ids: [],
			},
		};
		expect(visible(history, [live])).toEqual([
			"user:first",
			"assistant:one",
			"system:session.conversation.rewound",
		]);
	});
});

function plan(overrides: Partial<EditFromHerePlan> = {}): EditFromHerePlan {
	return {
		status: "ready",
		anchorTurnId: "t2",
		removedTurnIds: ["t2", "t3"],
		keptTurnCount: 1,
		anchorPrompt: "second @/tmp/shot.png",
		provider: { mode: "native" },
		files: {
			status: "restorable",
			turnCount: 2,
			fileCount: 3,
			files: [],
		},
		...overrides,
	};
}

describe("edit from here dialog", () => {
	it("offers restoring files only as its own explicit choice when every file can be restored", () => {
		expect(editFromHereActions(plan())).toEqual(["restore", "keep"]);
		expect(editFromHereFiles(plan())).toEqual({ kind: "restorable", fileCount: 3, turnCount: 2 });
		expect(buildExecuteConversationRewindInput(SESSION, plan(), true)).toEqual({
			sessionId: SESSION,
			anchorTurnId: "t2",
			removedTurnIds: ["t2", "t3"],
			restoreFiles: true,
			continueInNewThread: false,
		});
		expect(buildExecuteConversationRewindInput(SESSION, plan(), false).restoreFiles).toBe(false);
	});

	it("keeps files as they are and explains why when they cannot be restored safely", () => {
		const conflict = plan({
			files: {
				status: "not_restorable",
				turnId: "t2",
				stage: "prepare",
				reasonCode: "target_result_mismatch",
			},
		});
		expect(editFromHereActions(conflict)).toEqual(["keep"]);
		expect(editFromHereFiles(conflict)).toEqual({
			kind: "blocked",
			reasonKey: "turnReview.guardedUndo.failureReasons.changed",
		});
		// Even a stale request to restore cannot restore here.
		expect(buildExecuteConversationRewindInput(SESSION, conflict, true).restoreFiles).toBe(false);

		const untracked = plan({
			files: {
				status: "not_restorable",
				turnId: "t3",
				stage: "capture",
				reasonCode: "untracked_path",
			},
		});
		expect(editFromHereFiles(untracked)).toEqual({
			kind: "blocked",
			reasonKey: "turnReview.guardedUndo.reasons.untracked_path",
		});
		expect(editFromHereFiles(plan({ files: { status: "nothing_to_restore" } }))).toEqual({
			kind: "none",
		});
	});

	it("continues in a new thread when the provider cannot cut its own conversation", () => {
		const noRewind = plan({
			provider: { mode: "new_thread", reason: "provider_without_rewind" },
		});
		expect(editFromHereContinuation(noRewind)).toEqual({
			kind: "new_thread",
			reasonKey: "conversation.editFromHere.newThreadReasons.provider_without_rewind",
		});
		expect(buildExecuteConversationRewindInput(SESSION, noRewind, false).continueInNewThread).toBe(
			true,
		);
		expect(editFromHereContinuation(plan())).toEqual({ kind: "native" });
		expect(editFromHereContinuation(plan({ provider: { mode: "fresh" }, keptTurnCount: 0 }))).toEqual(
			{ kind: "fresh" },
		);
	});

	it("reads every outcome: rewound, fork, stopped restoring, refused", () => {
		expect(
			interpretConversationRewind({
				status: "rewound",
				provider: { mode: "native" },
				restoredTurnIds: ["t3", "t2"],
				anchorPrompt: "second @/tmp/shot.png",
			}),
		).toEqual({
			kind: "rewound",
			prompt: "second @/tmp/shot.png",
			restoredTurnCount: 2,
			continuation: "native",
		});
		expect(
			interpretConversationRewind({
				status: "continue_in_new_thread",
				restoredTurnIds: [],
				anchorPrompt: "second",
			}),
		).toEqual({ kind: "fork", prompt: "second", restoredTurnCount: 0 });
		expect(
			interpretConversationRewind({
				status: "files_stopped",
				stopped: {
					restoredTurnIds: ["t3"],
					turnId: "t2",
					outcome: "blocked",
					reasonCode: "target_result_mismatch",
				},
			}),
		).toEqual({
			kind: "files_stopped",
			restoredTurnCount: 1,
			resultKey: "turnReview.guardedUndo.results.blocked",
			reasonKey: "turnReview.guardedUndo.failureReasons.changed",
		});
		expect(interpretConversationRewind({ status: "refused", reason: "plan_changed" })).toEqual({
			kind: "refused",
			reason: "plan_changed",
		});
	});
});
