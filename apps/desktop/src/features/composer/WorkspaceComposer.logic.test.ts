import { describe, expect, it } from "vitest";
import {
	buildComposerContextDirectories,
	canSendPrompt,
	decideSend,
	getComposerDraftKey,
	isComposerSubmitEnabled,
	isSendDisabled,
	isSteerDisabled,
	resolvePlanModeState,
	setPlanModeState,
} from "./WorkspaceComposer.logic";

describe("WorkspaceComposer.logic", () => {
	it("builds a stable draft key", () => {
		expect(getComposerDraftKey("alpha")).toBe("dcc.workspace.composer.draft.alpha");
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

	it("gates send vs steer with helmor-style helpers", () => {
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

	it("reports context directories for workspace path and branch", () => {
		expect(
			buildComposerContextDirectories({
				workspacePath: "/projects/alpha",
				workspaceBranch: "feature/phase-2",
			}),
		).toEqual([
			{
				id: "workspace-path",
				label: "workspace",
				path: "/projects/alpha",
			},
			{
				id: "workspace-branch",
				label: "branch",
				path: "feature/phase-2",
			},
		]);
	});

	it("blocks send when no content is present", () => {
		expect(
			canSendPrompt({ disabled: false, hasContent: false, isSubmitting: false }),
		).toBe(false);
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
