import {
	Check,
	ChevronDown,
	ChevronUp,
	FileDiff,
	FolderGit2,
	GitBranch,
	LoaderCircle,
	ShieldCheck,
	Terminal,
	X,
} from "lucide-react";
import { memo, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { WorkspaceSetupReport } from "@dcc/contracts";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { setupDisplayCommand } from "@/features/workspaces/workspace-setup-report";
import { ProjectIdentityGlyph } from "@/features/workspaces/project-identity";
import { ExecutionOriginPicker } from "./ExecutionOriginPicker";
import {
	resolveExecutionDockStatus,
	type ExecutionDockChangeSummary,
	type ExecutionDockGitState,
} from "./ExecutionDock.logic";
import {
	resolveExecutionDockActions,
	type ExecutionDockAction,
	type ExecutionDockRunMode,
} from "./ExecutionDock.actions";
import { commitTranslationKey, type CommitMode } from "@/features/commit/WorkspaceCommitButton.logic";

export type { ExecutionDockChangeSummary } from "./ExecutionDock.logic";

type ExecutionDockProps = {
	projectLabel: string | null;
	projectIcon?: string | null;
	projectColor?: string | null;
	workspacePath: string | null;
	projectRootPath?: string | null;
	baseBranch: string | null;
	currentBranch: string | null;
	isIsolatedWorkspace: boolean;
	changeSummary: ExecutionDockChangeSummary | null;
	gitStatusState?: ExecutionDockGitState;
	contextProjects?: Array<{
		id: string;
		name: string;
		branch: string;
		icon?: string | null;
		color?: string | null;
	}>;
	setupReport?: WorkspaceSetupReport | null;
	onReviewChanges?: () => void;
	onCreateTaskFromBranch?: (branch: string) => Promise<void>;
	onRunRecommendedSetup?: (commands: string[]) => Promise<void>;
	onSkipRecommendedSetup?: () => Promise<void>;
	commitMode?: CommitMode | null;
	forgeRequestLabel?: "PR" | "MR";
	deliveryBusy?: boolean;
	onRunDeliveryAction?: (mode: ExecutionDockRunMode) => Promise<void> | void;
	onCreateChangeRequest?: (draft: boolean) => void;
	onOpenMultiProjectDelivery?: () => void;
};

/**
 * Persistent task execution context attached to the composer. It exposes the
 * safety and Git state users need before sending an instruction, without
 * turning the global chat header into a row of infrastructure badges.
 */
export const ExecutionDock = memo(function ExecutionDock({
	projectLabel,
	projectIcon = null,
	projectColor = null,
	workspacePath,
	projectRootPath = null,
	baseBranch,
	currentBranch,
	isIsolatedWorkspace,
	changeSummary,
	gitStatusState = "ready",
	contextProjects = [],
	setupReport = null,
	onReviewChanges,
	onCreateTaskFromBranch,
	onRunRecommendedSetup,
	onSkipRecommendedSetup,
	commitMode = null,
	forgeRequestLabel = "PR",
	deliveryBusy = false,
	onRunDeliveryAction,
	onCreateChangeRequest,
	onOpenMultiProjectDelivery,
}: ExecutionDockProps) {
	const { t } = useTranslation("common");
	const [isRunningSetup, setIsRunningSetup] = useState(false);
	const [setupError, setSetupError] = useState<string | null>(null);
	const [deliveryMenuOpen, setDeliveryMenuOpen] = useState(false);
	const workingBranch =
		currentBranch && currentBranch !== "HEAD" ? currentBranch : null;
	const displayedProject = projectLabel || t("composer.executionDock.projectFallback");
	const multiProject = contextProjects.length > 1;
	const branchCaption = workingBranch
		? t("composer.executionDock.workingBranch", { branch: workingBranch })
		: baseBranch
			? t("composer.executionDock.baseBranch", { branch: baseBranch })
			: t("composer.executionDock.branchUnavailable");
	const visibleProjects = multiProject
		? contextProjects.slice(0, 3)
		: [
				{
					id: "active",
					name: displayedProject,
					branch: baseBranch ?? "",
					icon: projectIcon,
					color: projectColor,
				},
			];
	const canChangeOrigin = Boolean(
		!multiProject &&
			projectRootPath &&
			onCreateTaskFromBranch,
	);
	const setupCommands =
		setupReport?.steps
			.filter(
				(step) =>
					step.command !== "compile_mission_spec_context" &&
					(step.status === "pending" || step.status === "failed"),
			)
			.map((step) => step.command) ?? [];
	const showSetupRecommendation = setupCommands.length > 0;
	const setupSummary = setupCommands
		.map((command) => setupDisplayCommand(command))
		.join(" · ");
	const gitStatus = resolveExecutionDockStatus(changeSummary, gitStatusState);
	const actionLoading = deliveryBusy || gitStatusState !== "ready";
	const deliveryActions = useMemo(
		() =>
			resolveExecutionDockActions({
				mode: commitMode,
				loading: actionLoading,
				multiProject,
					hasLocalChanges: (changeSummary?.files ?? 0) > 0,
					hasBranchChanges: (changeSummary?.branchFiles ?? 0) > 0,
					hasAheadCommits: (changeSummary?.aheadOfRemoteCount ?? 0) > 0,
					hasChangeRequest: Boolean(changeSummary?.pullRequestState?.trim()),
					hasOpenRequest: Boolean(
					changeSummary?.pullRequestState?.trim().toLowerCase() === "open",
				),
			}),
		[actionLoading, changeSummary, commitMode, multiProject],
	);
	const primaryAction = deliveryActions[0];
	const actionLabel = (action: ExecutionDockAction) => {
		if (multiProject) {
			return t("workspaceScope.delivery.action", { count: contextProjects.length });
		}
		if (action.id === "sync-base") {
			return t("composer.executionDock.actions.syncBase");
		}
		if (action.id === "create-draft-pr") {
			return t("composer.executionDock.actions.createDraft", { requestLabel: forgeRequestLabel });
		}
		if (action.mode === "commit") {
			return t("composer.executionDock.actions.commit");
		}
		if (action.mode === "sync-base") {
			return t("composer.executionDock.actions.syncBase");
		}
		if (action.mode) {
			return t(commitTranslationKey(action.mode, "idle"), { requestLabel: forgeRequestLabel });
		}
		return t("composer.executionDock.actions.git");
	};
	const runDockAction = (action: ExecutionDockAction) => {
		if (action.disabled) return;
		if (action.kind === "create-request") {
			onCreateChangeRequest?.(action.id === "create-draft-pr");
			return;
		}
		if (action.mode) {
			void onRunDeliveryAction?.(action.mode);
			return;
		}
		if (action.id === "primary" && multiProject) {
			onOpenMultiProjectDelivery?.();
		}
	};
	useEffect(() => {
		setSetupError(null);
	}, [projectRootPath, setupReport?.status]);

	async function runRecommendedSetup() {
		if (!onRunRecommendedSetup || isRunningSetup) return;
		setIsRunningSetup(true);
		setSetupError(null);
		try {
			await onRunRecommendedSetup(setupCommands);
		} catch (error) {
			setSetupError(error instanceof Error ? error.message : String(error));
		} finally {
			setIsRunningSetup(false);
		}
	}

	async function skipRecommendedSetup() {
		if (!onSkipRecommendedSetup || isRunningSetup) return;
		setIsRunningSetup(true);
		setSetupError(null);
		try {
			await onSkipRecommendedSetup();
		} catch (error) {
			setSetupError(error instanceof Error ? error.message : String(error));
		} finally {
			setIsRunningSetup(false);
		}
	}
	const projectContextControl = (
		<button
			type="button"
			disabled={!canChangeOrigin}
			className={cn(
				"flex h-full w-full min-w-0 items-center gap-2 border-r border-border/55 px-3 py-2 text-left max-sm:border-r-0",
				canChangeOrigin &&
					"transition-colors hover:bg-muted/35 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40",
			)}
			aria-label={
				canChangeOrigin ? t("composer.executionDock.origin.open") : undefined
			}
		>
			<span className="flex shrink-0 items-center pl-1">
				{visibleProjects.map((project, index) => (
					<ProjectIdentityGlyph
						key={project.id}
						icon={project.icon}
						color={project.color}
						size="sm"
						title={project.name}
						className={cn(
							"size-6",
							index > 0 && "-ml-1.5",
						)}
					/>
				))}
			</span>
			<span className="min-w-0 flex-1">
				<strong className="block truncate text-[11px] font-medium text-foreground">
					{multiProject
						? t("composer.executionDock.coordinatedProjects", {
								count: contextProjects.length,
							})
						: displayedProject}
				</strong>
				<small className="mt-0.5 block truncate text-[10px] leading-none text-muted-foreground">
					{multiProject
						? t("composer.executionDock.independentBases", {
								count: contextProjects.length,
							})
						: branchCaption}
				</small>
			</span>
			{canChangeOrigin ? (
				<ChevronUp className="size-3 shrink-0 text-muted-foreground" strokeWidth={2} />
			) : null}
		</button>
	);

	return (
		<div
			className="relative z-10 mx-2 -mb-px grid min-h-12 grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)_minmax(0,1fr)] overflow-visible rounded-t-xl border border-border/65 border-b-border/40 bg-card/90 shadow-[0_-8px_24px_-22px_rgba(0,0,0,0.45)] backdrop-blur-sm max-sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] dark:shadow-[0_-8px_24px_-22px_rgba(0,0,0,0.9)]"
			aria-label={t("composer.executionDock.ariaLabel")}
		>
			<Popover>
				<PopoverTrigger asChild>
					<button
						type="button"
						className="group flex min-w-0 items-center gap-2.5 rounded-tl-xl border-r border-border/55 px-3 py-2 text-left transition-colors hover:bg-muted/35 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40"
					>
						<span
							className={cn(
								"grid size-7 shrink-0 place-items-center rounded-lg",
								isIsolatedWorkspace
									? "bg-emerald-500/12 text-emerald-600 dark:text-emerald-400"
									: "bg-muted text-muted-foreground",
							)}
						>
							{isIsolatedWorkspace ? (
								<ShieldCheck className="size-4" strokeWidth={1.9} />
							) : (
								<FolderGit2 className="size-4" strokeWidth={1.9} />
							)}
						</span>
						<span className="min-w-0 flex-1">
							<strong className="block truncate text-[11px] font-medium text-foreground">
								{isIsolatedWorkspace
									? multiProject
										? t("composer.executionDock.protectedWorktrees", {
												count: contextProjects.length,
											})
										: t("composer.executionDock.protectedTitle")
									: t("composer.executionDock.localTitle")}
							</strong>
							<small className="mt-0.5 block truncate text-[10px] leading-none text-muted-foreground">
								{isIsolatedWorkspace
									? multiProject
										? t("composer.executionDock.protectedProjectsSubtitle")
										: t("composer.executionDock.protectedSubtitle")
									: t("composer.executionDock.localSubtitle")}
							</small>
						</span>
						<ChevronUp
							className="size-3 shrink-0 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
							strokeWidth={2}
						/>
					</button>
				</PopoverTrigger>
				<PopoverContent
					side="top"
					align="start"
					sideOffset={8}
					collisionPadding={12}
					className="w-[min(23rem,calc(100vw-1.5rem))] max-w-[calc(100vw-1.5rem)] gap-3 overflow-hidden p-3"
				>
					<div className="flex items-start gap-2.5">
						<span className="grid size-8 shrink-0 place-items-center rounded-lg bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">
							{isIsolatedWorkspace ? (
								<ShieldCheck className="size-4" strokeWidth={1.9} />
							) : (
								<FolderGit2 className="size-4" strokeWidth={1.9} />
							)}
						</span>
						<div className="min-w-0">
							<p className="text-[13px] font-medium text-foreground">
								{isIsolatedWorkspace
									? multiProject
										? t("composer.executionDock.popover.protectedProjectsTitle")
										: t("composer.executionDock.popover.protectedTitle")
									: t("composer.executionDock.popover.localTitle")}
							</p>
							<p className="mt-1 text-[11px] leading-4 text-muted-foreground">
								{isIsolatedWorkspace
									? multiProject
										? t("composer.executionDock.popover.protectedProjectsDescription", {
												count: contextProjects.length,
											})
										: t("composer.executionDock.popover.protectedDescription")
									: t("composer.executionDock.popover.localDescription")}
							</p>
						</div>
					</div>
					{multiProject ? (
						<div className="flex flex-wrap gap-1.5">
							{contextProjects.map((project) => (
								<span
									key={project.id}
									className="inline-flex min-w-0 items-center gap-1 rounded-md border border-border/50 bg-muted/25 px-2 py-1 text-[10px] text-muted-foreground"
								>
									<span className="max-w-32 truncate text-foreground/80">
										{project.name}
									</span>
									<span>·</span>
									<span className="max-w-24 truncate">{project.branch}</span>
								</span>
							))}
						</div>
					) : null}
					<div className="grid gap-2 rounded-lg border border-border/50 bg-muted/25 p-2.5 text-[11px] text-muted-foreground">
						<div className="flex min-w-0 items-center gap-2">
							<Check className="size-3.5 shrink-0 text-emerald-600 dark:text-emerald-400" />
							<span className="min-w-0 truncate">
								{isIsolatedWorkspace
									? multiProject
										? t("composer.executionDock.popover.isolatedCheckouts", {
												count: contextProjects.length,
											})
										: t("composer.executionDock.popover.isolatedCheckout")
									: t("composer.executionDock.popover.directCheckout")}
							</span>
						</div>
						<div className="flex min-w-0 items-center gap-2">
							<GitBranch className="size-3.5 shrink-0" />
							<span className="min-w-0 truncate">{branchCaption}</span>
						</div>
						{workspacePath ? (
							<div className="flex min-w-0 items-center gap-2">
								<FolderGit2 className="size-3.5 shrink-0" />
								<div className="min-w-0 flex-1">
									<span className="block text-[9px] leading-none text-muted-foreground/75">
										{t(
											multiProject
												? "composer.executionDock.popover.activeWorktree"
												: "composer.executionDock.popover.worktreePath",
										)}
									</span>
									<code
										className="mt-1 block min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-[10px] text-foreground/75"
										title={workspacePath}
									>
										{workspacePath}
									</code>
								</div>
							</div>
						) : null}
					</div>
				</PopoverContent>
			</Popover>

			<div className="min-w-0 max-sm:col-span-2 max-sm:row-start-2 max-sm:border-t max-sm:border-border/55">
				{canChangeOrigin && projectRootPath && onCreateTaskFromBranch ? (
					<ExecutionOriginPicker
						trigger={projectContextControl}
						projectRootPath={projectRootPath}
						baseBranch={baseBranch}
						onCreateFromBranch={onCreateTaskFromBranch}
					/>
				) : (
					projectContextControl
				)}
			</div>

			<div className="flex min-w-0 rounded-tr-xl">
				<button
					type="button"
					onClick={onReviewChanges}
					disabled={!onReviewChanges}
					className="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 overflow-hidden border-r border-border/45 px-3 py-2 text-[11px] text-muted-foreground/90 transition-colors hover:bg-muted/35 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40 disabled:cursor-default disabled:opacity-70"
					aria-label={t("composer.executionDock.reviewChanges")}
				>
				<FileDiff className="size-3.5 shrink-0" strokeWidth={1.8} />
				{gitStatus.kind === "loading" ? (
					<span className="min-w-0 flex-1 truncate max-[440px]:hidden">
						{t("composer.executionDock.readingChanges")}
					</span>
				) : gitStatus.kind === "error" ? (
					<span className="min-w-0 flex-1 truncate max-[440px]:hidden">
						{t("composer.executionDock.changesUnavailable")}
					</span>
				) : gitStatus.kind === "local" ? (
					<span className="min-w-0 flex-1 truncate tabular-nums">
						<strong className="font-medium text-foreground">{gitStatus.files}</strong>
						<span className="ml-1 max-[440px]:hidden">
							{t("composer.executionDock.changes", {
								count: gitStatus.files,
							})}
						</span>
						{gitStatus.additions > 0 ? (
							<span className="ml-1 text-emerald-600 dark:text-emerald-400">
								+{gitStatus.additions}
							</span>
						) : null}
						{gitStatus.deletions > 0 ? (
							<span className="ml-1 text-destructive">−{gitStatus.deletions}</span>
						) : null}
					</span>
				) : gitStatus.kind === "branch" ? (
					<span className="min-w-0 flex-1 truncate tabular-nums">
						<span className="font-medium text-foreground">
							{t("composer.executionDock.viewDiff")}
						</span>
						<span className="mx-1 text-muted-foreground/80">·</span>
						<strong className="font-medium text-foreground">{gitStatus.files}</strong>
						<span className="ml-1 max-[440px]:hidden">
							{t("composer.executionDock.branchFiles", { count: gitStatus.files })}
						</span>
						{gitStatus.additions > 0 ? (
							<span className="ml-1 text-emerald-600 dark:text-emerald-400">+{gitStatus.additions}</span>
						) : null}
						{gitStatus.deletions > 0 ? (
							<span className="ml-1 text-destructive">−{gitStatus.deletions}</span>
						) : null}
					</span>
				) : gitStatus.kind === "local-and-branch" ? (
					<span className="min-w-0 flex-1 truncate tabular-nums">
						<strong className="font-medium text-foreground">{gitStatus.localFiles}</strong>
						<span className="max-[440px]:hidden">
							{t("composer.executionDock.localChanges", { count: gitStatus.localFiles })}
						</span>
						<span className="mx-1 text-muted-foreground/70">·</span>
						<span className="font-medium text-foreground">
							{t("composer.executionDock.branchDiffShort", { count: gitStatus.branchFiles })}
						</span>
					</span>
				) : gitStatus.kind === "merged" ? (
					<span className="min-w-0 flex-1 truncate text-emerald-700 dark:text-emerald-400">
						{gitStatus.pullRequestNumber
							? t("composer.executionDock.mergedWithNumber", {
									pr: gitStatus.pullRequestNumber,
								})
							: t("composer.executionDock.merged")}
					</span>
				) : gitStatus.kind === "ahead" ? (
					<span className="min-w-0 flex-1 truncate text-foreground/80">
						{t("composer.executionDock.ahead", { count: gitStatus.commits })}
					</span>
				) : (
					<span className="min-w-0 flex-1 truncate text-foreground/65 max-[440px]:hidden">
						{t("composer.executionDock.noChanges")}
					</span>
				)}
				</button>
				<div className="flex min-w-24 max-[440px]:min-w-10">
					<button
						type="button"
						disabled={primaryAction.disabled}
						onClick={() => {
							if (primaryAction.kind === "review") {
								setDeliveryMenuOpen(true);
								return;
							}
							runDockAction(primaryAction);
						}}
						className="flex min-w-0 flex-1 cursor-pointer items-center justify-center gap-1.5 px-2.5 py-2 text-[11px] font-medium text-foreground transition-colors hover:bg-muted/35 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40 disabled:cursor-default disabled:opacity-70"
						aria-label={actionLabel(primaryAction)}
					>
						{deliveryBusy ? <LoaderCircle className="size-3.5 animate-spin" /> : null}
						<span className="truncate max-[440px]:hidden">{actionLabel(primaryAction)}</span>
					</button>
					<Popover open={deliveryMenuOpen} onOpenChange={setDeliveryMenuOpen}>
						<PopoverTrigger asChild>
							<button
								type="button"
								disabled={actionLoading}
								className="flex cursor-pointer items-center justify-center border-l border-border/45 px-2 text-foreground transition-colors hover:bg-muted/35 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40 disabled:cursor-default disabled:opacity-70"
								aria-label={t("composer.executionDock.actions.openMenu")}
							>
								<ChevronDown className="size-3.5 shrink-0" strokeWidth={2} />
							</button>
						</PopoverTrigger>
						<PopoverContent side="top" align="end" sideOffset={8} className="w-64 p-1.5">
							<p className="px-2.5 pb-1.5 pt-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
								{t("composer.executionDock.actions.title")}
							</p>
							{deliveryActions.map((action, index) => (
								<button
									key={`${action.id}-${index}`}
									type="button"
									disabled={action.disabled}
									onClick={() => runDockAction(action)}
									className={cn(
										"flex w-full cursor-pointer items-center justify-between rounded-md px-2.5 py-2 text-left text-[11px] transition-colors hover:bg-muted/70 disabled:cursor-not-allowed disabled:opacity-40",
										action.primary && "bg-muted/55 font-medium",
									)}
								>
									<span>{actionLabel(action)}</span>
									{action.primary ? (
										<span className="text-[10px] text-muted-foreground">
											{t("composer.executionDock.actions.primary")}
										</span>
									) : null}
								</button>
							))}
						</PopoverContent>
					</Popover>
				</div>
			</div>

			{showSetupRecommendation ? (
				<div className="col-span-3 flex min-w-0 items-center gap-2 border-t border-amber-500/20 bg-amber-500/[0.065] px-3 py-2 max-sm:col-span-2 max-sm:flex-wrap">
					<span className="grid size-6 shrink-0 place-items-center rounded-md bg-amber-500/12 text-amber-600 dark:text-amber-400">
						<Terminal className="size-3.5" strokeWidth={1.9} />
					</span>
					<span className="min-w-0 flex-1">
						<strong className="block text-[10.5px] font-medium text-foreground">
							{setupReport?.status === "failed"
								? t("composer.executionDock.setup.failed")
								: t("composer.executionDock.setup.recommended")}
						</strong>
						<small
							className={cn(
								"block truncate font-mono text-[9.5px] text-muted-foreground",
								setupError && "text-destructive",
							)}
							title={setupError ?? setupSummary}
						>
							{setupError ?? setupSummary}
						</small>
					</span>
					<button
						type="button"
						disabled={!onSkipRecommendedSetup || isRunningSetup}
						onClick={() => void skipRecommendedSetup()}
						className="inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-[10px] text-muted-foreground transition-colors hover:bg-background/70 hover:text-foreground disabled:opacity-50"
					>
						<X className="size-3" />
						{t("composer.executionDock.setup.skip")}
					</button>
					<button
						type="button"
						disabled={!onRunRecommendedSetup || isRunningSetup}
						onClick={() => void runRecommendedSetup()}
						className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md bg-amber-500/15 px-2.5 text-[10px] font-medium text-amber-800 transition-colors hover:bg-amber-500/25 disabled:opacity-50 dark:text-amber-200"
					>
						{isRunningSetup ? (
							<LoaderCircle className="size-3 animate-spin" />
						) : (
							<Terminal className="size-3" />
						)}
						{t("composer.executionDock.setup.run")}
					</button>
				</div>
			) : null}
		</div>
	);
});
