import { describe, expect, it, vi } from "vitest";
import { removeProjectFromDcc } from "./project-removal";

describe("removeProjectFromDcc", () => {
	it("deletes the repository once and only refreshes workspace state afterwards", async () => {
		const calls: string[] = [];
		const deleteRepository = vi.fn(async () => {
			calls.push("delete-repository");
		});
		const removeWorkspaceCaches = vi.fn(() => {
			calls.push("remove-workspace-caches");
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
				removeWorkspaceCaches,
				refreshRepositories,
				refreshWorkspaces,
			},
		);

		expect(deleteRepository).toHaveBeenCalledOnce();
		expect(deleteRepository).toHaveBeenCalledWith("/repo/project");
		expect(removeWorkspaceCaches).toHaveBeenCalledWith(["workspace-1", "workspace-2"]);
		expect(refreshRepositories).toHaveBeenCalledOnce();
		expect(refreshWorkspaces).toHaveBeenCalledOnce();
		expect(calls[0]).toBe("delete-repository");
		expect(calls[1]).toBe("remove-workspace-caches");
	});

	it("does not update local state when backend removal fails", async () => {
		const removeWorkspaceCaches = vi.fn();
		const refreshRepositories = vi.fn();
		const refreshWorkspaces = vi.fn();

		await expect(
			removeProjectFromDcc(
				{ repositoryId: "/repo/project", workspaceIds: ["workspace-1"] },
				{
					deleteRepository: async () => {
						throw new Error("cleanup failed");
					},
					removeWorkspaceCaches,
					refreshRepositories,
					refreshWorkspaces,
				},
			),
		).rejects.toThrow("cleanup failed");

		expect(removeWorkspaceCaches).not.toHaveBeenCalled();
		expect(refreshRepositories).not.toHaveBeenCalled();
		expect(refreshWorkspaces).not.toHaveBeenCalled();
	});
});
