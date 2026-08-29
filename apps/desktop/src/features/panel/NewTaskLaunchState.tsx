import { Boxes, Check, ChevronDown, FolderGit2, LoaderCircle, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Repository, WorkspaceIsolationMode } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { ProjectIdentityGlyph } from "@/features/workspaces/project-identity";
import { repositoryDisplayName } from "@/features/workspaces/repository-display-name";

type NewTaskLaunchStateProps = {
	repositories: Repository[];
	isCreating: boolean;
	onSelectProject: (
		repository: Repository,
		isolationMode: WorkspaceIsolationMode,
	) => Promise<void>;
	onSelectMultiple: () => void;
	onOpenProject: () => void;
};

const EXECUTION_MODES: Array<{
	id: WorkspaceIsolationMode;
	icon: typeof ShieldCheck;
	titleKey: "newTask.execution.protected" | "newTask.execution.local";
	descriptionKey:
		| "newTask.execution.protectedDescription"
		| "newTask.execution.localDescription";
}> = [
	{
		id: "protectedWorktree",
		icon: ShieldCheck,
		titleKey: "newTask.execution.protected",
		descriptionKey: "newTask.execution.protectedDescription",
	},
	{
		id: "localDirect",
		icon: FolderGit2,
		titleKey: "newTask.execution.local",
		descriptionKey: "newTask.execution.localDescription",
	},
];

export function NewTaskLaunchState({
	repositories,
	isCreating,
	onSelectProject,
	onSelectMultiple,
	onOpenProject,
}: NewTaskLaunchStateProps) {
	const { t } = useTranslation("common");
	const [isolationMode, setIsolationMode] =
		useState<WorkspaceIsolationMode>("protectedWorktree");
	const [pendingProjectId, setPendingProjectId] = useState<string | null>(null);

	async function selectProject(repository: Repository) {
		if (isCreating) return;
		setPendingProjectId(repository.id);
		try {
			await onSelectProject(repository, isolationMode);
		} finally {
			setPendingProjectId(null);
		}
	}

	return (
		<div className="flex min-h-0 flex-1 flex-col bg-background">
			<div className="flex min-h-0 flex-1 items-center justify-center px-6 pb-28 pt-20">
				<div className="flex w-full max-w-xl flex-col items-center text-center">
					<div className="mb-5 grid size-11 place-items-center rounded-full border border-border/65 bg-muted/20 text-foreground">
						<span className="text-lg leading-none">›_</span>
					</div>
					<h1 className="text-balance text-[32px] font-medium tracking-[-0.045em] text-foreground">
						{t("newTask.title")}
					</h1>

					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								disabled={isCreating}
								className="mt-1 h-9 gap-1.5 px-3 text-[17px] font-normal text-muted-foreground hover:bg-muted/40 hover:text-foreground"
							>
								{isCreating ? (
									<LoaderCircle className="size-4 animate-spin" />
								) : null}
								{t("newTask.chooseProject")}
								<ChevronDown className="size-4" strokeWidth={1.8} />
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent align="center" className="max-h-[24rem] w-80 overflow-y-auto">
							<DropdownMenuLabel>{t("newTask.projects")}</DropdownMenuLabel>
							{repositories.map((repository) => (
								<DropdownMenuItem
									key={repository.id}
									disabled={isCreating}
									onSelect={() => void selectProject(repository)}
									className="gap-2.5 py-2"
								>
									<ProjectIdentityGlyph
										icon={repository.icon}
										color={repository.color}
										size="sm"
										className="size-6"
									/>
									<span className="min-w-0 flex-1">
										<strong className="block truncate text-[12px] font-medium">
											{repositoryDisplayName(repository)}
										</strong>
										<small className="block truncate text-[10px] text-muted-foreground">
											{repository.baseBranch}
										</small>
									</span>
									{pendingProjectId === repository.id ? (
										<LoaderCircle className="size-3.5 animate-spin" />
									) : null}
								</DropdownMenuItem>
							))}
							<DropdownMenuSeparator />
							<DropdownMenuItem
								disabled={isCreating || repositories.length < 2}
								onSelect={onSelectMultiple}
								className="gap-2.5 py-2"
							>
								<Boxes className="size-4 text-cyan-500" />
								<span>
									<strong className="block text-[12px] font-medium">
										{t("newTask.multipleProjects")}
									</strong>
									<small className="text-[10px] text-muted-foreground">
										{t("newTask.multipleProjectsDescription")}
									</small>
								</span>
							</DropdownMenuItem>
							<DropdownMenuItem onSelect={onOpenProject} className="gap-2.5 py-2">
								<FolderGit2 className="size-4 text-muted-foreground" />
								{t("newTask.openAnotherProject")}
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>
				</div>
			</div>

			<div className="pointer-events-none absolute inset-x-0 bottom-5 flex justify-center px-5">
				<div className="pointer-events-auto flex w-full max-w-[42rem] items-center justify-center gap-1 rounded-xl border border-border/55 bg-sidebar/95 p-1 shadow-[var(--dcc-elevation-1)] backdrop-blur">
					{EXECUTION_MODES.map((mode) => {
						const Icon = mode.icon;
						const selected = isolationMode === mode.id;
						return (
							<button
								type="button"
								key={mode.id}
								disabled={isCreating}
								onClick={() => setIsolationMode(mode.id)}
								className={cn(
									"flex min-w-0 flex-1 items-center gap-2 rounded-lg px-3 py-2 text-left transition-colors",
									selected
										? "bg-accent text-foreground"
										: "text-muted-foreground hover:bg-muted/45 hover:text-foreground",
								)}
							>
								<span className={cn("grid size-7 shrink-0 place-items-center rounded-md", selected && mode.id === "protectedWorktree" ? "bg-emerald-500/12 text-emerald-500" : "bg-muted/55")}>
									<Icon className="size-4" strokeWidth={1.8} />
								</span>
								<span className="min-w-0 flex-1">
									<strong className="flex items-center gap-1.5 truncate text-[11px] font-medium">
										{t(mode.titleKey)}
										{selected ? <Check className="size-3 text-emerald-500" /> : null}
									</strong>
									<small className="block truncate text-[9.5px] text-muted-foreground">
										{t(mode.descriptionKey)}
									</small>
								</span>
							</button>
						);
					})}
				</div>
			</div>
		</div>
	);
}
