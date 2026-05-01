import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "../../components/ui/dialog";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import type { CreateWorkspaceForRepoInput } from "@dcc/contracts";
import type { WorkspaceSummary } from "./types";

type CreateWorkspaceDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onCreateWorkspace: (input: CreateWorkspaceForRepoInput) => Promise<WorkspaceSummary>;
	isSubmitting: boolean;
};

const INITIAL_FORM = {
	projectId: "dcc-demo",
	workspaceRoot: "",
	baseBranch: "main",
	name: "",
};

export function CreateWorkspaceDialog({
	open,
	onOpenChange,
	onCreateWorkspace,
	isSubmitting,
}: CreateWorkspaceDialogProps) {
	const [form, setForm] = useState(INITIAL_FORM);

	useEffect(() => {
		if (open) {
			setForm(INITIAL_FORM);
		}
	}, [open]);

	async function handleSubmit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		try {
			await onCreateWorkspace({
				projectId: form.projectId.trim(),
				workspaceRoot: form.workspaceRoot.trim(),
				baseBranch: form.baseBranch.trim(),
				name: form.name.trim() || null,
			});
			toast.success("Workspace created");
			onOpenChange(false);
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			toast.error("Failed to create workspace", {
				description: message,
			});
		}
	}

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>Create workspace</DialogTitle>
					<DialogDescription>
						Prepare a new worktree from a project root and base branch.
					</DialogDescription>
				</DialogHeader>

				<form className="grid gap-4" onSubmit={handleSubmit}>
					<div className="grid gap-2">
						<Label htmlFor="workspace-project-id">Project ID</Label>
						<Input
							id="workspace-project-id"
							value={form.projectId}
							onChange={(event) =>
								setForm((current) => ({ ...current, projectId: event.target.value }))
							}
							placeholder="dcc-demo"
						/>
					</div>

					<div className="grid gap-2">
						<Label htmlFor="workspace-root">Workspace root</Label>
						<Input
							id="workspace-root"
							value={form.workspaceRoot}
							onChange={(event) =>
								setForm((current) => ({
									...current,
									workspaceRoot: event.target.value,
								}))
							}
							placeholder="/path/to/repo"
						/>
					</div>

					<div className="grid gap-2">
						<Label htmlFor="workspace-branch">Base branch</Label>
						<Input
							id="workspace-branch"
							value={form.baseBranch}
							onChange={(event) =>
								setForm((current) => ({ ...current, baseBranch: event.target.value }))
							}
							placeholder="main"
						/>
					</div>

					<div className="grid gap-2">
						<Label htmlFor="workspace-name">Workspace name</Label>
						<Input
							id="workspace-name"
							value={form.name}
							onChange={(event) =>
								setForm((current) => ({ ...current, name: event.target.value }))
							}
							placeholder="Feature branch"
						/>
					</div>

					<DialogFooter>
						<Button
							type="button"
							variant="secondary"
							onClick={() => onOpenChange(false)}
						>
							Cancel
						</Button>
						<Button type="submit" disabled={isSubmitting}>
							{isSubmitting ? "Creating..." : "Create workspace"}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
