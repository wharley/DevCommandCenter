import type { WorkspaceCleanupEntry } from "@dcc/contracts";
import { Loader2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { workspaceCleanupScan } from "@/lib/workspace-api";
import {
	type CompletedCleanupRow,
	completedCleanupRows,
	deleteCompletedTasks,
	isPreselectedForCleanup,
	selectedCleanupBytes,
} from "./completed-cleanup";
import type { WorkspaceSummary } from "./types";
import { formatDiskBytes, workspaceDiskUsageIds } from "./workspace-disk-usage";
import { workspaceRailDisplayTitle } from "./workspace-rail-shared";

type ScanState =
	| { status: "loading" }
	| { status: "error" }
	| { status: "ready"; entries: WorkspaceCleanupEntry[]; totalBytes: number };

/**
 * Deletes completed tasks in one go. Each worktree is inspected first; only
 * the ones whose work is committed and published come checked, and the
 * others say what deleting them would lose.
 */
export function CompletedCleanupDialog({
	open,
	onOpenChange,
	workspaces,
	projectLabelOf,
	onDeleteWorkspace,
	onCleaned,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	/** The completed tasks, as the sidebar lists them. */
	workspaces: readonly WorkspaceSummary[];
	projectLabelOf: (workspace: WorkspaceSummary) => string | undefined;
	onDeleteWorkspace: (
		workspaceId: string,
		options?: { deleteRemoteBranch?: boolean },
	) => void | Promise<void>;
	/** After a deletion, with what the completed tasks still take. */
	onCleaned?: (remainingBytes: number) => void;
}) {
	const { t, i18n } = useTranslation("common");
	const [scan, setScan] = useState<ScanState>({ status: "loading" });
	const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
	const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
	const deleting = progress !== null;

	// Scanned once per opening: tasks removed while deleting leave the list
	// without triggering a new pass over every worktree.
	useEffect(() => {
		if (!open) {
			return;
		}
		let cancelled = false;
		setScan({ status: "loading" });
		void workspaceCleanupScan({ workspaceIds: workspaceDiskUsageIds(workspaces) })
			.then((result) => {
				if (cancelled) return;
				setScan({
					status: "ready",
					entries: result.workspaces,
					totalBytes: result.totalBytes,
				});
				setSelected(
					new Set(
						completedCleanupRows(workspaces, result.workspaces)
							.filter(isPreselectedForCleanup)
							.map((row) => row.workspace.id),
					),
				);
			})
			.catch((error) => {
				if (cancelled) return;
				console.warn("[dcc] failed to inspect completed worktrees", error);
				setScan({ status: "error" });
			});
		return () => {
			cancelled = true;
		};
	}, [open]);

	const rows = useMemo(
		() => (scan.status === "ready" ? completedCleanupRows(workspaces, scan.entries) : []),
		[scan, workspaces],
	);
	const selectedRows = rows.filter((row) => selected.has(row.workspace.id));
	const selectedBytes = selectedCleanupBytes(rows, selected);
	const formatBytes = (bytes: number) => formatDiskBytes(bytes, i18n.resolvedLanguage);

	const toggle = (workspaceId: string) => {
		setSelected((current) => {
			const next = new Set(current);
			if (!next.delete(workspaceId)) next.add(workspaceId);
			return next;
		});
	};

	const handleDelete = async () => {
		setProgress({ done: 0, total: selectedRows.length });
		const { freedBytes, failed } = await deleteCompletedTasks(
			selectedRows,
			(workspaceId) => onDeleteWorkspace(workspaceId, { deleteRemoteBranch: false }),
			({ done, total, deletedId }) => {
				setProgress({ done, total });
				if (deletedId) {
					setSelected((current) => {
						const next = new Set(current);
						next.delete(deletedId);
						return next;
					});
				}
			},
		);
		setProgress(null);
		if (scan.status === "ready") {
			onCleaned?.(Math.max(0, scan.totalBytes - freedBytes));
		}
		if (failed > 0) {
			toast.error(t("sidebar.cleanup.partial", { count: failed }));
			return;
		}
		toast.success(t("sidebar.cleanup.freed", { size: formatBytes(freedBytes) }));
		onOpenChange(false);
	};

	return (
		<Dialog
			open={open}
			onOpenChange={(next) => {
				if (!deleting) onOpenChange(next);
			}}
		>
			<DialogContent showCloseButton={!deleting} className="min-w-0 overflow-hidden sm:max-w-lg">
				<DialogHeader className="min-w-0 pr-7">
					<DialogTitle>{t("sidebar.cleanup.title")}</DialogTitle>
					<DialogDescription className="leading-5">
						{t("sidebar.cleanup.description")}
					</DialogDescription>
				</DialogHeader>

				{scan.status === "loading" ? (
					<p className="flex items-center gap-2 py-6 text-sm text-muted-foreground">
						<Loader2 className="size-3.5 animate-spin" aria-hidden />
						{t("sidebar.cleanup.scanning")}
					</p>
				) : scan.status === "error" ? (
					<p className="py-6 text-sm text-destructive">{t("sidebar.cleanup.scanFailed")}</p>
				) : rows.length === 0 ? (
					<p className="py-6 text-sm text-muted-foreground">{t("sidebar.cleanup.empty")}</p>
				) : (
					<ul className="-mx-1 max-h-[50vh] min-w-0 divide-y divide-border/60 overflow-y-auto rounded-lg border border-border/70">
						{rows.map((row) => (
							<CleanupRow
								key={row.workspace.id}
								row={row}
								project={projectLabelOf(row.workspace)}
								checked={selected.has(row.workspace.id)}
								disabled={deleting}
								size={formatBytes(row.bytes)}
								onToggle={() => toggle(row.workspace.id)}
							/>
						))}
					</ul>
				)}

				<DialogFooter className="min-w-0 items-center sm:justify-between">
					<span className="text-xs tabular-nums text-muted-foreground">
						{scan.status === "ready" && rows.length > 0
							? t("sidebar.cleanup.selected", {
									count: selectedRows.length,
									size: formatBytes(selectedBytes),
								})
							: null}
					</span>
					<span className="flex gap-2">
						<Button
							type="button"
							variant="outline"
							disabled={deleting}
							onClick={() => onOpenChange(false)}
						>
							{t("sidebar.cancel")}
						</Button>
						<Button
							type="button"
							variant="destructive"
							disabled={deleting || selectedRows.length === 0}
							onClick={() => {
								void handleDelete();
							}}
						>
							{progress ? (
								<>
									<Loader2 className="size-3.5 animate-spin" aria-hidden />
									{t("sidebar.cleanup.deleting", progress)}
								</>
							) : (
								t("sidebar.cleanup.confirm", { count: selectedRows.length })
							)}
						</Button>
					</span>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

function CleanupRow({
	row,
	project,
	checked,
	disabled,
	size,
	onToggle,
}: {
	row: CompletedCleanupRow;
	project: string | undefined;
	checked: boolean;
	disabled: boolean;
	size: string;
	onToggle: () => void;
}) {
	const { t } = useTranslation("common");
	const risky = row.safety === "uncommitted" || row.safety === "unpushed";
	const reason =
		row.safety === "uncommitted"
			? t("sidebar.cleanup.safety.uncommitted", { count: row.changedFiles })
			: row.safety === "unpushed"
				? t("sidebar.cleanup.safety.unpushed", {
						count: row.unpushedCommits,
						branch: row.workspace.branch,
					})
				: t(`sidebar.cleanup.safety.${row.safety}`);
	return (
		<li>
			<label className="flex min-w-0 cursor-pointer items-start gap-2.5 px-3 py-2.5 hover:bg-accent/40">
				<input
					type="checkbox"
					checked={checked}
					disabled={disabled}
					onChange={onToggle}
					className="mt-0.5 size-4 shrink-0 accent-primary"
				/>
				<span className="min-w-0 flex-1">
					<span className="block truncate text-sm font-medium text-foreground">
						{workspaceRailDisplayTitle(row.workspace)}
					</span>
					<span
						className={cn(
							"block truncate text-xs",
							risky ? "text-amber-700 dark:text-amber-300" : "text-muted-foreground",
						)}
					>
						{[reason, project].filter(Boolean).join(" · ")}
					</span>
				</span>
				<span className="shrink-0 pt-0.5 text-xs tabular-nums text-muted-foreground">{size}</span>
			</label>
		</li>
	);
}
