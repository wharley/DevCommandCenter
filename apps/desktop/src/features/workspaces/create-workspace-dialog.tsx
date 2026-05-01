import { LoaderCircle } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import {
	Dialog,
	DialogContent,
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

	const canSubmit = useMemo(() => {
		return (
			form.projectId.trim().length > 0 &&
			form.workspaceRoot.trim().length > 0 &&
			form.baseBranch.trim().length > 0 &&
			!isSubmitting
		);
	}, [form.baseBranch, form.projectId, form.workspaceRoot, isSubmitting]);

	async function handleSubmit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		if (!canSubmit) {
			return;
		}

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
		<Dialog
			open={open}
			onOpenChange={(nextOpen) => {
				if (isSubmitting && !nextOpen) {
					return;
				}
				onOpenChange(nextOpen);
			}}
		>
			<DialogContent className="gap-3 p-4 sm:max-w-sm">
				<DialogHeader className="space-y-1">
					<DialogTitle className="text-[13px] font-medium tracking-[-0.01em]">
						Create workspace
					</DialogTitle>
					<p className="text-[12px] leading-snug text-muted-foreground">
						New worktree from a local repo path and base branch.
					</p>
				</DialogHeader>

				<form
					onSubmit={handleSubmit}
					className="flex flex-col gap-3"
				>
					<div className="flex flex-col gap-1">
						<Label
							htmlFor="workspace-project-id"
							className="text-[12px] font-medium tracking-[-0.01em]"
						>
							Project ID
						</Label>
						<Input
							id="workspace-project-id"
							value={form.projectId}
							onChange={(event) =>
								setForm((current) => ({ ...current, projectId: event.target.value }))
							}
							placeholder="dcc-demo"
							autoComplete="off"
							spellCheck={false}
							disabled={isSubmitting}
							className="h-7 text-[13px] md:text-[13px]"
						/>
					</div>

					<div className="flex flex-col gap-1">
						<Label
							htmlFor="workspace-root"
							className="text-[12px] font-medium tracking-[-0.01em]"
						>
							Repository path
						</Label>
						<Input
							id="workspace-root"
							value={form.workspaceRoot}
							onChange={(event) =>
								setForm((current) => ({
									...current,
									workspaceRoot: event.target.value,
								}))
							}
							placeholder="/path/to/git/repo"
							autoComplete="off"
							spellCheck={false}
							disabled={isSubmitting}
							className="h-7 font-mono text-[13px] md:text-[13px]"
						/>
					</div>

					<div className="flex flex-col gap-1">
						<Label
							htmlFor="workspace-branch"
							className="text-[12px] font-medium tracking-[-0.01em]"
						>
							Base branch
						</Label>
						<Input
							id="workspace-branch"
							value={form.baseBranch}
							onChange={(event) =>
								setForm((current) => ({ ...current, baseBranch: event.target.value }))
							}
							placeholder="main"
							autoComplete="off"
							spellCheck={false}
							disabled={isSubmitting}
							className="h-7 text-[13px] md:text-[13px]"
						/>
					</div>

					<div className="flex flex-col gap-1">
						<Label
							htmlFor="workspace-name"
							className="text-[12px] font-medium tracking-[-0.01em]"
						>
							Display name{" "}
							<span className="font-normal text-muted-foreground">(optional)</span>
						</Label>
						<Input
							id="workspace-name"
							value={form.name}
							onChange={(event) =>
								setForm((current) => ({ ...current, name: event.target.value }))
							}
							placeholder="Defaults from branch if empty"
							autoComplete="off"
							spellCheck={false}
							disabled={isSubmitting}
							className="h-7 text-[13px] md:text-[13px]"
						/>
					</div>

					<div className="flex flex-wrap items-center justify-end gap-2 pt-0.5">
						<Button
							type="button"
							variant="outline"
							size="sm"
							disabled={isSubmitting}
							onClick={() => onOpenChange(false)}
						>
							Cancel
						</Button>
						<Button
							type="submit"
							size="sm"
							disabled={!canSubmit}
							className="inline-flex gap-1.5"
						>
							{isSubmitting ? (
								<>
									<LoaderCircle
										aria-hidden
										className="size-4 shrink-0 animate-spin"
										strokeWidth={2.1}
									/>
									Creating…
								</>
							) : (
								"Create workspace"
							)}
						</Button>
					</div>
				</form>
			</DialogContent>
		</Dialog>
	);
}
