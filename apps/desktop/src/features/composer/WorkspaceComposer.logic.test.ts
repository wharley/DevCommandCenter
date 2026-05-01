import { describe, expect, it } from "vitest";
import {
	buildComposerContextDirectories,
	canSendPrompt,
	decideSend,
	getComposerDraftKey,
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
});
