import { projectIconValue } from "@dcc/mascots";
import {
	Box,
	Check,
	CheckCircle2,
	FolderOpen,
	GitBranch,
	GitFork,
	Link2,
	LoaderCircle,
	Plus,
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
	WorkspaceIsolationMode,
	WorkspaceSourceUrlResolution,
} from "@dcc/contracts";
import type {
	WorkspaceBundleCreationResult,
	WorkspaceCreationResult,
} from "./use-workspaces";
import {
	includePickedRepository,
	inferProjectIdFromWorkspaceRoot,
	initialTaskRepository,
	initialWorkspaceStart,
	isBranchWorkspaceSource,
	linkedFolderOptionId,
	normalizeWorkspaceRoot,
	linkedFoldersChanged,
	linkedProjectSelection,
	orderMultiProjectSelection,
	withLinkedFolderOptions,
	readLastTaskRepositoryRoot,
	rememberLastTaskRepositoryRoot,
	repositoryNameFromWorkspaceRoot,
	type WorkspaceStart,
} from "./create-workspace-dialog.logic";
import {
	listLocalBranches,
	resolveWorkspaceSourceUrl,
} from "../../lib/workspace-api";
import {
	setupHintsDescription,
	setupReportDescription,
} from "./workspace-setup-report";
import { cn } from "@/lib/utils";
import { repositoryDisplayName } from "./repository-display-name";
import { ProjectIdentityGlyph } from "./project-identity";

import "./workspace-dialog.css";

type WorkspaceCreationMode = "open" | "clone";

