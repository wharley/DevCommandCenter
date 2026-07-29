import { describe, expect, it, vi } from "vitest";
import { removeProjectFromDcc } from "./project-removal";

describe("removeProjectFromDcc", () => {
	it("deletes the repository once and only refreshes workspace state afterwards", async () => {
		const calls: string[] = [];
		const deleteRepository = vi.fn(async () => {
			calls.push("delete-repository");
		});
		const removeLocalState = vi.fn(() => {
			calls.push("remove-local-state");
		});
		const refreshRepositories = vi.fn(async () => {
			calls.push("refresh-repositories");
		});
		const refreshWorkspaces = vi.fn(async () => {
			calls.push("refresh-workspaces");
		});

		await removeProjectFromDcc(
			{
				repositoryId: "/repo/project",
				workspaceIds: ["workspace-1", "workspace-1", "workspace-2"],
			},
			{
				deleteRepository,
				removeLocalState,
				refreshRepositories,
				refreshWorkspaces,
			},
		);

		expect(deleteRepository).toHaveBeenCalledOnce();
		expect(deleteRepository).toHaveBeenCalledWith("/repo/project");
		expect(removeLocalState).toHaveBeenCalledWith(["workspace-1", "workspace-2"]);
		expect(refreshRepositories).toHaveBeenCalledOnce();
		expect(refreshWorkspaces).toHaveBeenCalledOnce();
		expect(calls[0]).toBe("delete-repository");
		expect(calls[1]).toBe("remove-local-state");
	});

	it("does not update local state when backend removal fails", async () => {
		const removeLocalState = vi.fn();
		const refreshRepositories = vi.fn();
		const refreshWorkspaces = vi.fn();

		await expect(
			removeProjectFromDcc(
				{ repositoryId: "/repo/project", workspaceIds: ["workspace-1"] },
				{
					deleteRepository: async () => {
						throw new Error("cleanup failed");
					},
					removeLocalState,
					refreshRepositories,
					refreshWorkspaces,
				},
			),
		).rejects.toThrow("cleanup failed");

		expect(removeLocalState).not.toHaveBeenCalled();
		expect(refreshRepositories).not.toHaveBeenCalled();
		expect(refreshWorkspaces).not.toHaveBeenCalled();
	});
});
