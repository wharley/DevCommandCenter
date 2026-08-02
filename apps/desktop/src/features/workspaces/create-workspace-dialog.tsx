import {
	Box,
	Boxes,
	Check,
	CheckCircle2,
	FolderOpen,
	GitBranch,
	Link2,
	LoaderCircle,
	ShieldCheck,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
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
	CreateWorkspaceFromSourceUrlInput,
	CreateWorkspaceFromUrlInput,
	Repository,
	WorkspaceSourceUrlResolution,
} from "@dcc/contracts";
import type {
	WorkspaceBundleCreationResult,
	WorkspaceCreationResult,
} from "./use-workspaces";
import {
	includePickedRepository,
	inferProjectIdFromWorkspaceRoot,
	normalizeWorkspaceRoot,
	repositoryNameFromWorkspaceRoot,
} from "./create-workspace-dialog.logic";
import { listLocalBranches, resolveWorkspaceSourceUrl } from "../../lib/workspace-api";
import {
	setupHintsDescription,
	setupReportDescription,
} from "./workspace-setup-report";
import { cn } from "@/lib/utils";
import { repositoryDisplayName } from "./repository-display-name";
import { ProjectIdentityGlyph } from "./project-identity";

type WorkspaceCreationMode = "open" | "clone";

type RepositoryOption = Pick<
	Repository,
	| "id"
	| "projectId"
	| "name"
	| "displayName"
	| "icon"
	| "color"
	| "rootPath"
	| "baseBranch"
>;

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
	onCreateWorkspaceFromSourceUrl: (
		input: CreateWorkspaceFromSourceUrlInput,
	) => Promise<WorkspaceCreationResult>;
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

