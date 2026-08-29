import { AlertTriangle, Check, FolderGit2, GitBranch, ShieldCheck, Terminal } from "lucide-react";
import { memo } from "react";
import { useTranslation } from "react-i18next";
import type { WorkspaceSetupReport } from "@dcc/contracts";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { ProjectIdentityGlyph } from "@/features/workspaces/project-identity";

type ExecutionContextRailProps = {
	projectLabel: string | null;
	projectIcon?: string | null;
	projectColor?: string | null;
	baseBranch: string | null;
	currentBranch: string | null;
	isIsolatedWorkspace: boolean;
	contextProjects?: Array<{
		id: string;
		name: string;
		branch: string;
		icon?: string | null;
		color?: string | null;
	}>;
	setupReport?: WorkspaceSetupReport | null;
	onOpenTerminal?: () => void;
};

export const ExecutionContextRail = memo(function ExecutionContextRail({
	projectLabel,
	projectIcon = null,
	projectColor = null,
	baseBranch,
	currentBranch,
	isIsolatedWorkspace,
	contextProjects = [],
	setupReport = null,
	onOpenTerminal,
}: ExecutionContextRailProps) {
	const { t } = useTranslation("common");
	const multiProject = contextProjects.length > 1;
	const displayedProject = projectLabel || t("composer.executionDock.projectFallback");
	const workingBranch = currentBranch && currentBranch !== "HEAD" ? currentBranch : baseBranch;
	const visibleProjects = multiProject
		? contextProjects.slice(0, 3)
		: [{ id: "active", name: displayedProject, branch: baseBranch ?? "", icon: projectIcon, color: projectColor }];
	const setupProblem = setupReport?.steps.find(
		(step) =>
			step.command !== "compile_mission_spec_context" &&
			step.command !== "refresh_repository_forge_metadata" &&
			(step.status === "warning" || step.status === "failed"),
	);
	const hasSetupFailure =
		isIsolatedWorkspace &&
		(Boolean(setupProblem) ||
			(setupReport?.status === "failed" && setupReport.steps.length === 0));

	return (
		<div className="mt-2 flex flex-col gap-2 px-1">
			{hasSetupFailure ? (
				<div className="flex min-w-0 items-center gap-2 rounded-lg border border-amber-500/20 bg-amber-500/[0.06] px-2.5 py-2">
					<AlertTriangle className="size-3.5 shrink-0 text-amber-500" />
					<span className="min-w-0 flex-1">
						<strong className="block text-[10.5px] font-medium">
							{t("composer.executionDock.setup.failed")}
						</strong>
						<small className="block truncate text-[9.5px] text-muted-foreground">
							{setupProblem?.detail ??
								setupReport?.message ??
								setupProblem?.label ??
								t("composer.executionDock.setup.commandFailed")}
						</small>
					</span>
					<button type="button" disabled={!onOpenTerminal} onClick={onOpenTerminal} className="inline-flex h-6 items-center gap-1 rounded-md bg-amber-500/15 px-2 text-[10px] font-medium text-amber-700 hover:bg-amber-500/25 disabled:opacity-50 dark:text-amber-200">
						<Terminal className="size-3" />
						{t("workbench.terminal.open")}
					</button>
				</div>
			) : null}

			<div className="flex min-w-0 items-center justify-between gap-3 px-1 text-[10.5px] text-muted-foreground">
				<Popover>
					<PopoverTrigger asChild>
						<button type="button" className="flex min-w-0 items-center gap-1.5 rounded-md px-1.5 py-1 transition-colors hover:bg-muted/45 hover:text-foreground">
							{isIsolatedWorkspace ? <ShieldCheck className="size-3.5 text-emerald-500" /> : <FolderGit2 className="size-3.5" />}
							<span className="truncate">
								{isIsolatedWorkspace
									? t("newTask.execution.protected")
									: t("newTask.execution.local")}
							</span>
						</button>
					</PopoverTrigger>
					<PopoverContent side="top" align="start" className="w-80 p-2">
						<div className={cn("flex items-start gap-2 rounded-lg p-2", isIsolatedWorkspace && "bg-emerald-500/[0.06]")}>
							<ShieldCheck className="mt-0.5 size-4 shrink-0 text-emerald-500" />
							<span className="min-w-0 flex-1">
								<strong className="flex items-center gap-1.5 text-[11px] font-medium">
									{t("newTask.execution.protected")}
									{isIsolatedWorkspace ? <Check className="size-3" /> : null}
								</strong>
								<small className="text-[10px] text-muted-foreground">{t("newTask.execution.protectedDescription")}</small>
							</span>
						</div>
						<div className={cn("mt-1 flex items-start gap-2 rounded-lg p-2", !isIsolatedWorkspace && "bg-muted/55")}>
							<FolderGit2 className="mt-0.5 size-4 shrink-0" />
							<span className="min-w-0 flex-1">
								<strong className="flex items-center gap-1.5 text-[11px] font-medium">
									{t("newTask.execution.local")}
									{!isIsolatedWorkspace ? <Check className="size-3" /> : null}
								</strong>
								<small className="text-[10px] text-muted-foreground">{t("newTask.execution.localDescription")}</small>
							</span>
						</div>
						<p className="px-2 pb-1 pt-2 text-[9.5px] text-muted-foreground">{t("newTask.execution.fixedAfterCreation")}</p>
					</PopoverContent>
				</Popover>

				<div className="flex min-w-0 items-center gap-3">
					<span className="flex min-w-0 items-center gap-1.5">
						<span className="flex shrink-0 items-center pl-1">
							{visibleProjects.map((project, index) => (
								<ProjectIdentityGlyph key={project.id} icon={project.icon} color={project.color} size="sm" title={project.name} className={cn("size-4", index > 0 && "-ml-1")} />
							))}
						</span>
						<span className="max-w-44 truncate">{multiProject ? t("composer.executionDock.coordinatedProjects", { count: contextProjects.length }) : displayedProject}</span>
					</span>
					{workingBranch ? (
						<span className="flex min-w-0 items-center gap-1.5">
							<GitBranch className="size-3.5 shrink-0" />
							<span className="max-w-36 truncate">{workingBranch}</span>
						</span>
					) : null}
				</div>
			</div>
		</div>
	);
});