const EMPTY_LINKED_FOLDERS: Record<string, string[]> = {};

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
	initialCreationScope?: "single" | "multi";
	/** Carries the *Worktree* choice from New task into a multi-project task. */
	/**
	 * "Open another project": the project is not among the recent ones, so
	 * nothing is preselected and the folder picker opens straight away.
	 */
	startWithFolderPicker?: boolean;
	initialIsolationMode?: WorkspaceIsolationMode;
	/** Each project's linked folders, by repository id. */
	linkedFoldersByRepositoryId?: Record<string, string[]>;
	/** The linked folders without Git, by repository id: context only. */
	contextPathsByRepositoryId?: Record<string, string[]>;
	onSaveLinkedFolders?: (repositoryId: string, paths: string[]) => Promise<void>;
	repositoryContext?: ExistingRepositoryContext | null;
	onOpenChange: (open: boolean) => void;
	onCreateWorkspace: (
		input: CreateWorkspaceForRepoInput,
	) => Promise<WorkspaceCreationResult>;
	onCreateWorkspaceFromSourceUrl: (
		input: CreateWorkspaceFromSourceUrlInput,
	) => Promise<WorkspaceCreationResult>;
	onCreateWorkspaceBundle: (
		input: CreateWorkspaceBundleForReposInput,
	) => Promise<WorkspaceBundleCreationResult>;
	onCloneWorkspace: (
		input: CreateWorkspaceFromUrlInput,
	) => Promise<WorkspaceCreationResult>;
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
			toast.success(successTitle);
			return;
		case "completed":
			toast.success(successTitle, {
				description: setupReportDescription(
					t,
					result.setupReport,
					result.setupHints,
				),
			});
			return;
		case "warning":
			toast.warning(successTitle, {
				description: setupReportDescription(
					t,
					result.setupReport,
					result.setupHints,
				),
			});
			return;
		case "failed":
			toast.error(t("workspaceDialog.toastSetupFailedTitle"), {
				description: setupReportDescription(
					t,
					result.setupReport,
					result.setupHints,
				),
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
	initialCreationScope = "single",
	initialIsolationMode = "protectedWorktree",
	startWithFolderPicker = false,
	linkedFoldersByRepositoryId = EMPTY_LINKED_FOLDERS,
	contextPathsByRepositoryId = EMPTY_LINKED_FOLDERS,
	onSaveLinkedFolders,
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
	const [usesWorktree, setUsesWorktree] = useState(true);
	// The other projects of this task besides the primary, as option ids.
	const [extraRepositoryIds, setExtraRepositoryIds] = useState<string[]>([]);
	// The primary's folders without Git this task brings as context.
	const [includedContextPaths, setIncludedContextPaths] = useState<string[]>([]);
	const [addingProjects, setAddingProjects] = useState(false);
	const [rememberLinkedFolders, setRememberLinkedFolders] = useState(false);
	const [workspaceStart, setWorkspaceStart] = useState<WorkspaceStart>("new");
	const [sourceUrl, setSourceUrl] = useState("");
	const [validatedSourceUrl, setValidatedSourceUrl] = useState("");
	const [sourceResolution, setSourceResolution] =
		useState<WorkspaceSourceUrlResolution | null>(null);
	const [isResolvingSource, setIsResolvingSource] = useState(false);
	const [selectedSingleRepositoryId, setSelectedSingleRepositoryId] = useState<
		string | null
	>(null);
	const [pickedRepository, setPickedRepository] =
		useState<RepositoryOption | null>(null);
	const branchLoadSequenceRef = useRef(0);
	const suppressCloseAutoFocusRef = useRef(false);
	// The linked folders that can be projects (Git); the others only ride
	// along as the primary project's context.
	const linkedProjectFolders = useMemo(
		() =>
			Object.fromEntries(
				Object.entries(linkedFoldersByRepositoryId).map(([repositoryId, paths]) => [
					repositoryId,
					paths.filter(
						(path) => !(contextPathsByRepositoryId[repositoryId] ?? []).includes(path),
					),
				]),
			),
		[contextPathsByRepositoryId, linkedFoldersByRepositoryId],
	);
	// Projects plus the linked Git folders that are not projects yet.
	const multiOptions = useMemo<RepositoryOption[]>(
		() =>
			withLinkedFolderOptions<RepositoryOption>(repositories, linkedProjectFolders, (path) => ({
				id: linkedFolderOptionId(path),
				projectId: inferProjectIdFromWorkspaceRoot(path),
				name: repositoryNameFromWorkspaceRoot(path),
				displayName: null,
				icon: null,
				color: null,
				rootPath: path,
				// Empty: the task starts from that checkout's current branch.
				baseBranch: "",
			})),
		[linkedProjectFolders, repositories],
	);
	const singleRepositoryOptions = useMemo<RepositoryOption[]>(
		() => includePickedRepository(repositories, pickedRepository),
		[pickedRepository, repositories],
	);
	// The task's own project: the one picked here, or the project the dialog
	// was opened from.
	const primaryOption = useMemo<RepositoryOption | null>(() => {
		if (mode !== "open") return null;
		if (repositoryContext) {
			const root = normalizeWorkspaceRoot(repositoryContext.workspaceRoot);
			return (
				repositories.find(
					(repository) => normalizeWorkspaceRoot(repository.rootPath) === root,
				) ?? null
			);
		}
		return (
			singleRepositoryOptions.find((option) => option.id === selectedSingleRepositoryId) ??
			null
		);
	}, [mode, repositories, repositoryContext, selectedSingleRepositoryId, singleRepositoryOptions]);
	const extraOptions = orderMultiProjectSelection(multiOptions, extraRepositoryIds);
	const primaryRegistered = Boolean(
		primaryOption && repositories.some((repository) => repository.id === primaryOption.id),
	);

	/** A new primary brings its linked folders along, already selected. */
	function presetLinkedFolders(primaryId: string | null) {
		setExtraRepositoryIds(
			primaryId ? linkedProjectSelection(primaryId, multiOptions, linkedProjectFolders) : [],
		);
		setIncludedContextPaths(primaryId ? (contextPathsByRepositoryId[primaryId] ?? []) : []);
	}

	useEffect(() => {
		if (open) {
			branchLoadSequenceRef.current += 1;
			suppressCloseAutoFocusRef.current = false;
			const initialRepository =
				mode === "open" && !startWithFolderPicker
					? initialTaskRepository(
							repositories,
							repositoryContext?.workspaceRoot,
							readLastTaskRepositoryRoot(),
						)
					: null;
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
			setAddingProjects(mode === "open" && initialCreationScope === "multi");
			setUsesWorktree(initialIsolationMode !== "localDirect");
			setRememberLinkedFolders(false);
			setWorkspaceStart(initialWorkspaceStart(repositoryContext !== null));
			setSourceUrl("");
			setValidatedSourceUrl("");
			setSourceResolution(null);
			setIsResolvingSource(false);
			presetLinkedFolders(mode === "open" ? (initialRepository?.id ?? null) : null);
			setSelectedSingleRepositoryId(initialRepository?.id ?? null);
			setPickedRepository(null);
			if (mode === "open" && initialForm.workspaceRoot.trim().length > 0) {
				void loadBranchesForWorkspaceRoot(initialForm.workspaceRoot);
			}
			if (mode === "open" && startWithFolderPicker) {
				void handlePickWorkspaceRoot();
			}
		}
	}, [initialCreationScope, initialIsolationMode, mode, open, repositoryContext, startWithFolderPicker]);

	function selectSingleRepository(repository: RepositoryOption) {
		rememberLastTaskRepositoryRoot(repository.rootPath);
		if (repository.id !== selectedSingleRepositoryId) {
			presetLinkedFolders(repository.id);
		}
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
				? (selected[0] ?? "")
				: (selected ?? "");

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
			presetLinkedFolders(null);
			setSelectedSingleRepositoryId(repositoryOption.id);
			rememberLastTaskRepositoryRoot(repositoryOption.rootPath);

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
			if (!isBranchWorkspaceSource(resolution.kind)) {
				toast.error(t("workspaceDialog.pullRequestUrlTitle"), {
					description: t("workspaceDialog.pullRequestUrlDescription"),
				});
				return;
			}
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
		repositoryContext !== null &&
		workspaceStart === "branch";

	const canSubmit = useMemo(() => {
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
		form.baseBranch,
		form.name,
		form.projectId,
		form.repositoryUrl,
		form.workspaceRoot,
		isSubmitting,
		isResolvingSource,
		isSourceWorkspace,
		mode,
		sourceResolution,
		sourceUrl,
		validatedSourceUrl,
	]);
	// A task with other projects becomes a multi-project task.
	const bringsProjects = mode === "open" && !isSourceWorkspace && extraOptions.length > 0;
	const protectedWorktreeCount = 1 + (bringsProjects ? extraOptions.length : 0);
	const localMode = mode === "open" && !isSourceWorkspace && !usesWorktree;
	const taskFolders = [
		...extraOptions.map((option) => option.rootPath),
		...includedContextPaths,
	];
	const canRememberLinkedFolders =
		Boolean(onSaveLinkedFolders) &&
		primaryRegistered &&
		!isSourceWorkspace &&
		primaryOption !== null &&
		linkedFoldersChanged(linkedFoldersByRepositoryId[primaryOption.id] ?? [], taskFolders);
	const protectionBranch = isSourceWorkspace
		? (sourceResolution?.baseBranch ?? null)
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
			const isolationMode: WorkspaceIsolationMode | null = usesWorktree ? null : "localDirect";
			if (bringsProjects && primaryOption) {
				// Other projects make this a multi-project task; the primary keeps
				// the base branch chosen here and brings its context folders.
				const result = await onCreateWorkspaceBundle({
					name: form.name.trim() || t("sidebar.newWorkspace"),
					projects: [
						{
							projectId: form.projectId.trim(),
							workspaceRoot: form.workspaceRoot.trim(),
							baseBranch: form.baseBranch.trim(),
							name: repositoryDisplayName(primaryOption),
							isolationMode,
							...(includedContextPaths.length
								? { contextFolders: includedContextPaths }
								: {}),
						},
						...extraOptions.map((repository) => ({
							projectId: repository.projectId,
							workspaceRoot: repository.rootPath,
							baseBranch: repository.baseBranch,
							name: repositoryDisplayName(repository),
							isolationMode,
						})),
					],
				});
				toast.success(t("workspaceDialog.multiToastSuccess"), {
					description: t(
						usesWorktree
							? "workspaceDialog.multiToastDescription"
							: "workspaceDialog.multiLocalToastDescription",
						{ count: result.workspaces.length },
					),
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
					isolationMode,
					contextFolders: includedContextPaths,
				});
				notifyWorkspaceCreationResult(t, mode, result);
			}
			if (rememberLinkedFolders && canRememberLinkedFolders && primaryOption) {
				try {
					await onSaveLinkedFolders?.(primaryOption.id, taskFolders);
				} catch (error) {
					toast.error(t("workspaceDialog.rememberLinkedFoldersError"), {
						description: error instanceof Error ? error.message : String(error),
					});
				}
			}
			if (mode === "open") {
				rememberLastTaskRepositoryRoot(form.workspaceRoot);
			}
			onOpenChange(false);
		} catch (error) {
			suppressCloseAutoFocusRef.current = false;
			const message = error instanceof Error ? error.message : String(error);
			toast.error(
				mode === "clone"
					? t("workspaceDialog.toastCloneError")
					: t("workspaceDialog.toastCreateError"),
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
				className="workspace-dialog"
				showCloseButton={!isSubmitting}
			>
				<DialogHeader className="workspace-dialog-header">
					<div className="workspace-dialog-heading">
						<span className="workspace-dialog-mark">
							{mode === "clone" ? (
								<GitFork size={22} aria-hidden />
							) : (
								<FolderOpen size={22} aria-hidden />
							)}
						</span>
						<div>
							<DialogTitle className="text-[15px] font-medium tracking-[-0.015em]">
								{mode === "clone"
									? t("workspaceDialog.cloneTitle")
									: repositoryContext || initialCreationScope === "multi"
										? t("workspaceDialog.createTitle")
										: t("workspaceDialog.design.openTitle")}
							</DialogTitle>
							<DialogDescription className="min-w-0 text-[12px] leading-snug text-muted-foreground">
								{mode === "clone"
									? t("workspaceDialog.design.cloneDescription")
									: t("workspaceDialog.createDescription")}
							</DialogDescription>
						</div>
					</div>
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
								<span
									className="workspace-dialog-path font-mono text-[10px]"
									title={repositoryContext.workspaceRoot}
								>
									{repositoryContext.workspaceRoot}
								</span>
							</span>
						</div>
					) : null}
				</DialogHeader>

				<form onSubmit={handleSubmit} className="workspace-dialog-form">
					<div className="workspace-dialog-scroll">
						{mode === "open" &&
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
										const selected =
											selectedSingleRepositoryId === repository.id;
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
													icon={projectIconValue(repository)}
													color={repository.color}
													seed={repository.rootPath}
													size="sm"
													className="size-6"
												/>
												<span className="min-w-0 flex-1">
													<strong
														title={repositoryDisplayName(repository)}
														className="block truncate text-[11px] font-medium text-foreground"
													>
														{repositoryDisplayName(repository)}
													</strong>
													<small className="block truncate text-[9.5px] text-muted-foreground">
														{selected &&
														isLoadingBranches &&
														!repository.baseBranch
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
									variant={workspaceStart === "branch" ? "secondary" : "ghost"}
									size="sm"
									className="h-7 gap-1.5 text-[12px]"
									disabled={isSubmitting || isResolvingSource}
									onClick={() => setWorkspaceStart("branch")}
								>
									<Link2 className="size-3.5" aria-hidden />
									{t("workspaceDialog.existingBranchOrPr")}
								</Button>
							</div>
						) : null}

						{mode === "open" && !isSourceWorkspace && primaryOption ? (
							<div className="flex min-w-0 flex-col gap-2">
								<div className="flex items-center justify-between gap-3">
									<div className="min-w-0">
										<p className="text-[12px] font-medium text-foreground">
											{t("workspaceDialog.alsoInTask")}
										</p>
										<p className="text-[10.5px] text-muted-foreground">
											{t("workspaceDialog.alsoInTaskDescription")}
										</p>
									</div>
									<Button
										type="button"
										variant="ghost"
										size="sm"
										aria-expanded={addingProjects}
										className="h-7 shrink-0 gap-1.5 px-2 text-[11px] text-muted-foreground"
										disabled={isSubmitting}
										onClick={() => setAddingProjects((current) => !current)}
									>
										<Plus className="size-3.5" aria-hidden />
										{t("workspaceDialog.addProject")}
									</Button>
								</div>
								{extraOptions.length > 0 || (contextPathsByRepositoryId[primaryOption.id] ?? []).length > 0 ? (
									<div className="flex min-w-0 flex-wrap gap-1.5">
										{extraOptions.map((repository) => (
											<button
												type="button"
												key={repository.id}
												disabled={isSubmitting}
												title={repository.rootPath}
												aria-label={t("workspaceDialog.removeFromTask", {
													project: repositoryDisplayName(repository),
												})}
												className="inline-flex max-w-full items-center gap-1.5 rounded-full border border-emerald-500/30 bg-emerald-500/[0.07] px-2.5 py-1 text-[11px] font-medium text-foreground transition-colors hover:bg-emerald-500/[0.12]"
												onClick={() =>
													setExtraRepositoryIds((current) =>
														current.filter((id) => id !== repository.id),
													)
												}
											>
												<Check className="size-3 shrink-0 text-emerald-600 dark:text-emerald-400" strokeWidth={2.4} />
												<span className="truncate">{repositoryDisplayName(repository)}</span>
											</button>
										))}
										{(contextPathsByRepositoryId[primaryOption.id] ?? []).map((path) => {
											const included = includedContextPaths.includes(path);
											return (
												<button
													type="button"
													key={path}
													disabled={isSubmitting}
													title={path}
													aria-pressed={included}
													className={cn(
														"inline-flex max-w-full items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] font-medium transition-colors",
														included
															? "border-border bg-muted/50 text-foreground"
															: "border-dashed border-border/70 text-muted-foreground line-through",
													)}
													onClick={() =>
														setIncludedContextPaths((current) =>
															included
																? current.filter((candidate) => candidate !== path)
																: [...current, path],
														)
													}
												>
													<FolderOpen className="size-3 shrink-0" aria-hidden />
													<span className="truncate">{repositoryNameFromWorkspaceRoot(path)}</span>
													<span className="shrink-0 text-[10px] font-normal text-muted-foreground">
														{t("workspaceDialog.contextFolder")}
													</span>
												</button>
											);
										})}
									</div>
								) : (
									<p className="text-[11px] text-muted-foreground">
										{t("workspaceDialog.onlyThisProject")}
									</p>
								)}
								{addingProjects ? (
									<div className="max-h-44 space-y-1 overflow-y-auto rounded-xl border border-border/60 bg-muted/10 p-2">
										{multiOptions
											.filter((repository) => repository.id !== primaryOption.id)
											.map((repository) => {
												const checked = extraRepositoryIds.includes(repository.id);
												return (
													<label
														key={repository.id}
														className={cn(
															"workspace-dialog-project-option flex cursor-pointer items-center gap-2.5 rounded-lg border px-2.5 py-1.5 transition-colors",
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
																setExtraRepositoryIds((current) =>
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
															icon={projectIconValue(repository)}
															color={repository.color}
															seed={repository.rootPath}
															size="sm"
														/>
														<span className="min-w-0 flex-1">
															<span className="block truncate text-[12px] font-medium">
																{repositoryDisplayName(repository)}
															</span>
															<span className="workspace-dialog-project-meta text-[10px] text-muted-foreground">
																<span
																	className="workspace-dialog-path font-mono"
																	title={repository.rootPath}
																>
																	{repository.rootPath}
																</span>
																<span className="shrink-0">·</span>
																<span className="shrink-0">
																	{repository.baseBranch
																		? t("workspaceDialog.basePreview", {
																				branch: repository.baseBranch,
																			})
																		: t("workspaceDialog.linkedFolderBranch")}
																</span>
															</span>
														</span>
													</label>
												);
											})}
									</div>
								) : null}
								{canRememberLinkedFolders ? (
									<label className="inline-flex cursor-pointer items-center gap-2 text-[11.5px] text-muted-foreground has-[:disabled]:cursor-progress has-[:disabled]:opacity-60">
										<input
											type="checkbox"
											checked={rememberLinkedFolders}
											disabled={isSubmitting}
											onChange={(event) => setRememberLinkedFolders(event.target.checked)}
											className="size-3.5 accent-primary"
										/>
										<span>{t("workspaceDialog.rememberLinkedFolders")}</span>
									</label>
								) : null}
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
												<LoaderCircle
													className="size-3.5 animate-spin"
													aria-hidden
												/>
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
											{t("workspaceDialog.branchResolved")}
										</div>
										{sourceResolution.title ? (
											<p className="mt-1 truncate text-foreground">
												{sourceResolution.title}
											</p>
										) : null}
										<p className="mt-1 break-all font-mono text-muted-foreground">
											{sourceResolution.headBranch} →{" "}
											{sourceResolution.baseBranch}
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
							<section className="workspace-dialog-section">
								<h3>
									<Link2 size={15} aria-hidden />
									{t("workspaceDialog.design.origin")}
								</h3>
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
							</section>
						) : null}

						<div
							className={
								mode === "clone"
									? "workspace-dialog-section"
									: "workspace-dialog-field-group"
							}
						>
							{mode === "clone" && (
								<h3>
									<FolderOpen size={15} aria-hidden />
									{t("workspaceDialog.design.destination")}
								</h3>
							)}
							<div
								className={
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
										setForm((current) => ({
											...current,
											projectId: event.target.value,
										}))
									}
									placeholder="dcc-demo"
									autoComplete="off"
									spellCheck={false}
									disabled={
										isSubmitting ||
										(mode === "open" && repositoryContext !== null)
									}
									className="h-7 min-w-0 text-[13px] md:text-[13px]"
								/>
							</div>

							<div
								className={
									isSourceWorkspace ||
									(mode === "open" &&
										(repositoryContext !== null ||
											selectedSingleRepositoryId !== null))
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
										disabled={
											isSubmitting ||
											(mode === "open" && repositoryContext !== null)
										}
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
										if (
											mode === "open" &&
											form.workspaceRoot.trim().length > 0
										) {
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
										mode === "clone"
											? "/path/to/clone/destination"
											: "/path/to/git/repo"
									}
									autoComplete="off"
									spellCheck={false}
									disabled={
										isSubmitting ||
										(mode === "open" && repositoryContext !== null)
									}
									className="h-7 min-w-0 font-mono text-[13px] md:text-[13px]"
								/>
							</div>
						</div>
						<div className="workspace-dialog-section">
							<h3>
								<GitBranch size={15} aria-hidden />
								{t("workspaceDialog.design.taskSetup")}
							</h3>
							<div
								className={
									isSourceWorkspace
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
										title={form.baseBranch}
										value={form.baseBranch}
										onChange={(event) =>
											setForm((current) => ({
												...current,
												baseBranch: event.target.value,
											}))
										}
										disabled={
											isSubmitting ||
											isLoadingBranches ||
											availableBranches.length === 0
										}
										className="h-7 w-full min-w-0 rounded-md border border-input bg-background px-2 text-[13px] text-foreground shadow-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
									>
										{isLoadingBranches ? (
											<option value="">
												{t("workspaceDialog.loadingBranches")}
											</option>
										) : availableBranches.length === 0 ? (
											<option value="">
												{t("workspaceDialog.chooseFolderForBranches")}
											</option>
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
										title={form.baseBranch}
										value={form.baseBranch}
										onChange={(event) =>
											setForm((current) => ({
												...current,
												baseBranch: event.target.value,
											}))
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
									{t("workspaceDialog.displayName")}{" "}
									<span className="font-normal text-muted-foreground">
										({t("workspaceDialog.optional")})
									</span>
								</Label>
								<Input
									id="workspace-name"
									value={form.name}
									onChange={(event) =>
										setForm((current) => ({
											...current,
											name: event.target.value,
										}))
									}
									placeholder={t("workspaceDialog.defaultsFromBranch")}
									autoComplete="off"
									spellCheck={false}
									disabled={isSubmitting}
									className="h-7 min-w-0 text-[13px] md:text-[13px]"
								/>
							</div>
						</div>
						{mode === "open" ? (
							<div className="workspace-dialog-protection">
								{localMode ? (
									<span className="grid size-9 shrink-0 place-items-center rounded-lg bg-amber-500/12 text-amber-600 dark:text-amber-400">
										<FolderOpen className="size-[18px]" strokeWidth={1.9} />
									</span>
								) : (
									<span className="grid size-9 shrink-0 place-items-center rounded-lg bg-emerald-500/12 text-emerald-600 dark:text-emerald-400">
										<ShieldCheck className="size-[18px]" strokeWidth={1.9} />
									</span>
								)}
								<div className="min-w-0 flex-1">
									<p className="text-[12px] font-medium text-foreground">
										{localMode
											? t("workspaceDialog.localPreview", {
													count: protectedWorktreeCount,
												})
											: t("workspaceDialog.protectionPreview", {
													count: protectedWorktreeCount,
												})}
									</p>
									<p className="mt-1 text-[10.5px] leading-4 text-muted-foreground">
										{localMode
											? t("workspaceDialog.localDescription")
											: t("workspaceDialog.protectionDescription")}
									</p>
									{!isSourceWorkspace ? (
										<label className="mt-2 inline-flex cursor-pointer items-center gap-2 text-[11.5px] text-muted-foreground has-[:disabled]:cursor-progress has-[:disabled]:opacity-60">
											<input
												type="checkbox"
												checked={usesWorktree}
												disabled={isSubmitting}
												onChange={(event) => setUsesWorktree(event.target.checked)}
												className="size-3.5 accent-primary"
											/>
											<span className="font-medium text-foreground">
												{t("newTask.execution.worktree")}
											</span>
											<span aria-hidden>·</span>
											<span>
												{t(
													usesWorktree
														? "newTask.execution.protectedDescription"
														: "newTask.execution.localDescription",
												)}
											</span>
										</label>
									) : null}
								</div>
								{protectionBranch ? (
									<span
										className="workspace-dialog-branch"
										title={protectionBranch}
									>
										<GitBranch className="size-3 shrink-0" strokeWidth={1.8} />
										<span>
											{t("workspaceDialog.basePreview", {
												branch: protectionBranch,
											})}
										</span>
									</span>
								) : null}
							</div>
						) : null}
					</div>
					<footer className="workspace-dialog-footer">
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
									{mode === "clone"
										? t("workspaceDialog.cloning")
										: t("workspaceDialog.creating")}
								</>
							) : mode === "clone" ? (
								t("workspaceDialog.cloneSubmit")
							) : bringsProjects ? (
								t("workspaceDialog.multiSubmit")
							) : isSourceWorkspace ? (
								t("workspaceDialog.sourceSubmit")
							) : (
								t("workspaceDialog.createSubmit")
							)}
						</Button>
					</footer>
				</form>
			</DialogContent>
		</Dialog>
	);
}
