import { describe, expect, it } from "vitest";
import {
	deriveWorkspaceCommitMessage,
	sanitizeWorkspaceCommitBody,
	sanitizeWorkspaceCommitSubject,
} from "./commit-message";

describe("deriveWorkspaceCommitMessage", () => {
	it("does not need a task or chat title to describe a source change", () => {
		expect(deriveWorkspaceCommitMessage([
			{
				path: "apps/desktop/src/features/commit/use-workspace-delivery.ts",
				status: "M",
			},
		])).toBe("chore(commit): update use workspace delivery");
	});

	it("describes a single added documentation file", () => {
		expect(deriveWorkspaceCommitMessage([
			{ path: "docs/README.md", status: "A" },
		])).toBe("docs: add README");
	});

	it("uses only staged entries when the caller supplies a staged subset", () => {
		expect(deriveWorkspaceCommitMessage([
			{ path: "crates/dcc-tauri/src/commands/workspace_commands.rs", status: "M" },
		])).toBe("chore(commands): update workspace commands");
	});

	it("summarizes several changes in the same feature", () => {
		expect(deriveWorkspaceCommitMessage([
			{ path: "apps/desktop/src/features/commit/WorkspaceCommitButton.tsx", status: "M" },
			{ path: "apps/desktop/src/features/commit/commit-message.ts", status: "A" },
		])).toBe("chore(commit): update commit files");
	});

	it("uses a conservative project fallback for unrelated paths", () => {
		expect(deriveWorkspaceCommitMessage([
			{ path: "apps/desktop/src/App.tsx", status: "M" },
			{ path: "crates/dcc-tauri/src/state.rs", status: "M" },
		])).toBe("chore: update project files");
	});

	it("recognizes tests, CI, and dependency-only changes", () => {
		expect(deriveWorkspaceCommitMessage([
			{ path: "apps/desktop/src/auth/login.test.ts", status: "M" },
		])).toBe("test(auth): update login test");
		expect(deriveWorkspaceCommitMessage([
			{ path: ".github/workflows/release.yml", status: "M" },
		])).toBe("ci: update release");
		expect(deriveWorkspaceCommitMessage([
			{ path: "package.json", status: "M" },
		])).toBe("build: update package");
	});

	it("returns a safe fallback for an empty list", () => {
		expect(deriveWorkspaceCommitMessage([])).toBe("chore: update project files");
	});

	it("keeps provider-shaped output out of the commit subject", () => {
		expect(sanitizeWorkspaceCommitSubject('```json\n{"subject":"fix: leaked prompt"}\n```'))
			.toBe("fix: leaked prompt");
		expect(sanitizeWorkspaceCommitSubject("\n\n")).toBe("chore: update project files");
		expect(sanitizeWorkspaceCommitBody("Fixes the staged behavior.\n\nReviewed locally."))
			.toBe("Fixes the staged behavior.\n\nReviewed locally.");
		expect(sanitizeWorkspaceCommitBody("```text\nbody: leaked token\ncontext\n```"))
			.toBe("context");
	});
});