export function notifyWorkspaceCreationResult(
	t: (key: string, options?: Record<string, string>) => string,
	mode: WorkspaceCreationMode,
	result: WorkspaceCreationResult,
) {
	const successTitle =
		mode === "clone"
			? t("workspaceDialog.toastCloneSuccess")
			: t("workspaceDialog.toastCreateSuccess");

	switch (result.setupReport.status) {
		case "pending":
			toast.success(successTitle, {
				description: setupReportDescription(t, result.setupReport, result.setupHints),
			});
			return;
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
	onCreateWorkspaceFromSourceUrl,
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
	const [workspaceStart, setWorkspaceStart] = useState<"new" | "source">("new");
	const [sourceUrl, setSourceUrl] = useState("");
	const [validatedSourceUrl, setValidatedSourceUrl] = useState("");
	const [sourceResolution, setSourceResolution] =
		useState<WorkspaceSourceUrlResolution | null>(null);
	const [isResolvingSource, setIsResolvingSource] = useState(false);
	const [selectedRepositoryIds, setSelectedRepositoryIds] = useState<string[]>([]);
	const [selectedSingleRepositoryId, setSelectedSingleRepositoryId] = useState<
		string | null
	>(null);
	const [pickedRepository, setPickedRepository] =
		useState<RepositoryOption | null>(null);
	const branchLoadSequenceRef = useRef(0);
	const suppressCloseAutoFocusRef = useRef(false);
	const singleRepositoryOptions = useMemo<RepositoryOption[]>(
		() => includePickedRepository(repositories, pickedRepository),
		[pickedRepository, repositories],
	);

	useEffect(() => {
		if (open) {
			branchLoadSequenceRef.current += 1;
			suppressCloseAutoFocusRef.current = false;
			const contextRepository = repositoryContext
				? repositories.find(
						(repository) => repository.rootPath === repositoryContext.workspaceRoot,
					)
				: null;
			const initialRepository =
				mode === "open" ? (contextRepository ?? repositories[0] ?? null) : null;
			const initialForm = initialRepository
				? {
						...buildInitialForm(mode, repositoryContext),
						projectId: initialRepository.projectId,
						workspaceRoot: initialRepository.rootPath,
						baseBranch: initialRepository.baseBranch,
					}
				: buildInitialForm(mode, repositoryContext);
			setForm(initialForm);
			setAvailableBranches([]);
			setIsLoadingBranches(false);
			setCreationScope("single");
			setWorkspaceStart("new");
			setSourceUrl("");
			setValidatedSourceUrl("");
			setSourceResolution(null);
			setIsResolvingSource(false);
			setSelectedRepositoryIds([]);
			setSelectedSingleRepositoryId(initialRepository?.id ?? null);
			setPickedRepository(null);
			if (mode === "open" && initialForm.workspaceRoot.trim().length > 0) {
				void loadBranchesForWorkspaceRoot(initialForm.workspaceRoot);
			}
		}
	}, [mode, open, repositoryContext]);

	function selectSingleRepository(repository: RepositoryOption) {
		setSelectedSingleRepositoryId(repository.id);
		setForm((current) => ({
			...current,
			projectId: repository.projectId,
			workspaceRoot: repository.rootPath,
			baseBranch: repository.baseBranch,
		}));
		void loadBranchesForWorkspaceRoot(repository.rootPath);
	}

	async function loadBranchesForWorkspaceRoot(
		workspaceRoot: string,
	): Promise<string[] | null> {
		if (mode !== "open" || workspaceRoot.trim().length === 0) {
			setAvailableBranches([]);
			return [];
		}

		const requestSequence = ++branchLoadSequenceRef.current;
		const requestedRoot = workspaceRoot.trim();
		setIsLoadingBranches(true);
		try {
			const result = await listLocalBranches({
				workspaceRoot: requestedRoot,
			});
			if (requestSequence !== branchLoadSequenceRef.current) {
				return null;
			}
			setAvailableBranches(result.branches);
			setForm((current) => {
				if (
					normalizeWorkspaceRoot(current.workspaceRoot) !==
					normalizeWorkspaceRoot(requestedRoot)
				) {
					return current;
				}
				if (
					current.baseBranch.trim().length > 0 &&
					result.branches.includes(current.baseBranch)
				) {
					return current;
				}

				return {
					...current,
					baseBranch: result.branches[0] ?? "",
				};
			});
			return result.branches;
		} catch (error) {
			if (requestSequence !== branchLoadSequenceRef.current) {
				return null;
			}
			setAvailableBranches([]);
			setForm((current) =>
				normalizeWorkspaceRoot(current.workspaceRoot) ===
				normalizeWorkspaceRoot(requestedRoot)
					? { ...current, baseBranch: "" }
					: current,
			);
			const message = error instanceof Error ? error.message : String(error);
			toast.error(t("workspaceDialog.toastLoadBranchesError"), {
				description: message,
			});
			return null;
		} finally {
			if (requestSequence === branchLoadSequenceRef.current) {
				setIsLoadingBranches(false);
			}
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
			if (mode === "clone") {
				setForm((current) => ({
					...current,
					workspaceRoot: pickedPath,
					projectId:
						current.projectId.trim().length > 0
							? current.projectId
							: inferProjectIdFromWorkspaceRoot(pickedPath),
				}));
				return;
			}

			const normalizedPickedPath = normalizeWorkspaceRoot(pickedPath);
			const trackedRepository = repositories.find(
				(repository) =>
					normalizeWorkspaceRoot(repository.rootPath) === normalizedPickedPath,
			);
			if (trackedRepository) {
				setPickedRepository(null);
				selectSingleRepository(trackedRepository);
				return;
			}

			const projectId = inferProjectIdFromWorkspaceRoot(pickedPath);
			const repositoryOption: RepositoryOption = {
				id: `picked:${normalizedPickedPath}`,
				projectId,
				name: repositoryNameFromWorkspaceRoot(pickedPath),
				displayName: null,
				icon: null,
				color: null,
				rootPath: pickedPath,
				baseBranch: "",
			};
			setPickedRepository(repositoryOption);
			setSelectedSingleRepositoryId(repositoryOption.id);

			setForm((current) => ({
				...current,
				workspaceRoot: pickedPath,
				projectId,
				baseBranch: "",
			}));
			const branches = await loadBranchesForWorkspaceRoot(pickedPath);
			if (branches) {
				setPickedRepository((current) =>
					current?.id === repositoryOption.id
						? { ...current, baseBranch: branches[0] ?? "" }
						: current,
				);
			}
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			toast.error(t("workspaceDialog.toastPickerError"), {
				description: message,
			});
		}
	}

	async function handleResolveSourceUrl() {
		if (!repositoryContext || sourceUrl.trim().length === 0) {
			return;
		}
		setIsResolvingSource(true);
		setSourceResolution(null);
		setValidatedSourceUrl("");
		try {
			const resolution = await resolveWorkspaceSourceUrl({
				workspaceRoot: repositoryContext.workspaceRoot,
				url: sourceUrl.trim(),
				forgeLogin: null,
			});
			setSourceResolution(resolution);
			setValidatedSourceUrl(sourceUrl.trim());
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			toast.error(t("workspaceDialog.sourceValidationError"), {
				description: message,
			});
		} finally {
			setIsResolvingSource(false);
		}
	}

	const isSourceWorkspace =
		mode === "open" &&
		creationScope === "single" &&
		repositoryContext !== null &&
		workspaceStart === "source";

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

		if (isSourceWorkspace) {
			return (
				sourceResolution !== null &&
				validatedSourceUrl === sourceUrl.trim() &&
				!isResolvingSource
			);
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
		isResolvingSource,
		isSourceWorkspace,
		mode,
		selectedRepositoryIds.length,
		sourceResolution,
		sourceUrl,
		validatedSourceUrl,
	]);
	const protectedWorktreeCount =
		creationScope === "multi" ? selectedRepositoryIds.length : 1;
	const protectionBranch = isSourceWorkspace
		? sourceResolution?.baseBranch ?? null
		: form.baseBranch.trim() || null;

	async function handleSubmit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		if (!canSubmit) {
			return;
		}

		// Workspace selection can close this dialog before the async submit handler
		// resumes. Mark the close up front so Radix never restores focus to the
		// trigger after the newly selected task has focused its composer.
		suppressCloseAutoFocusRef.current = true;
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
						name: repositoryDisplayName(repository),
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
			} else if (isSourceWorkspace && repositoryContext) {
				const result = await onCreateWorkspaceFromSourceUrl({
					projectId: repositoryContext.projectId,
					workspaceRoot: repositoryContext.workspaceRoot,
					url: sourceUrl.trim(),
					name: form.name.trim() || null,
					forgeLogin: null,
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
			suppressCloseAutoFocusRef.current = false;
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
			<DialogContent
				onCloseAutoFocus={(event) => {
					if (suppressCloseAutoFocusRef.current) {
						event.preventDefault();
						suppressCloseAutoFocusRef.current = false;
					}
				}}
				className="max-h-[min(46rem,calc(100vh-2rem))] w-[min(calc(100vw-2rem),38rem)] max-w-[38rem] gap-4 overflow-y-auto overflow-x-hidden p-5 sm:w-[38rem] sm:max-w-[38rem]"
			>
				<DialogHeader className="min-w-0 space-y-1">
					<DialogTitle className="text-[15px] font-medium tracking-[-0.015em]">
						{mode === "clone" ? t("workspaceDialog.cloneTitle") : t("workspaceDialog.createTitle")}
					</DialogTitle>
					<DialogDescription className="min-w-0 text-[12px] leading-snug text-muted-foreground">
						{mode === "clone" ? t("workspaceDialog.cloneDescription") : t("workspaceDialog.createDescription")}
					</DialogDescription>
					{mode === "open" && repositoryContext ? (
						<div className="mt-2 flex min-w-0 items-center gap-2.5 overflow-hidden rounded-lg border border-border/60 bg-muted/20 px-3 py-2.5 text-[11.5px] leading-5 text-muted-foreground">
							<span className="grid size-8 shrink-0 place-items-center rounded-lg bg-background text-foreground ring-1 ring-border/60">
								<Box className="size-4" strokeWidth={1.8} />
							</span>
							<span className="min-w-0">
								<span className="block truncate font-medium text-foreground">
									{t("workspaceDialog.usingTrackedRepository", {
										label: repositoryContext.label,
									})}
								</span>
								<span className="block truncate font-mono text-[10px]">
									{repositoryContext.workspaceRoot}
								</span>
							</span>
						</div>
					) : null}
				</DialogHeader>

				<form
					onSubmit={handleSubmit}
					className="flex min-w-0 flex-col gap-3 overflow-x-hidden"
				>
					{mode === "open" ? (
						<div className="space-y-2">
							<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
								{t("workspaceDialog.taskScopeLabel")}
							</p>
							<div className="grid grid-cols-2 gap-2 max-[500px]:grid-cols-1">
								<button
									type="button"
									aria-pressed={creationScope === "single"}
									className={cn(
										"flex min-w-0 items-start gap-2.5 rounded-xl border p-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
										creationScope === "single"
											? "border-emerald-500/35 bg-emerald-500/[0.07]"
											: "border-border/60 bg-muted/10 hover:bg-muted/35",
									)}
									disabled={isSubmitting}
									onClick={() => setCreationScope("single")}
								>
									<span className="grid size-8 shrink-0 place-items-center rounded-lg bg-background ring-1 ring-border/50">
										<Box className="size-4" strokeWidth={1.8} />
									</span>
									<span className="min-w-0">
										<strong className="block text-[12px] font-medium text-foreground">
											{t("workspaceDialog.singleWorkspace")}
										</strong>
										<small className="mt-1 block text-[10.5px] leading-4 text-muted-foreground">
											{t("workspaceDialog.singleWorkspaceDescription")}
										</small>
									</span>
								</button>
								<button
									type="button"
									aria-pressed={creationScope === "multi"}
									className={cn(
										"flex min-w-0 items-start gap-2.5 rounded-xl border p-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-45",
										creationScope === "multi"
											? "border-emerald-500/35 bg-emerald-500/[0.07]"
											: "border-border/60 bg-muted/10 hover:bg-muted/35",
									)}
									disabled={isSubmitting || repositories.length < 2}
									onClick={() => {
										setCreationScope("multi");
										setWorkspaceStart("new");
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
									<span className="grid size-8 shrink-0 place-items-center rounded-lg bg-background ring-1 ring-border/50">
										<Boxes className="size-4" strokeWidth={1.8} />
									</span>
									<span className="min-w-0">
										<strong className="block text-[12px] font-medium text-foreground">
											{t("workspaceDialog.multiWorkspace")}
										</strong>
										<small className="mt-1 block text-[10.5px] leading-4 text-muted-foreground">
											{t("workspaceDialog.multiWorkspaceDescription")}
										</small>
									</span>
								</button>
							</div>
						</div>
					) : null}

					{mode === "open" &&
					creationScope === "single" &&
					repositoryContext === null &&
					(repositories.length > 0 || pickedRepository !== null) ? (
						<div className="space-y-2">
							<div className="flex items-center justify-between gap-3">
								<div>
									<p className="text-[12px] font-medium text-foreground">
										{t("workspaceDialog.singleProjectLabel")}
									</p>
									<p className="text-[10.5px] text-muted-foreground">
										{t("workspaceDialog.singleProjectDescription")}
									</p>
								</div>
								<Button
									type="button"
									variant="ghost"
									size="sm"
									className="h-7 shrink-0 gap-1.5 px-2 text-[11px] text-muted-foreground"
									disabled={isSubmitting}
									onClick={() => void handlePickWorkspaceRoot()}
								>
									<FolderOpen className="size-3.5" />
									{t("workspaceDialog.otherFolder")}
								</Button>
							</div>
							<div className="grid max-h-36 grid-cols-2 gap-1.5 overflow-y-auto rounded-xl border border-border/60 bg-muted/10 p-2 max-[500px]:grid-cols-1">
								{singleRepositoryOptions.map((repository) => {
									const selected = selectedSingleRepositoryId === repository.id;
									return (
										<button
											type="button"
											key={repository.id}
											aria-pressed={selected}
											className={cn(
												"flex min-w-0 items-center gap-2 rounded-lg border px-2.5 py-2 text-left transition-colors",
												selected
													? "border-emerald-500/30 bg-emerald-500/[0.055]"
													: "border-transparent hover:border-border/50 hover:bg-muted/40",
											)}
											disabled={isSubmitting}
											onClick={() => selectSingleRepository(repository)}
										>
											<ProjectIdentityGlyph
												icon={repository.icon}
												color={repository.color}
												size="sm"
												className="size-6"
											/>
											<span className="min-w-0 flex-1">
												<strong className="block truncate text-[11px] font-medium text-foreground">
													{repositoryDisplayName(repository)}
												</strong>
												<small className="block truncate text-[9.5px] text-muted-foreground">
													{selected && isLoadingBranches && !repository.baseBranch
														? t("workspaceDialog.loadingBranches")
														: t("workspaceDialog.basePreview", {
																branch: repository.baseBranch,
															})}
												</small>
											</span>
											{selected ? (
												<Check className="size-3.5 shrink-0 text-emerald-600 dark:text-emerald-400" />
											) : null}
										</button>
									);
								})}
							</div>
						</div>
					) : null}

					{mode === "open" &&
					creationScope === "single" &&
					repositoryContext ? (
						<div className="grid grid-cols-2 gap-1 rounded-md border border-border/60 p-1">
							<Button
								type="button"
								variant={workspaceStart === "new" ? "secondary" : "ghost"}
								size="sm"
								className="h-7 text-[12px]"
								disabled={isSubmitting || isResolvingSource}
								onClick={() => setWorkspaceStart("new")}
							>
								{t("workspaceDialog.newWorkspaceStart")}
							</Button>
							<Button
								type="button"
								variant={workspaceStart === "source" ? "secondary" : "ghost"}
								size="sm"
								className="h-7 gap-1.5 text-[12px]"
								disabled={isSubmitting || isResolvingSource}
								onClick={() => setWorkspaceStart("source")}
							>
								<Link2 className="size-3.5" aria-hidden />
								{t("workspaceDialog.existingBranchOrPr")}
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
							<div className="max-h-52 space-y-1.5 overflow-y-auto rounded-xl border border-border/60 bg-muted/10 p-2">
								{repositories.map((repository) => {
									const checked = selectedRepositoryIds.includes(repository.id);
									return (
										<label
											key={repository.id}
											className={cn(
												"flex cursor-pointer items-center gap-2.5 rounded-lg border px-2.5 py-2 transition-colors",
												checked
													? "border-emerald-500/30 bg-emerald-500/[0.055]"
													: "border-transparent hover:border-border/50 hover:bg-muted/40",
											)}
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
												className="sr-only"
											/>
											<span
												className={cn(
													"grid size-5 shrink-0 place-items-center rounded-md border",
													checked
														? "border-emerald-500/40 bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
														: "border-border/70 bg-background text-transparent",
												)}
											>
												<Check className="size-3" strokeWidth={2.4} />
											</span>
											<ProjectIdentityGlyph
												icon={repository.icon}
												color={repository.color}
												size="sm"
											/>
											<span className="min-w-0">
												<span className="block truncate text-[12px] font-medium">
													{repositoryDisplayName(repository)}
												</span>
												<span className="flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground">
													<span className="truncate font-mono">{repository.rootPath}</span>
													<span className="shrink-0">·</span>
													<span className="shrink-0">
														{t("workspaceDialog.basePreview", {
															branch: repository.baseBranch,
														})}
													</span>
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

					{isSourceWorkspace ? (
						<div className="flex min-w-0 flex-col gap-2">
							<div className="flex min-w-0 flex-col gap-1">
								<Label
									htmlFor="workspace-source-url"
									className="text-[12px] font-medium tracking-[-0.01em]"
								>
									{t("workspaceDialog.sourceUrl")}
								</Label>
								<p className="text-[11px] leading-snug text-muted-foreground">
									{t("workspaceDialog.sourceUrlDescription")}
								</p>
								<div className="flex min-w-0 gap-2">
									<Input
										id="workspace-source-url"
										value={sourceUrl}
										onChange={(event) => {
											setSourceUrl(event.target.value);
											setSourceResolution(null);
											setValidatedSourceUrl("");
										}}
										placeholder={t("workspaceDialog.sourceUrlPlaceholder")}
										autoComplete="off"
										spellCheck={false}
										disabled={isSubmitting || isResolvingSource}
										className="h-7 min-w-0 flex-1 font-mono text-[12px] md:text-[12px]"
									/>
									<Button
										type="button"
										variant="outline"
										size="sm"
										className="h-7 shrink-0 gap-1.5"
										disabled={
											isSubmitting ||
											isResolvingSource ||
											sourceUrl.trim().length === 0
										}
										onClick={() => void handleResolveSourceUrl()}
									>
										{isResolvingSource ? (
											<LoaderCircle className="size-3.5 animate-spin" aria-hidden />
										) : (
											<Link2 className="size-3.5" aria-hidden />
										)}
										{t("workspaceDialog.validateSource")}
									</Button>
								</div>
							</div>
							{sourceResolution ? (
								<div className="rounded-md border border-emerald-500/30 bg-emerald-500/5 px-2.5 py-2 text-[11px]">
									<div className="flex items-center gap-1.5 font-medium text-foreground">
										<CheckCircle2
											className="size-3.5 text-emerald-600 dark:text-emerald-400"
											aria-hidden
										/>
										{sourceResolution.kind === "pull_request"
											? t("workspaceDialog.pullRequestResolved", {
													number: String(sourceResolution.changeRequestNumber ?? ""),
												})
											: t("workspaceDialog.branchResolved")}
									</div>
									{sourceResolution.title ? (
										<p className="mt-1 truncate text-foreground">
											{sourceResolution.title}
										</p>
									) : null}
									<p className="mt-1 break-all font-mono text-muted-foreground">
										{sourceResolution.headBranch} → {sourceResolution.baseBranch}
									</p>
									{sourceResolution.isCrossRepository ? (
										<p className="mt-1 text-muted-foreground">
											{t("workspaceDialog.forkSource", {
												repository:
													sourceResolution.sourceRepository ??
													sourceResolution.repository,
											})}
										</p>
									) : null}
								</div>
							) : null}
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
						className={
							creationScope === "multi" ||
							isSourceWorkspace ||
							mode === "open"
								? "hidden"
								: "flex min-w-0 flex-col gap-1"
						}
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
						className={
							creationScope === "multi" ||
							isSourceWorkspace ||
							(mode === "open" &&
								(repositoryContext !== null || selectedSingleRepositoryId !== null))
								? "hidden"
								: "flex min-w-0 flex-col gap-1"
						}
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
							onChange={(event) => {
								const workspaceRoot = event.target.value;
								setForm((current) => ({
									...current,
									workspaceRoot,
									projectId:
										mode === "open"
											? inferProjectIdFromWorkspaceRoot(workspaceRoot)
											: current.projectId,
								}));
							}}
							onBlur={() => {
								if (mode === "open" && form.workspaceRoot.trim().length > 0) {
									if (form.projectId.trim().length === 0) {
										setForm((current) => ({
											...current,
											projectId: inferProjectIdFromWorkspaceRoot(
												current.workspaceRoot,
											),
										}));
									}
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
						className={
							creationScope === "multi" || isSourceWorkspace
								? "hidden"
								: "flex min-w-0 flex-col gap-1"
						}
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

					{mode === "open" ? (
						<div className="flex min-w-0 items-start gap-3 rounded-xl border border-emerald-500/25 bg-emerald-500/[0.055] p-3">
							<span className="grid size-9 shrink-0 place-items-center rounded-lg bg-emerald-500/12 text-emerald-600 dark:text-emerald-400">
								<ShieldCheck className="size-[18px]" strokeWidth={1.9} />
							</span>
							<div className="min-w-0 flex-1">
								<p className="text-[12px] font-medium text-foreground">
									{creationScope === "multi" && protectedWorktreeCount < 2
										? t("workspaceDialog.protectionPending")
										: t("workspaceDialog.protectionPreview", {
												count: protectedWorktreeCount,
											})}
								</p>
								<p className="mt-1 text-[10.5px] leading-4 text-muted-foreground">
									{t("workspaceDialog.protectionDescription")}
								</p>
							</div>
							{creationScope === "single" && protectionBranch ? (
								<span className="inline-flex max-w-36 shrink-0 items-center gap-1 rounded-md border border-emerald-500/20 bg-background/60 px-2 py-1 text-[10px] text-muted-foreground">
									<GitBranch className="size-3 shrink-0" strokeWidth={1.8} />
									<span className="truncate">
										{t("workspaceDialog.basePreview", {
											branch: protectionBranch,
										})}
									</span>
								</span>
							) : null}
						</div>
					) : null}

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
							) : isSourceWorkspace ? (
								t("workspaceDialog.sourceSubmit")
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
