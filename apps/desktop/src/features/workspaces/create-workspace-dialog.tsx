import { LoaderCircle, FolderOpen } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "../../components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
} from "../../components/ui/dialog";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import type {
	CreateWorkspaceForRepoInput,
	CreateWorkspaceFromUrlInput,
} from "@dcc/contracts";
import type { WorkspaceSummary } from "./types";
import { inferProjectIdFromWorkspaceRoot } from "./create-workspace-dialog.logic";
import { listLocalBranches } from "../../lib/workspace-api";

type WorkspaceCreationMode = "open" | "clone";

type CreateWorkspaceDialogProps = {
	open: boolean;
	mode: WorkspaceCreationMode;
	onOpenChange: (open: boolean) => void;
	onCreateWorkspace: (input: CreateWorkspaceForRepoInput) => Promise<WorkspaceSummary>;
	onCloneWorkspace: (input: CreateWorkspaceFromUrlInput) => Promise<WorkspaceSummary>;
	isSubmitting: boolean;
};

const INITIAL_FORM = {
	projectId: "",
	workspaceRoot: "",
	baseBranch: "main",
	name: "",
	repositoryUrl: "",
};

function getInitialForm(mode: WorkspaceCreationMode) {
	return mode === "clone"
		? {
				...INITIAL_FORM,
				baseBranch: "",
			}
		: INITIAL_FORM;
}

