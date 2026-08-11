import { describe, expect, it } from "vitest";
import { resolveExecutionDockActions } from "./ExecutionDock.actions";

describe("resolveExecutionDockActions", () => {
	it("routes conflict and fix modes to review", () => {
		expect(resolveExecutionDockActions({
			mode: "resolve-conflicts",
			loading: false,
			multiProject: false,
			hasLocalChanges: true,
			hasBranchChanges: true,
			hasAheadCommits: false,
			hasChangeRequest: false,
			hasOpenRequest: false,
		})[0]).toMatchObject({ kind: "review", primary: true });
	});

	it("keeps merge executable from the dock", () => {
		expect(resolveExecutionDockActions({
			mode: "merge",
			loading: false,
			multiProject: false,
			hasLocalChanges: false,
			hasBranchChanges: true,
			hasAheadCommits: false,
			hasChangeRequest: true,
			hasOpenRequest: true,
		})[0]).toMatchObject({ mode: "merge", kind: "execute", primary: true });
	});

	it("opens PR creation from create-pr mode", () => {
		expect(resolveExecutionDockActions({
			mode: "create-pr",
			loading: false,
			multiProject: false,
			hasLocalChanges: false,
			hasBranchChanges: true,
			hasAheadCommits: false,
			hasChangeRequest: false,
			hasOpenRequest: false,
		})[0]).toMatchObject({ kind: "create-request", primary: true });
	});

	it("executes base sync from the primary action", () => {
		expect(resolveExecutionDockActions({
			mode: "create-pr",
			loading: false,
			multiProject: false,
			hasLocalChanges: false,
			hasBranchChanges: false,
			hasAheadCommits: false,
			hasChangeRequest: false,
			hasOpenRequest: false,
		})[0]).toMatchObject({ mode: "sync-base", kind: "execute", primary: true });
	});

	it("does not hide secondary actions when the primary is unavailable", () => {
		const actions = resolveExecutionDockActions({
			mode: "push",
			loading: false,
			multiProject: false,
			hasLocalChanges: false,
			hasBranchChanges: true,
			hasAheadCommits: true,
			hasChangeRequest: false,
			hasOpenRequest: false,
		});
		expect(actions.some((action) => action.id === "create-pr")).toBe(true);
	});

	it("does not duplicate the primary action in the popover", () => {
		const actions = resolveExecutionDockActions({
			mode: "commit-and-push",
			loading: false,
			multiProject: false,
			hasLocalChanges: true,
			hasBranchChanges: true,
			hasAheadCommits: false,
			hasChangeRequest: false,
			hasOpenRequest: false,
		});
		expect(actions.filter((action) => action.mode === "commit-and-push")).toHaveLength(1);
	});

	it("keeps an existing change request in view/merge actions", () => {
		const actions = resolveExecutionDockActions({
			mode: "fix",
			loading: false,
			multiProject: false,
			hasLocalChanges: false,
			hasBranchChanges: true,
			hasAheadCommits: false,
			hasChangeRequest: true,
			hasOpenRequest: true,
		});
		expect(actions.some((action) => action.id === "open-pr" && !action.disabled)).toBe(true);
		expect(actions.some((action) => action.id === "merge" && action.disabled)).toBe(true);
		expect(actions.some((action) => action.id === "create-pr")).toBe(false);
	});

});
