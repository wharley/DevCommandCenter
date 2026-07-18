import { LoaderCircle, FolderOpen } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "../../components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "../../components/ui/dialog";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import type {
	CreateWorkspaceBundleForReposInput,
	CreateWorkspaceForRepoInput,
	CreateWorkspaceFromUrlInput,
	Repository,
} from "@dcc/contracts";
import type {
	WorkspaceBundleCreationResult,
	WorkspaceCreationResult,
} from "./use-workspaces";
import { inferProjectIdFromWorkspaceRoot } from "./create-workspace-dialog.logic";
import { listLocalBranches } from "../../lib/workspace-api";
import {
	setupHintsDescription,
	setupReportDescription,
} from "./workspace-setup-report";

type WorkspaceCreationMode = "open" | "clone";

export type ExistingRepositoryContext = {
	projectId: string;
	workspaceRoot: string;
	label: string;
};

type CreateWorkspaceDialogProps = {
	open: boolean;
	mode: WorkspaceCreationMode;
	repositoryContext?: ExistingRepositoryContext | null;
	onOpenChange: (open: boolean) => void;
	onCreateWorkspace: (input: CreateWorkspaceForRepoInput) => Promise<WorkspaceCreationResult>;
	onCreateWorkspaceBundle: (
		input: CreateWorkspaceBundleForReposInput,
	) => Promise<WorkspaceBundleCreationResult>;
	onCloneWorkspace: (input: CreateWorkspaceFromUrlInput) => Promise<WorkspaceCreationResult>;
	repositories: Repository[];
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

function buildInitialForm(
	mode: WorkspaceCreationMode,
	repositoryContext: ExistingRepositoryContext | null | undefined,
) {
	const initial = getInitialForm(mode);
	if (mode !== "open" || !repositoryContext) {
		return initial;
	}

	return {
		...initial,
		projectId: repositoryContext.projectId,
		workspaceRoot: repositoryContext.workspaceRoot,
		baseBranch: "",
	};
}

function notifyWorkspaceCreationResult(
	t: (key: string, options?: Record<string, string>) => string,
	mode: WorkspaceCreationMode,
	result: WorkspaceCreationResult,
) {
	const successTitle =
		mode === "clone"
			? t("workspaceDialog.toastCloneSuccess")
			: t("workspaceDialog.toastCreateSuccess");

	switch (result.setupReport.status) {
		case "completed":
			toast.success(successTitle, {
				description: setupReportDescription(t, result.setupReport, result.setupHints),
			});
			return;
		case "warning":
			toast.warning(successTitle, {
				description: setupReportDescription(t, result.setupReport, result.setupHints),
			});
			return;
		case "failed":
			toast.error(t("workspaceDialog.toastSetupFailedTitle"), {
				description: setupReportDescription(t, result.setupReport, result.setupHints),
			});
			return;
		default:
			toast.success(successTitle, {
				description: setupHintsDescription(t, result.setupHints),
			});
	}
}

export function CreateWorkspaceDialog({
	open,
	mode,
	repositoryContext = null,
	onOpenChange,
	onCreateWorkspace,
	onCreateWorkspaceBundle,
	onCloneWorkspace,
	repositories,
	isSubmitting,
}: CreateWorkspaceDialogProps) {
	const { t } = useTranslation("common");
	const [form, setForm] = useState(INITIAL_FORM);
	const [availableBranches, setAvailableBranches] = useState<string[]>([]);
	const [isLoadingBranches, setIsLoadingBranches] = useState(false);
	const [creationScope, setCreationScope] = useState<"single" | "multi">("single");
	const [selectedRepositoryIds, setSelectedRepositoryIds] = useState<string[]>([]);

	useEffect(() => {
		if (open) {
			const initialForm = buildInitialForm(mode, repositoryContext);
			setForm(initialForm);
			setAvailableBranches([]);
			setIsLoadingBranches(false);
			setCreationScope("single");
			setSelectedRepositoryIds([]);
			if (mode === "open" && initialForm.workspaceRoot.trim().length > 0) {
				void loadBranchesForWorkspaceRoot(initialForm.workspaceRoot);
			}
		}
	}, [mode, open, repositoryContext]);

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
			toast.error(t("workspaceDialog.toastLoadBranchesError"), {
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
						? t("workspaceDialog.pickFolderClone")
						: t("workspaceDialog.pickFolderRepo"),
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
			toast.error(t("workspaceDialog.toastPickerError"), {
				description: message,
			});
		}
	}

	const canSubmit = useMemo(() => {
		if (mode === "open" && creationScope === "multi") {
			return (
				form.name.trim().length > 0 &&
				selectedRepositoryIds.length >= 2 &&
				!isSubmitting
			);
		}
		const hasCommonFields =
			form.projectId.trim().length > 0 &&
			form.workspaceRoot.trim().length > 0 &&
			!isSubmitting;

		if (mode === "clone") {
			return hasCommonFields && form.repositoryUrl.trim().length > 0;
		}

		return hasCommonFields && form.baseBranch.trim().length > 0;
	}, [
		creationScope,
		form.baseBranch,
		form.name,
		form.projectId,
		form.repositoryUrl,
		form.workspaceRoot,
		isSubmitting,
		mode,
		selectedRepositoryIds.length,
	]);

	async function handleSubmit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		if (!canSubmit) {
			return;
		}

		try {
			if (mode === "open" && creationScope === "multi") {
				const selectedRepositories = repositories.filter((repository) =>
					selectedRepositoryIds.includes(repository.id),
				);
				const result = await onCreateWorkspaceBundle({
					name: form.name.trim(),
					projects: selectedRepositories.map((repository) => ({
						projectId: repository.projectId,
						workspaceRoot: repository.rootPath,
						baseBranch: repository.baseBranch,
						name: repository.name,
					})),
				});
				toast.success(t("workspaceDialog.multiToastSuccess"), {
					description: t("workspaceDialog.multiToastDescription", {
						count: result.workspaces.length,
					}),
				});
			} else if (mode === "clone") {
				const result = await onCloneWorkspace({
					projectId: form.projectId.trim(),
					repositoryUrl: form.repositoryUrl.trim(),
					workspaceRoot: form.workspaceRoot.trim(),
					baseBranch: form.baseBranch.trim(),
					name: form.name.trim() || null,
				});
				notifyWorkspaceCreationResult(t, mode, result);
			} else {
				const result = await onCreateWorkspace({
					projectId: form.projectId.trim(),
					workspaceRoot: form.workspaceRoot.trim(),
					baseBranch: form.baseBranch.trim(),
					name: form.name.trim() || null,
				});
				notifyWorkspaceCreationResult(t, mode, result);
			}
			onOpenChange(false);
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			toast.error(
				mode === "clone" ? t("workspaceDialog.toastCloneError") : t("workspaceDialog.toastCreateError"),
				{
					description: message,
				},
			);
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
			<DialogContent className="w-[min(calc(100vw-2rem),30rem)] max-w-[30rem] gap-3 overflow-hidden p-4 sm:w-[30rem] sm:max-w-[30rem]">
				<DialogHeader className="min-w-0 space-y-1">
					<DialogTitle className="text-[13px] font-medium tracking-[-0.01em]">
						{mode === "clone" ? t("workspaceDialog.cloneTitle") : t("workspaceDialog.createTitle")}
					</DialogTitle>
					<DialogDescription className="min-w-0 text-[12px] leading-snug text-muted-foreground">
						{mode === "clone" ? t("workspaceDialog.cloneDescription") : t("workspaceDialog.createDescription")}
					</DialogDescription>
					{mode === "open" && repositoryContext ? (
						<div className="min-w-0 overflow-hidden rounded-md border border-border/70 bg-muted/30 px-2.5 py-2 text-[11.5px] leading-5 text-muted-foreground">
							<span className="font-medium text-foreground">
								{t("workspaceDialog.usingTrackedRepository", {
									label: repositoryContext.label,
								})}
							</span>
							<span className="block break-all font-mono text-[11px] [overflow-wrap:anywhere]">
								{repositoryContext.workspaceRoot}
							</span>
						</div>
					) : null}
				</DialogHeader>

				<form
					onSubmit={handleSubmit}
					className="flex min-w-0 flex-col gap-3 overflow-x-hidden"
				>
					{mode === "open" ? (
						<div className="grid grid-cols-2 gap-1 rounded-md bg-muted/45 p-1">
							<Button
								type="button"
								variant={creationScope === "single" ? "secondary" : "ghost"}
								size="sm"
								className="h-7 text-[12px]"
								disabled={isSubmitting}
								onClick={() => setCreationScope("single")}
							>
								{t("workspaceDialog.singleWorkspace")}
							</Button>
							<Button
								type="button"
								variant={creationScope === "multi" ? "secondary" : "ghost"}
								size="sm"
								className="h-7 text-[12px]"
								disabled={isSubmitting || repositories.length < 2}
								onClick={() => {
									setCreationScope("multi");
									const contextRepository = repositoryContext
										? repositories.find(
											(repository) =>
												repository.rootPath === repositoryContext.workspaceRoot,
											)
										: null;
									if (contextRepository) {
										setSelectedRepositoryIds((current) =>
											current.includes(contextRepository.id)
												? current
												: [contextRepository.id, ...current],
										);
									}
								}}
							>
								{t("workspaceDialog.multiWorkspace")}
							</Button>
						</div>
					) : null}

					{mode === "open" && creationScope === "multi" ? (
						<div className="flex min-w-0 flex-col gap-2">
							<div>
								<p className="text-[12px] font-medium">
									{t("workspaceDialog.selectProjects")}
								</p>
								<p className="text-[11px] leading-snug text-muted-foreground">
									{t("workspaceDialog.selectProjectsDescription")}
								</p>
							</div>
							<div className="max-h-44 space-y-1 overflow-y-auto rounded-md border border-border/70 p-1.5">
								{repositories.map((repository) => {
									const checked = selectedRepositoryIds.includes(repository.id);
									return (
										<label
											key={repository.id}
											className="flex cursor-pointer items-start gap-2 rounded px-2 py-1.5 hover:bg-muted/60"
										>
											<input
												type="checkbox"
												checked={checked}
												disabled={isSubmitting}
												onChange={() =>
													setSelectedRepositoryIds((current) =>
														checked
															? current.filter((id) => id !== repository.id)
															: [...current, repository.id],
													)
												}
												className="mt-0.5 size-3.5 accent-primary"
											/>
											<span className="min-w-0">
												<span className="block truncate text-[12px] font-medium">
													{repository.name}
												</span>
												<span className="block truncate font-mono text-[10.5px] text-muted-foreground">
													{repository.rootPath}
												</span>
											</span>
										</label>
									);
								})}
							</div>
							<p className="text-[11px] text-muted-foreground">
								{t("workspaceDialog.projectsSelected", {
									count: selectedRepositoryIds.length,
								})}
							</p>
						</div>
					) : null}

					{mode === "clone" ? (
						<div className="flex min-w-0 flex-col gap-1">
							<Label
								htmlFor="workspace-repository-url"
								className="text-[12px] font-medium tracking-[-0.01em]"
							>
								{t("workspaceDialog.repositoryUrl")}
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
								className="h-7 min-w-0 font-mono text-[13px] md:text-[13px]"
							/>
						</div>
					) : null}

					<div
						className={creationScope === "multi" ? "hidden" : "flex min-w-0 flex-col gap-1"}
					>
						<div className="flex min-w-0 flex-wrap items-start justify-between gap-1.5 sm:flex-nowrap sm:items-center sm:gap-2">
							<Label
								htmlFor="workspace-project-id"
								className="text-[12px] font-medium tracking-[-0.01em]"
							>
								{t("workspaceDialog.projectId")}
							</Label>
							<span className="max-w-full text-[11px] leading-snug text-muted-foreground sm:max-w-[11rem] sm:text-right">
								{t("workspaceDialog.autoFilledFromFolder")}
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
							disabled={isSubmitting || (mode === "open" && repositoryContext !== null)}
							className="h-7 min-w-0 text-[13px] md:text-[13px]"
						/>
					</div>

					<div
						className={creationScope === "multi" ? "hidden" : "flex min-w-0 flex-col gap-1"}
					>
						<div className="flex min-w-0 flex-wrap items-start justify-between gap-1.5 sm:flex-nowrap sm:items-center sm:gap-2">
							<Label
								htmlFor="workspace-root"
								className="text-[12px] font-medium tracking-[-0.01em]"
							>
								{mode === "clone"
									? t("workspaceDialog.destinationFolder")
									: t("workspaceDialog.repositoryPath")}
							</Label>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="h-6 shrink-0 self-start gap-1.5 px-2 text-[11px] text-muted-foreground hover:text-foreground sm:self-auto"
								disabled={isSubmitting || (mode === "open" && repositoryContext !== null)}
								onClick={handlePickWorkspaceRoot}
							>
								<FolderOpen className="size-3.5" aria-hidden />
								{t("workspaceDialog.chooseFolder")}
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
							disabled={isSubmitting || (mode === "open" && repositoryContext !== null)}
							className="h-7 min-w-0 font-mono text-[13px] md:text-[13px]"
						/>
					</div>

					<div
						className={creationScope === "multi" ? "hidden" : "flex min-w-0 flex-col gap-1"}
					>
						<Label
							htmlFor="workspace-branch"
							className="text-[12px] font-medium tracking-[-0.01em]"
						>
							{mode === "clone"
								? t("workspaceDialog.baseBranchOptional")
								: t("workspaceDialog.baseBranch")}
						</Label>
						{mode === "open" ? (
							<select
								id="workspace-branch"
								value={form.baseBranch}
								onChange={(event) =>
									setForm((current) => ({ ...current, baseBranch: event.target.value }))
								}
								disabled={isSubmitting || isLoadingBranches || availableBranches.length === 0}
								className="h-7 w-full min-w-0 rounded-md border border-input bg-background px-2 text-[13px] text-foreground shadow-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
							>
								{isLoadingBranches ? (
									<option value="">{t("workspaceDialog.loadingBranches")}</option>
								) : availableBranches.length === 0 ? (
									<option value="">{t("workspaceDialog.chooseFolderForBranches")}</option>
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
								placeholder={t("workspaceDialog.autoDetectPlaceholder")}
								autoComplete="off"
								spellCheck={false}
								disabled={isSubmitting}
								className="h-7 min-w-0 text-[13px] md:text-[13px]"
							/>
						)}
					</div>

					<div className="flex min-w-0 flex-col gap-1">
						<Label
							htmlFor="workspace-name"
							className="text-[12px] font-medium tracking-[-0.01em]"
						>
							{creationScope === "multi"
								? t("workspaceDialog.multiName")
								: t("workspaceDialog.displayName")}{" "}
							{creationScope === "single" ? (
								<span className="font-normal text-muted-foreground">
									({t("workspaceDialog.optional")})
								</span>
							) : null}
						</Label>
						<Input
							id="workspace-name"
							value={form.name}
							onChange={(event) =>
								setForm((current) => ({ ...current, name: event.target.value }))
							}
							placeholder={
								creationScope === "multi"
									? t("workspaceDialog.multiNamePlaceholder")
									: t("workspaceDialog.defaultsFromBranch")
							}
							autoComplete="off"
							spellCheck={false}
							disabled={isSubmitting}
							className="h-7 min-w-0 text-[13px] md:text-[13px]"
						/>
					</div>

					<div className="flex flex-wrap items-center justify-stretch gap-2 pt-0.5 sm:justify-end">
						<Button
							type="button"
							variant="outline"
							size="sm"
							disabled={isSubmitting}
							onClick={() => onOpenChange(false)}
						>
							{t("workspaceDialog.cancel")}
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
								{mode === "clone" ? t("workspaceDialog.cloning") : t("workspaceDialog.creating")}
								</>
							) : mode === "clone" ? (
								t("workspaceDialog.cloneSubmit")
							) : creationScope === "multi" ? (
								t("workspaceDialog.multiSubmit")
							) : (
								t("workspaceDialog.createSubmit")
							)}
						</Button>
					</div>
				</form>
			</DialogContent>
		</Dialog>
	);
}