export function CreateWorkspaceDialog({
	open,
	mode,
	onOpenChange,
	onCreateWorkspace,
	onCloneWorkspace,
	isSubmitting,
}: CreateWorkspaceDialogProps) {
	const [form, setForm] = useState(INITIAL_FORM);
	const [availableBranches, setAvailableBranches] = useState<string[]>([]);
	const [isLoadingBranches, setIsLoadingBranches] = useState(false);

	useEffect(() => {
		if (open) {
			setForm(getInitialForm(mode));
			setAvailableBranches([]);
			setIsLoadingBranches(false);
		}
	}, [mode, open]);

	async function loadBranchesForWorkspaceRoot(workspaceRoot: string) {
		if (mode !== "open" || workspaceRoot.trim().length === 0) {
			setAvailableBranches([]);
			return;
		}

		setIsLoadingBranches(true);
		try {
			const result = await listLocalBranches({
				workspaceRoot: workspaceRoot.trim(),
			});
			setAvailableBranches(result.branches);
			setForm((current) => {
				if (current.baseBranch.trim().length > 0 && result.branches.includes(current.baseBranch)) {
					return current;
				}

				return {
					...current,
					baseBranch: result.branches[0] ?? "",
				};
			});
		} catch (error) {
			setAvailableBranches([]);
			setForm((current) => ({
				...current,
				baseBranch: "",
			}));
			const message = error instanceof Error ? error.message : String(error);
			toast.error("Could not load branches", {
				description: message,
			});
		} finally {
			setIsLoadingBranches(false);
		}
	}

	async function handlePickWorkspaceRoot() {
		try {
			const selected = await openDialog({
				directory: true,
				multiple: false,
				title:
					mode === "clone"
						? "Choose a destination folder"
						: "Choose a repository folder",
			});

			const pickedPath = Array.isArray(selected)
				? selected[0] ?? ""
				: selected ?? "";

			if (!pickedPath) {
				return;
			}

			setForm((current) => ({
				...current,
				workspaceRoot: pickedPath,
				projectId:
					current.projectId.trim().length > 0
						? current.projectId
						: inferProjectIdFromWorkspaceRoot(pickedPath),
			}));
			if (mode === "open") {
				void loadBranchesForWorkspaceRoot(pickedPath);
			}
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			toast.error("Could not open folder picker", {
				description: message,
			});
		}
	}

	const canSubmit = useMemo(() => {
		const hasCommonFields =
			form.projectId.trim().length > 0 &&
			form.workspaceRoot.trim().length > 0 &&
			!isSubmitting;

		if (mode === "clone") {
			return hasCommonFields && form.repositoryUrl.trim().length > 0;
		}

		return hasCommonFields && form.baseBranch.trim().length > 0;
	}, [form.baseBranch, form.projectId, form.repositoryUrl, form.workspaceRoot, isSubmitting, mode]);

	async function handleSubmit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		if (!canSubmit) {
			return;
		}

		try {
			if (mode === "clone") {
				await onCloneWorkspace({
					projectId: form.projectId.trim(),
					repositoryUrl: form.repositoryUrl.trim(),
					workspaceRoot: form.workspaceRoot.trim(),
					baseBranch: form.baseBranch.trim(),
					name: form.name.trim() || null,
				});
				toast.success("Repository cloned");
			} else {
				await onCreateWorkspace({
					projectId: form.projectId.trim(),
					workspaceRoot: form.workspaceRoot.trim(),
					baseBranch: form.baseBranch.trim(),
					name: form.name.trim() || null,
				});
				toast.success("Workspace created");
			}
			onOpenChange(false);
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			toast.error(mode === "clone" ? "Failed to clone repository" : "Failed to create workspace", {
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
						{mode === "clone" ? "Clone repository" : "Create workspace"}
					</DialogTitle>
					<p className="text-[12px] leading-snug text-muted-foreground">
						{mode === "clone"
							? "Clone a remote repository into a local folder, then create the worktree from it. Leave base branch empty to auto-detect the remote default branch."
							: "New worktree from a local repo path and base branch."}
					</p>
				</DialogHeader>

				<form
					onSubmit={handleSubmit}
					className="flex flex-col gap-3"
				>
					{mode === "clone" ? (
						<div className="flex flex-col gap-1">
							<Label
								htmlFor="workspace-repository-url"
								className="text-[12px] font-medium tracking-[-0.01em]"
							>
								Repository URL
							</Label>
							<Input
								id="workspace-repository-url"
								value={form.repositoryUrl}
								onChange={(event) =>
									setForm((current) => ({
										...current,
										repositoryUrl: event.target.value,
									}))
								}
								placeholder="https://github.com/org/repo.git"
								autoComplete="off"
								spellCheck={false}
								disabled={isSubmitting}
								className="h-7 font-mono text-[13px] md:text-[13px]"
							/>
						</div>
					) : null}

					<div className="flex flex-col gap-1">
						<div className="flex items-center justify-between gap-2">
							<Label
								htmlFor="workspace-project-id"
								className="text-[12px] font-medium tracking-[-0.01em]"
							>
								Project ID
							</Label>
							<span className="text-[11px] text-muted-foreground">
								Auto-filled from folder name
							</span>
						</div>
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
						<div className="flex items-center justify-between gap-2">
							<Label
								htmlFor="workspace-root"
								className="text-[12px] font-medium tracking-[-0.01em]"
							>
								{mode === "clone" ? "Destination folder" : "Repository path"}
							</Label>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="h-6 gap-1.5 px-2 text-[11px] text-muted-foreground hover:text-foreground"
								disabled={isSubmitting}
								onClick={handlePickWorkspaceRoot}
							>
								<FolderOpen className="size-3.5" aria-hidden />
								Choose folder
							</Button>
						</div>
						<Input
							id="workspace-root"
							value={form.workspaceRoot}
							onChange={(event) =>
								setForm((current) => ({
									...current,
									workspaceRoot: event.target.value,
								}))
							}
							onBlur={() => {
								if (mode === "open" && form.workspaceRoot.trim().length > 0) {
									void loadBranchesForWorkspaceRoot(form.workspaceRoot);
								}
							}}
							placeholder={
								mode === "clone" ? "/path/to/clone/destination" : "/path/to/git/repo"
							}
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
							{mode === "clone" ? "Base branch (optional)" : "Base branch"}
						</Label>
						{mode === "open" ? (
							<select
								id="workspace-branch"
								value={form.baseBranch}
								onChange={(event) =>
									setForm((current) => ({ ...current, baseBranch: event.target.value }))
								}
								disabled={isSubmitting || isLoadingBranches || availableBranches.length === 0}
								className="h-7 rounded-md border border-input bg-background px-2 text-[13px] text-foreground shadow-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
							>
								{isLoadingBranches ? (
									<option value="">Loading branches…</option>
								) : availableBranches.length === 0 ? (
									<option value="">Choose a folder to load branches</option>
								) : null}
								{availableBranches.map((branch) => (
									<option key={branch} value={branch}>
										{branch}
									</option>
								))}
							</select>
						) : (
							<Input
								id="workspace-branch"
								value={form.baseBranch}
								onChange={(event) =>
									setForm((current) => ({ ...current, baseBranch: event.target.value }))
								}
								placeholder="Auto-detect default branch"
								autoComplete="off"
								spellCheck={false}
								disabled={isSubmitting}
								className="h-7 text-[13px] md:text-[13px]"
							/>
						)}
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
									{mode === "clone" ? "Cloning…" : "Creating…"}
								</>
							) : mode === "clone" ? (
								"Clone repository"
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
