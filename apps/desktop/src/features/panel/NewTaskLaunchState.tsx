import {
	ArrowUpRight,
	Boxes,
	ChevronDown,
	FolderGit2,
	LoaderCircle,
} from "lucide-react";
import { useRef, useState } from "react";
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

export function NewTaskLaunchState({
	repositories,
	isCreating,
	onSelectProject,
	onSelectMultiple,
	onOpenProject,
}: NewTaskLaunchStateProps) {
	const { t } = useTranslation("common");
	// Per-task choice: every new task starts isolated; opting out is one click.
	const [useWorktree, setUseWorktree] = useState(true);
	const isolationMode: WorkspaceIsolationMode = useWorktree
		? "protectedWorktree"
		: "localDirect";
	const [pendingProjectId, setPendingProjectId] = useState<string | null>(null);
	const selectingProject = useRef(false);
	const busy = isCreating || pendingProjectId !== null;

	async function selectProject(repository: Repository) {
		if (isCreating || selectingProject.current) return;
		selectingProject.current = true;
		setPendingProjectId(repository.id);
		try {
			await onSelectProject(repository, isolationMode);
		} finally {
			selectingProject.current = false;
			setPendingProjectId(null);
		}
	}

	return (
		<div className="dcc-task-launch flex min-h-0 flex-1 flex-col bg-background">
			<div className="dcc-task-launch-content flex flex-1 items-center justify-center">
				<div className="flex w-full max-w-xl flex-col items-center text-center">
					<div className="dcc-task-launch-mark mb-5 grid size-11 place-items-center rounded-full border border-border/65 bg-muted/20 text-foreground">
						<img src="/dcc-glyph.svg" alt="" className="size-6" />
					</div>
					<h1 className="text-balance text-[32px] font-medium tracking-[-0.045em] text-foreground">
						{t("newTask.title")}
					</h1>
					<p className="mt-2 max-w-sm text-sm leading-6 text-muted-foreground">
						{t("newTask.designDescription")}
					</p>

					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								disabled={busy}
								className="mt-1 h-9 gap-1.5 px-3 text-[17px] font-normal text-muted-foreground hover:bg-muted/40 hover:text-foreground"
							>
								{isCreating ? (
									<LoaderCircle className="size-4 animate-spin" />
								) : null}
								{t("newTask.chooseProject")}
								<ChevronDown className="size-4" strokeWidth={1.8} />
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent
							align="center"
							className="max-h-[24rem] w-80 overflow-y-auto"
						>
							<DropdownMenuLabel>{t("newTask.projects")}</DropdownMenuLabel>
							{repositories.map((repository) => (
								<DropdownMenuItem
									key={repository.id}
									disabled={busy}
									onSelect={() => void selectProject(repository)}
									className="gap-2.5 py-2"
								>
									<ProjectIdentityGlyph
										icon={repository.icon}
										color={repository.color}
										seed={repository.rootPath}
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
								disabled={busy || repositories.length < 2}
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
							<DropdownMenuItem
								onSelect={onOpenProject}
								className="gap-2.5 py-2"
							>
								<FolderGit2 className="size-4 text-muted-foreground" />
								{t("newTask.openAnotherProject")}
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>
					<label
						className="dcc-task-launch-worktree mt-3 inline-flex cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-[12px] text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground has-[:disabled]:cursor-progress has-[:disabled]:opacity-60"
					>
						<input
							type="checkbox"
							checked={useWorktree}
							disabled={busy}
							onChange={(event) => setUseWorktree(event.target.checked)}
							className="size-3.5 accent-primary"
						/>
						<span className="font-medium text-foreground">
							{t("newTask.execution.worktree")}
						</span>
						<span aria-hidden>·</span>
						<span>
							{t(
								useWorktree
									? "newTask.execution.protectedDescription"
									: "newTask.execution.localDescription",
							)}
						</span>
					</label>
					{repositories.length > 0 ? (
						<div
							className="dcc-project-shortcuts"
							aria-label={t("newTask.projects")}
						>
							{repositories.slice(0, 4).map((repository) => (
								<button
									key={repository.id}
									type="button"
									className="dcc-project-shortcut"
									disabled={busy}
									onClick={() => void selectProject(repository)}
								>
									<ProjectIdentityGlyph
										icon={repository.icon}
										color={repository.color}
										seed={repository.rootPath}
										size="sm"
										className="size-9 shrink-0 rounded-xl"
									/>
									<span className="min-w-0 flex-1">
										<strong className="block truncate text-[13px] font-medium">
											{repositoryDisplayName(repository)}
										</strong>
										<small className="mt-1 block truncate text-[11px] text-muted-foreground">
											{repository.baseBranch}
										</small>
									</span>
									{pendingProjectId === repository.id ? (
										<LoaderCircle className="size-4 shrink-0 animate-spin text-muted-foreground" />
									) : (
										<ArrowUpRight
											className="size-4 shrink-0 text-muted-foreground/60"
											aria-hidden
										/>
									)}
								</button>
							))}
						</div>
					) : null}
				</div>
			</div>
		</div>
	);
}
