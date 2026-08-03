import {
	Check,
	ChevronUp,
	FileDiff,
	FolderGit2,
	GitBranch,
	LoaderCircle,
	ShieldCheck,
	Terminal,
	X,
} from "lucide-react";
import { memo, useEffect, useState } from "react";
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

export type ExecutionDockChangeSummary = {
	files: number;
	additions: number;
	deletions: number;
};

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
	gitStatusState?: "loading" | "ready" | "error";
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
}: ExecutionDockProps) {
	const { t } = useTranslation("common");
	const [isRunningSetup, setIsRunningSetup] = useState(false);
	const [setupError, setSetupError] = useState<string | null>(null);
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
			className="relative z-10 mx-2 -mb-px grid min-h-12 grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)_auto] overflow-visible rounded-t-xl border border-border/65 border-b-border/40 bg-card/90 shadow-[0_-8px_24px_-22px_rgba(0,0,0,0.45)] backdrop-blur-sm max-sm:grid-cols-[minmax(0,1fr)_auto] dark:shadow-[0_-8px_24px_-22px_rgba(0,0,0,0.9)]"
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

			<button
				type="button"
				onClick={onReviewChanges}
				disabled={!onReviewChanges}
				className="flex items-center gap-1.5 rounded-tr-xl px-3 py-2 text-[11px] text-muted-foreground/90 transition-colors hover:bg-muted/35 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40 disabled:cursor-default disabled:opacity-70"
				aria-label={t("composer.executionDock.reviewChanges")}
			>
				<FileDiff className="size-3.5 shrink-0" strokeWidth={1.8} />
				{gitStatusState === "loading" ? (
					<span className="whitespace-nowrap max-[440px]:hidden">
						{t("composer.executionDock.readingChanges")}
					</span>
				) : gitStatusState === "error" ? (
					<span className="whitespace-nowrap max-[440px]:hidden">
						{t("composer.executionDock.changesUnavailable")}
					</span>
				) : changeSummary ? (
					<span className="flex items-center gap-1 whitespace-nowrap tabular-nums">
						<strong className="font-medium text-foreground">{changeSummary.files}</strong>
						<span className="max-[440px]:hidden">
							{t("composer.executionDock.changes", {
								count: changeSummary.files,
							})}
						</span>
						{changeSummary.additions > 0 ? (
							<span className="text-emerald-600 dark:text-emerald-400">
								+{changeSummary.additions}
							</span>
						) : null}
						{changeSummary.deletions > 0 ? (
							<span className="text-destructive">−{changeSummary.deletions}</span>
						) : null}
					</span>
				) : (
					<span className="whitespace-nowrap text-foreground/65 max-[440px]:hidden">
						{t("composer.executionDock.noChanges")}
					</span>
				)}
			</button>

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
