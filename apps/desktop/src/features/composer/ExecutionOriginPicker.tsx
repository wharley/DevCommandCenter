import {
	Check,
	CheckCircle2,
	GitBranch,
	Link2,
	LoaderCircle,
	Search,
} from "lucide-react";
import { useEffect, useMemo, useState, type ReactElement } from "react";
import { useTranslation } from "react-i18next";
import type { WorkspaceSourceUrlResolution } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { listLocalBranches, resolveWorkspaceSourceUrl } from "@/lib/workspace-api";
import { cn } from "@/lib/utils";

type ExecutionOriginPickerProps = {
	trigger: ReactElement;
	projectRootPath: string;
	baseBranch: string | null;
	onCreateFromBranch: (branch: string) => Promise<void>;
	onCreateFromSourceUrl: (url: string) => Promise<void>;
};

/** Creates a new protected task from a different Git origin. */
export function ExecutionOriginPicker({
	trigger,
	projectRootPath,
	baseBranch,
	onCreateFromBranch,
	onCreateFromSourceUrl,
}: ExecutionOriginPickerProps) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);
	const [mode, setMode] = useState<"branch" | "source">("branch");
	const [branches, setBranches] = useState<string[]>([]);
	const [branchSearch, setBranchSearch] = useState("");
	const [isLoadingBranches, setIsLoadingBranches] = useState(false);
	const [sourceUrl, setSourceUrl] = useState("");
	const [validatedSourceUrl, setValidatedSourceUrl] = useState("");
	const [sourceResolution, setSourceResolution] =
		useState<WorkspaceSourceUrlResolution | null>(null);
	const [isResolvingSource, setIsResolvingSource] = useState(false);
	const [isCreating, setIsCreating] = useState(false);
	const [creatingBranch, setCreatingBranch] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		setBranches([]);
		setBranchSearch("");
		setSourceUrl("");
		setValidatedSourceUrl("");
		setSourceResolution(null);
		setError(null);
	}, [projectRootPath]);

	useEffect(() => {
		if (!open || branches.length > 0) return;
		let cancelled = false;
		setIsLoadingBranches(true);
		setError(null);
		void listLocalBranches({ workspaceRoot: projectRootPath })
			.then((result) => {
				if (!cancelled) setBranches(result.branches);
			})
			.catch((reason: unknown) => {
				if (!cancelled) {
					setError(reason instanceof Error ? reason.message : String(reason));
				}
			})
			.finally(() => {
				if (!cancelled) setIsLoadingBranches(false);
			});
		return () => {
			cancelled = true;
		};
	}, [branches.length, open, projectRootPath]);

	const visibleBranches = useMemo(() => {
		const query = branchSearch.trim().toLocaleLowerCase();
		if (!query) return branches;
		return branches.filter((branch) => branch.toLocaleLowerCase().includes(query));
	}, [branchSearch, branches]);

	async function createFromBranch(branch: string) {
		if (branch === baseBranch || isCreating) return;
		setIsCreating(true);
		setCreatingBranch(branch);
		setError(null);
		try {
			await onCreateFromBranch(branch);
			setOpen(false);
		} catch (reason) {
			setError(reason instanceof Error ? reason.message : String(reason));
		} finally {
			setIsCreating(false);
			setCreatingBranch(null);
		}
	}

	async function validateSource() {
		const url = sourceUrl.trim();
		if (!url || isResolvingSource) return;
		setIsResolvingSource(true);
		setSourceResolution(null);
		setValidatedSourceUrl("");
		setError(null);
		try {
			const resolution = await resolveWorkspaceSourceUrl({
				workspaceRoot: projectRootPath,
				url,
				forgeLogin: null,
			});
			setSourceResolution(resolution);
			setValidatedSourceUrl(url);
		} catch (reason) {
			setError(reason instanceof Error ? reason.message : String(reason));
		} finally {
			setIsResolvingSource(false);
		}
	}

	async function createFromSource() {
		const url = sourceUrl.trim();
		if (!sourceResolution || validatedSourceUrl !== url || isCreating) return;
		setIsCreating(true);
		setError(null);
		try {
			await onCreateFromSourceUrl(url);
			setOpen(false);
		} catch (reason) {
			setError(reason instanceof Error ? reason.message : String(reason));
		} finally {
			setIsCreating(false);
		}
	}

	return (
		<Popover
			open={open}
			onOpenChange={(nextOpen) => {
				if (!isCreating) setOpen(nextOpen);
				if (nextOpen) setError(null);
			}}
		>
			<PopoverTrigger asChild>{trigger}</PopoverTrigger>
			<PopoverContent
				side="top"
				align="center"
				sideOffset={8}
				collisionPadding={12}
				className="w-[min(23rem,calc(100vw-1.5rem))] max-w-[calc(100vw-1.5rem)] gap-3 p-3"
			>
				<div>
					<p className="text-[13px] font-medium text-foreground">
						{t("composer.executionDock.origin.title")}
					</p>
					<p className="mt-1 text-[10.5px] leading-4 text-muted-foreground">
						{t("composer.executionDock.origin.description")}
					</p>
				</div>

				<div className="grid grid-cols-2 gap-1 rounded-lg bg-muted/40 p-1">
					<button
						type="button"
						className={cn(
							"flex h-7 items-center justify-center gap-1.5 rounded-md text-[11px] transition-colors",
							mode === "branch"
								? "bg-background text-foreground shadow-sm"
								: "text-muted-foreground hover:text-foreground",
						)}
						onClick={() => setMode("branch")}
					>
						<GitBranch className="size-3.5" />
						{t("composer.executionDock.origin.branchTab")}
					</button>
					<button
						type="button"
						className={cn(
							"flex h-7 items-center justify-center gap-1.5 rounded-md text-[11px] transition-colors",
							mode === "source"
								? "bg-background text-foreground shadow-sm"
								: "text-muted-foreground hover:text-foreground",
						)}
						onClick={() => setMode("source")}
					>
						<Link2 className="size-3.5" />
						{t("composer.executionDock.origin.sourceTab")}
					</button>
				</div>

				{mode === "branch" ? (
					<div className="space-y-2">
						<div className="relative">
							<Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
							<Input
								value={branchSearch}
								onChange={(event) => setBranchSearch(event.target.value)}
								placeholder={t("composer.executionDock.origin.searchBranches")}
								className="h-8 pl-7 text-[11px]"
							/>
						</div>
						<div className="max-h-48 space-y-1 overflow-y-auto rounded-lg border border-border/60 p-1.5">
							{isLoadingBranches ? (
								<div className="flex h-14 items-center justify-center gap-2 text-[11px] text-muted-foreground">
									<LoaderCircle className="size-3.5 animate-spin" />
									{t("composer.executionDock.origin.loadingBranches")}
								</div>
							) : visibleBranches.length === 0 ? (
								<p className="px-2 py-4 text-center text-[11px] text-muted-foreground">
									{t("composer.executionDock.origin.noBranches")}
								</p>
							) : (
								visibleBranches.map((branch) => {
									const current = branch === baseBranch;
									return (
										<button
											type="button"
											key={branch}
											disabled={current || isCreating}
											onClick={() => void createFromBranch(branch)}
											className="flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] text-foreground transition-colors hover:bg-muted/60 disabled:cursor-default disabled:opacity-60"
										>
											<GitBranch className="size-3.5 shrink-0 text-muted-foreground" />
											<span className="min-w-0 flex-1 truncate">{branch}</span>
											{creatingBranch === branch ? (
												<LoaderCircle className="size-3 animate-spin text-emerald-600 dark:text-emerald-400" />
											) : current ? (
												<span className="inline-flex items-center gap-1 text-[9.5px] text-emerald-600 dark:text-emerald-400">
													<Check className="size-3" />
													{t("composer.executionDock.origin.currentBase")}
												</span>
											) : null}
										</button>
									);
								})
							)}
						</div>
					</div>
				) : (
					<div className="space-y-2.5">
						<div className="flex gap-2">
							<Input
								value={sourceUrl}
								onChange={(event) => {
									setSourceUrl(event.target.value);
									setSourceResolution(null);
									setValidatedSourceUrl("");
								}}
								placeholder={t("composer.executionDock.origin.sourcePlaceholder")}
								className="h-8 min-w-0 flex-1 font-mono text-[10.5px]"
							/>
							<Button
								type="button"
								variant="outline"
								size="sm"
								className="h-8 shrink-0 gap-1.5"
								disabled={!sourceUrl.trim() || isResolvingSource || isCreating}
								onClick={() => void validateSource()}
							>
								{isResolvingSource ? (
									<LoaderCircle className="size-3.5 animate-spin" />
								) : (
									<Link2 className="size-3.5" />
								)}
								{t("composer.executionDock.origin.validate")}
							</Button>
						</div>
						{sourceResolution ? (
							<div className="rounded-lg border border-emerald-500/25 bg-emerald-500/[0.055] p-2.5">
								<div className="flex items-center gap-1.5 text-[11px] font-medium text-foreground">
									<CheckCircle2 className="size-3.5 text-emerald-600 dark:text-emerald-400" />
									{sourceResolution.kind === "pull_request"
										? t("composer.executionDock.origin.prValidated", {
												number: sourceResolution.changeRequestNumber ?? "",
											})
										: t("composer.executionDock.origin.branchValidated")}
								</div>
								{sourceResolution.title ? (
									<p className="mt-1 truncate text-[10.5px] text-foreground/85">
										{sourceResolution.title}
									</p>
								) : null}
								<p className="mt-1 truncate font-mono text-[9.5px] text-muted-foreground">
									{sourceResolution.headBranch} → {sourceResolution.baseBranch}
								</p>
								<Button
									type="button"
									size="sm"
									className="mt-2 h-7 w-full gap-1.5"
									disabled={isCreating}
									onClick={() => void createFromSource()}
								>
									{isCreating ? <LoaderCircle className="size-3.5 animate-spin" /> : null}
									{t("composer.executionDock.origin.createTask")}
								</Button>
							</div>
						) : null}
					</div>
				)}

				{error ? (
					<p className="rounded-md bg-destructive/10 px-2.5 py-2 text-[10.5px] leading-4 text-destructive">
						{error}
					</p>
				) : null}
			</PopoverContent>
		</Popover>
	);
}
