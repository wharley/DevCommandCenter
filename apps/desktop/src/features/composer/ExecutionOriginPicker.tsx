import { Check, GitBranch, LoaderCircle, Search } from "lucide-react";
import { useEffect, useMemo, useState, type ReactElement } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { listLocalBranches } from "@/lib/workspace-api";

type ExecutionOriginPickerProps = {
	trigger: ReactElement;
	projectRootPath: string;
	baseBranch: string | null;
	onCreateFromBranch: (branch: string) => Promise<void>;
};

/** Creates a new protected task from a different Git origin. */
export function ExecutionOriginPicker({
	trigger,
	projectRootPath,
	baseBranch,
	onCreateFromBranch,
}: ExecutionOriginPickerProps) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);
	const [branches, setBranches] = useState<string[]>([]);
	const [branchSearch, setBranchSearch] = useState("");
	const [isLoadingBranches, setIsLoadingBranches] = useState(false);
	const [isCreating, setIsCreating] = useState(false);
	const [creatingBranch, setCreatingBranch] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		setBranches([]);
		setBranchSearch("");
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

				{error ? (
					<p className="rounded-md bg-destructive/10 px-2.5 py-2 text-[10.5px] leading-4 text-destructive">
						{error}
					</p>
				) : null}
			</PopoverContent>
		</Popover>
	);
}
