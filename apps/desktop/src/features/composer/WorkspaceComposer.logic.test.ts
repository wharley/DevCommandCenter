import type { CoreEvent } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import {
	buildMissionSpecFilename,
	buildSpecDraftPrompt,
	canSendPrompt,
	decideSend,
	getCompactComposerModelLabel,
	getComposerConversationDraftKey,
	getComposerDraftKey,
	isComposerSubmitEnabled,
	isSendDisabled,
	isSteerDisabled,
	latestTurnQueueEventKey,
	resolvePlanModeState,
	setPlanModeState,
	submitComposerDraftOptimistically,
} from "./WorkspaceComposer.logic";

const event = (value: object) => value as CoreEvent;

describe("WorkspaceComposer.logic", () => {
	it("removes only the redundant Claude brand from the compact model label", () => {
		expect(getCompactComposerModelLabel("claude_code", "Claude Fable 5")).toBe(
			"Fable 5",
		);
		expect(getCompactComposerModelLabel("codex", "GPT-5.6 Terra")).toBe(
			"GPT-5.6 Terra",
		);
		expect(getCompactComposerModelLabel("cursor", "Claude Sonnet 5")).toBe(
			"Claude Sonnet 5",
		);
	});

	it("derives a stable mission spec filename from the workspace branch", () => {
		expect(buildMissionSpecFilename("feature/SDD Spike")).toBe(
			"feature-sdd-spike.spec.md",
		);
		expect(buildMissionSpecFilename(null)).toBe("mission.spec.md");
	});

	it("builds a spec draft prompt that writes the versioned DCC spec only", () => {
		const prompt = buildSpecDraftPrompt({ workspaceBranch: "feature/sdd" });

		expect(prompt).toContain(".devcommandcenter/specs/feature-sdd.spec.md");
		expect(prompt).toContain(".devcommandcenter/spec.template.md");
		expect(prompt).toContain("Do not implement code yet");
	});

	it("builds a stable draft key", () => {
		expect(getComposerDraftKey("alpha")).toBe("dcc.workspace.composer.draft.alpha");
	});

	it("isolates draft persistence by conversation with a stable new-session fallback", () => {
		expect(getComposerConversationDraftKey("alpha", "session-a")).not.toBe(
			getComposerConversationDraftKey("alpha", "session-b"),
		);
		expect(getComposerConversationDraftKey("alpha", null)).toBe(
			getComposerConversationDraftKey("alpha", null),
		);
		expect(getComposerConversationDraftKey("alpha", "")).toBe(
			getComposerConversationDraftKey("alpha", null),
		);
		expect(getComposerConversationDraftKey("alpha", "new")).not.toBe(
			getComposerConversationDraftKey("alpha", null),
		);
	});

	it("decides to block when disabled", () => {
		expect(
			decideSend({ hasContent: true, sending: false, disabled: true }),
		).toEqual({ kind: "blocked", reason: "disabled" });
	});

	it("decides to send when content exists and not sending", () => {
		expect(
			decideSend({ hasContent: true, sending: false, disabled: false }),
		).toEqual({ kind: "send" });
	});

	it("decides steer when streaming and content is present", () => {
		expect(
			decideSend({ hasContent: true, sending: true, disabled: false }),
		).toEqual({ kind: "steer" });
	});

	it("gates send vs steer with the shared helpers", () => {
		const ok = isComposerSubmitEnabled({
			disabled: false,
			hasProvider: true,
			hasContent: true,
		});
		expect(isSendDisabled(ok, false)).toBe(false);
		expect(isSendDisabled(ok, true)).toBe(true);
		expect(isSteerDisabled(ok, false)).toBe(true);
		expect(isSteerDisabled(ok, true)).toBe(false);
	});

	it("changes the queue refresh key when a queued turn is dispatched", () => {
		const queued = event({
			sessionTurnQueued: {
				session_id: "session-a",
				queued_turn: { id: "queued-a" },
			},
		});
		const nextTurnStarted = event({
			sessionTurnStarted: {
				session_id: "session-a",
				turn_id: "turn-b",
				prompt: "follow up",
			},
		});
		const dispatched = event({
			sessionQueuedTurnDispatched: {
				session_id: "session-a",
				queued_turn_id: "queued-a",
				turn_id: "turn-b",
			},
		});

		expect(latestTurnQueueEventKey([queued])).toBe(
			"session-a:queued:queued-a",
		);
		expect(latestTurnQueueEventKey([queued, nextTurnStarted])).toBe(
			"session-a:queued:queued-a",
		);
		expect(latestTurnQueueEventKey([queued, nextTurnStarted, dispatched])).toBe(
			"session-a:dispatched:queued-a:turn-b",
		);
	});

	it("blocks send when no content is present", () => {
		expect(
			canSendPrompt({ disabled: false, hasContent: false, isSubmitting: false }),
		).toBe(false);
	});

	it("clears the submitted draft before waiting for acceptance", async () => {
		const events: string[] = [];
		let acceptSubmission: (accepted: boolean) => void = () => {
			throw new Error("submission resolver was not initialized");
		};
		const submission = submitComposerDraftOptimistically({
			clearSubmittedDraft: () => events.push("clear"),
			submit: () =>
				new Promise<boolean>((resolve) => {
					events.push("submit");
					acceptSubmission = resolve;
				}),
			restoreSubmittedDraft: () => events.push("restore"),
		});

		expect(events).toEqual(["clear", "submit"]);
		acceptSubmission(true);
		await expect(submission).resolves.toBe(true);
		expect(events).toEqual(["clear", "submit"]);
	});

	it("restores an optimistically cleared draft when submission is rejected", async () => {
		const events: string[] = [];
		const accepted = await submitComposerDraftOptimistically({
			clearSubmittedDraft: () => events.push("clear"),
			submit: async () => false,
			restoreSubmittedDraft: () => events.push("restore"),
		});

		expect(accepted).toBe(false);
		expect(events).toEqual(["clear", "restore"]);
	});

	it("restores an optimistically cleared draft when submission throws", async () => {
		const events: string[] = [];
		const accepted = await submitComposerDraftOptimistically({
			clearSubmittedDraft: () => events.push("clear"),
			submit: async () => {
				throw new Error("network failure");
			},
			restoreSubmittedDraft: () => events.push("restore"),
		});

		expect(accepted).toBe(false);
		expect(events).toEqual(["clear", "restore"]);
	});

	it("scopes plan mode by workspace before a session exists", () => {
		const state = setPlanModeState({}, {
			workspaceId: "workspace-a",
			sessionId: null,
			enabled: true,
		});

		expect(
			resolvePlanModeState(state, {
				workspaceId: "workspace-a",
				sessionId: null,
			}),
		).toBe(true);
		expect(
			resolvePlanModeState(state, {
				workspaceId: "workspace-b",
				sessionId: null,
			}),
		).toBe(false);
	});

	it("prefers session-scoped plan mode over the workspace fallback", () => {
		const state = {
			"workspace:workspace-a": true,
			"session:session-a": false,
		};

		expect(
			resolvePlanModeState(state, {
				workspaceId: "workspace-a",
				sessionId: "session-a",
			}),
		).toBe(false);
		expect(
			resolvePlanModeState(state, {
				workspaceId: "workspace-a",
				sessionId: "session-b",
			}),
		).toBe(true);
	});
});
