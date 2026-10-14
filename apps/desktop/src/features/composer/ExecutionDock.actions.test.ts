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

	it("stops at a local commit when the repository has no remote", () => {
		const base = {
			mode: "commit-and-push" as const,
			loading: false,
			multiProject: false,
			hasLocalChanges: true,
			hasBranchChanges: true,
			hasAheadCommits: true,
			hasChangeRequest: false,
			hasOpenRequest: false,
			hasRemote: false,
		};
		const actions = resolveExecutionDockActions(base);
		expect(actions[0]).toMatchObject({ mode: "commit", kind: "execute", disabled: false, primary: true });
		const byId = Object.fromEntries(actions.map((action) => [action.id, action.disabled]));
		expect(byId["commit-and-push"]).toBe(true);
		expect(byId.push).toBe(true);
		expect(byId["create-pr"]).toBe(true);
		expect(byId["create-draft-pr"]).toBe(true);
		expect(actions.filter((action) => action.mode === "commit")).toHaveLength(1);

		expect(resolveExecutionDockActions({ ...base, mode: "create-pr", hasLocalChanges: false })[0])
			.toMatchObject({ mode: "commit", disabled: true });
		expect(resolveExecutionDockActions({ ...base, mode: "resolve-conflicts" })[0])
			.toMatchObject({ kind: "review" });
		expect(resolveExecutionDockActions({ ...base, hasRemote: undefined })[0])
			.toMatchObject({ mode: "commit-and-push" });
	});
});
