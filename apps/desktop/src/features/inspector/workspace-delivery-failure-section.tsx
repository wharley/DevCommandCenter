import {
	AlertTriangle,
	ChevronRight,
	FileWarning,
	GitBranch,
	GitCommitHorizontal,
	Server,
	TerminalSquare,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useWorkspaceDeliveryFailure } from "./use-workspace-delivery-failure";

export function WorkspaceDeliveryFailureSection({
	workspaceRoot,
	branch,
	enabled,
}: {
	workspaceRoot: string | null;
	branch: string | null;
	enabled: boolean;
}) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);
	const query = useWorkspaceDeliveryFailure(workspaceRoot, branch, enabled);
	const failure = query.data?.snapshot ?? null;

	if (!enabled || !failure) return null;

	const pushTarget = failure.pushTarget
		? `${failure.pushTarget.remote}/${failure.pushTarget.branch}`
		: null;
	const shortSha = failure.headSha?.slice(0, 8) ?? null;
	const capturedDate = new Date(failure.createdAt);
	const capturedAt = Number.isNaN(capturedDate.getTime())
		? failure.createdAt
		: capturedDate.toLocaleString();

	return (
		<div className="shrink-0 overflow-hidden rounded-md border border-amber-500/30 bg-amber-500/[0.045]">
			<div className="flex min-w-0 items-center gap-2 px-2.5 py-2">
				<Button
					type="button"
					variant="ghost"
					size="xs"
					className="-ml-1 h-6 min-w-0 flex-1 justify-start gap-2 px-1 text-left hover:bg-transparent"
					onClick={() => setOpen((value) => !value)}
					aria-expanded={open}
				>
					<ChevronRight
						className={cn(
							"size-3.5 shrink-0 transition-transform",
							open && "rotate-90",
						)}
					/>
					<AlertTriangle className="size-3.5 shrink-0 text-amber-600 dark:text-amber-400" />
					<span className="truncate text-[11px] font-medium">
						{t("inspector.deliveryFailure.title")}
					</span>
				</Button>
				<Badge
					variant="outline"
					className={cn(
						"h-5 rounded-full px-1.5 text-[9px] font-medium",
						failure.classification === "unknown"
							? "border-border/70 bg-muted/60 text-muted-foreground"
							: "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-300",
					)}
				>
					{t(
						`inspector.deliveryFailure.classification.${failure.classification}`,
					)}
				</Badge>
			</div>

			{open ? (
				<div className="space-y-2 border-t border-amber-500/15 px-2.5 py-2">
					<p className="text-[9.5px] leading-relaxed text-muted-foreground">
						{t(
							`inspector.deliveryFailure.classificationHint.${failure.classification}`,
						)}
					</p>
					<div className="grid grid-cols-2 gap-1.5">
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<GitBranch className="size-3.5 shrink-0" />
							<span className="truncate">
								{failure.branch ??
									t("inspector.deliveryFailure.branchUnavailable")}
							</span>
						</div>
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<GitCommitHorizontal className="size-3.5 shrink-0" />
							<span className="truncate">
								{shortSha ?? t("inspector.deliveryFailure.commitUnavailable")}
							</span>
						</div>
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<Server className="size-3.5 shrink-0" />
							<span className="truncate">
								{failure.remote ??
									t("inspector.deliveryFailure.remoteUnavailable")}
							</span>
						</div>
						<div className="flex items-center gap-1.5 rounded-md bg-background/60 px-2 py-1.5 text-[9.5px] text-muted-foreground">
							<GitBranch className="size-3.5 shrink-0" />
							<span className="truncate">
								{pushTarget ??
									t("inspector.deliveryFailure.pushTargetUnavailable")}
							</span>
						</div>
					</div>
					<p className="text-[9px] text-muted-foreground">
						{t("inspector.deliveryFailure.capturedContext", {
							operation: t(
								`inspector.deliveryFailure.operation.${failure.operation}`,
							),
							time: capturedAt,
						})}
					</p>

					<div>
						<p className="mb-1 flex items-center gap-1 text-[9px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
							<TerminalSquare className="size-3" />
							{t("inspector.deliveryFailure.output")}
						</p>
						<pre className="max-h-36 overflow-auto whitespace-pre-wrap break-words rounded-md bg-background/70 p-2 font-mono text-[9.5px] leading-[1.45] text-foreground/85">
							{failure.output}
						</pre>
						{failure.outputTruncated ? (
							<p className="mt-1 text-[9px] text-amber-700 dark:text-amber-300">
								{t("inspector.deliveryFailure.outputTruncated")}
							</p>
						) : null}
					</div>

					<div>
						<p className="mb-1 flex items-center gap-1 text-[9px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
							<FileWarning className="size-3" />
							{t("inspector.deliveryFailure.changedFiles", {
								count: failure.changedFiles.length,
							})}
						</p>
						{failure.changedFiles.length > 0 ? (
							<div className="max-h-24 space-y-0.5 overflow-auto rounded-md bg-background/50 px-2 py-1.5">
								{failure.changedFiles.map((path) => (
									<p
										key={path}
										className="truncate font-mono text-[9.5px] text-muted-foreground"
										title={path}
									>
										{path}
									</p>
								))}
							</div>
						) : (
							<p className="text-[9.5px] text-muted-foreground">
								{t("inspector.deliveryFailure.noChangedFiles")}
							</p>
						)}
						{failure.changedFilesTruncated ? (
							<p className="mt-1 text-[9px] text-amber-700 dark:text-amber-300">
								{t("inspector.deliveryFailure.filesTruncated")}
							</p>
						) : null}
					</div>
				</div>
			) : null}
		</div>
	);
}
