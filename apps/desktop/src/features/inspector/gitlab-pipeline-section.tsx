import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { WorkspacePipelineJob } from "@dcc/contracts";
import {
	Ban,
	CheckCircle2,
	ChevronRight,
	CircleAlert,
	CircleDot,
	Clock3,
	ExternalLink,
	FileTerminal,
	Loader2,
	RotateCcw,
	XCircle,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { openExternal } from "@/lib/shell-api";
import {
	workspacePipelineJobLog,
	workspacePipelineJobRetry,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import {
	useWorkspacePipeline,
	WORKSPACE_PIPELINE_QUERY_KEY,
} from "./use-workspace-pipeline";

type PipelineTone = "success" | "danger" | "progress" | "waiting" | "muted";

function pipelineTone(status: string): PipelineTone {
	switch (status) {
		case "success":
			return "success";
		case "failed":
			return "danger";
		case "running":
		case "pending":
		case "preparing":
		case "canceling":
			return "progress";
		case "manual":
		case "scheduled":
		case "waiting_for_callback":
		case "waiting_for_resource":
			return "waiting";
		default:
			return "muted";
	}
}

function StatusIcon({
	status,
	className,
}: {
	status: string;
	className?: string;
}) {
	const tone = pipelineTone(status);
	if (tone === "success") {
		return <CheckCircle2 className={cn("text-emerald-500", className)} />;
	}
	if (tone === "danger") {
		return <XCircle className={cn("text-destructive", className)} />;
	}
	if (status === "canceled") {
		return <Ban className={cn("text-muted-foreground", className)} />;
	}
	if (tone === "progress") {
		return <Loader2 className={cn("animate-spin text-sky-500", className)} />;
	}
	if (tone === "waiting") {
		return <Clock3 className={cn("text-amber-500", className)} />;
	}
	return <CircleDot className={cn("text-muted-foreground", className)} />;
}

function statusClass(status: string) {
	switch (pipelineTone(status)) {
		case "success":
			return "border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400";
		case "danger":
			return "border-destructive/20 bg-destructive/10 text-destructive";
		case "progress":
			return "border-sky-500/20 bg-sky-500/10 text-sky-700 dark:text-sky-400";
		case "waiting":
			return "border-amber-500/20 bg-amber-500/10 text-amber-700 dark:text-amber-400";
		default:
			return "border-border/60 bg-muted/50 text-muted-foreground";
	}
}

function formatDuration(seconds: number | null) {
	if (seconds == null || !Number.isFinite(seconds)) return null;
	if (seconds < 60) return `${Math.max(0, Math.round(seconds))}s`;
	const minutes = Math.floor(seconds / 60);
	const remainingSeconds = Math.round(seconds % 60);
	if (minutes < 60) return `${minutes}m ${remainingSeconds}s`;
	const hours = Math.floor(minutes / 60);
	return `${hours}h ${minutes % 60}m`;
}

function PipelineJobRow({
	job,
	active,
	busy,
	onToggleLog,
	onRetry,
}: {
	job: WorkspacePipelineJob;
	active: boolean;
	busy: boolean;
	onToggleLog: () => void;
	onRetry: () => void;
}) {
	const { t } = useTranslation("common");
	const duration = formatDuration(job.duration);

	return (
		<div className="border-t border-border/40 first:border-t-0">
			<div className="flex min-w-0 items-center gap-2 px-2.5 py-1.5">
				<StatusIcon status={job.status} className="size-3.5 shrink-0" />
				<div className="min-w-0 flex-1">
					<p className="truncate text-[11px] font-medium">{job.name}</p>
					<p className="truncate text-[9.5px] text-muted-foreground">
						{job.stage}
						{duration ? ` · ${duration}` : ""}
						{job.allowFailure ? ` · ${t("inspector.pipeline.allowedFailure")}` : ""}
					</p>
				</div>
				<Badge
					variant="outline"
					className={cn(
						"h-5 shrink-0 rounded-full px-1.5 text-[9px] font-medium",
						statusClass(job.status),
					)}
				>
					{t(`inspector.pipeline.status.${job.status}`, {
						defaultValue: job.status,
					})}
				</Badge>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							className={cn(
								"size-6 text-muted-foreground",
								active && "bg-muted text-foreground",
							)}
							onClick={onToggleLog}
							aria-label={t("inspector.pipeline.viewLog", { job: job.name })}
						>
							<FileTerminal className="size-3.5" />
						</Button>
					</TooltipTrigger>
					<TooltipContent>{t("inspector.pipeline.log")}</TooltipContent>
				</Tooltip>
				{job.webUrl ? (
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								className="size-6 text-muted-foreground"
								onClick={() => void openExternal(job.webUrl!)}
								aria-label={t("inspector.pipeline.openJob", { job: job.name })}
							>
								<ExternalLink className="size-3.5" />
							</Button>
						</TooltipTrigger>
						<TooltipContent>{t("inspector.pipeline.openJobShort")}</TooltipContent>
					</Tooltip>
				) : null}
				{job.retryable ? (
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								className="size-6 text-muted-foreground hover:text-foreground"
								disabled={busy}
								onClick={onRetry}
								aria-label={t("inspector.pipeline.retryJob", { job: job.name })}
							>
								{busy ? (
									<Loader2 className="size-3.5 animate-spin" />
								) : (
									<RotateCcw className="size-3.5" />
								)}
							</Button>
						</TooltipTrigger>
						<TooltipContent>{t("inspector.pipeline.retry")}</TooltipContent>
					</Tooltip>
				) : null}
			</div>
		</div>
	);
}

export function GitlabPipelineSection({
	workspaceRoot,
	forgeLogin,
	enabled,
}: {
	workspaceRoot: string | null;
	forgeLogin: string | null;
	enabled: boolean;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [open, setOpen] = useState(false);
	const [activeJobId, setActiveJobId] = useState<number | null>(null);
	const [retryingJobId, setRetryingJobId] = useState<number | null>(null);
	const pipelineQuery = useWorkspacePipeline(workspaceRoot, forgeLogin, enabled);
	const pipeline = pipelineQuery.data?.pipeline ?? null;
	const root = workspaceRoot?.trim() ?? "";
	const selectedLogin = forgeLogin?.trim() || null;
	const activeJob = pipeline?.jobs.find((job) => job.id === activeJobId) ?? null;
	const logQuery = useQuery({
		queryKey: [
			WORKSPACE_PIPELINE_QUERY_KEY,
			"log",
			root,
			pipeline?.id ?? null,
			activeJobId,
			selectedLogin,
		],
		queryFn: () =>
			workspacePipelineJobLog({
				workspaceRoot: root,
				pipelineId: pipeline!.id,
				jobId: activeJobId!,
				forgeLogin: selectedLogin,
			}),
		enabled: Boolean(root && pipeline && activeJobId != null),
		staleTime: 30_000,
		retry: false,
	});

	if (!enabled) return null;

	if (pipelineQuery.isPending) {
		return (
			<div className="flex shrink-0 items-center gap-2 rounded-md border border-border/50 bg-muted/20 px-2.5 py-2 text-[11px] text-muted-foreground">
				<Loader2 className="size-3.5 animate-spin" />
				{t("inspector.pipeline.loading")}
			</div>
		);
	}

	if (pipelineQuery.isError) {
		return (
			<div className="flex shrink-0 items-center gap-2 rounded-md border border-amber-500/20 bg-amber-500/5 px-2.5 py-2 text-[11px] text-amber-700 dark:text-amber-300">
				<CircleAlert className="size-3.5 shrink-0" />
				<span className="min-w-0 flex-1 truncate">
					{t("inspector.pipeline.loadFailed")}
				</span>
				<Button
					type="button"
					variant="ghost"
					size="xs"
					className="h-6 px-1.5 text-[10px]"
					onClick={() => void pipelineQuery.refetch()}
				>
					{t("inspector.pipeline.tryAgain")}
				</Button>
			</div>
		);
	}

	if (!pipeline) {
		return (
			<div className="flex shrink-0 items-center gap-2 rounded-md border border-border/50 bg-muted/15 px-2.5 py-2 text-[11px] text-muted-foreground">
				<CircleDot className="size-3.5 shrink-0" />
				<span className="truncate">{t("inspector.pipeline.notFound")}</span>
			</div>
		);
	}

	const duration = formatDuration(pipeline.duration);
	const failedJobs = pipeline.jobs.filter((job) => job.status === "failed").length;

	const handleRetry = async (job: WorkspacePipelineJob) => {
		if (
			!window.confirm(
				t("inspector.pipeline.retryConfirm", {
					job: job.name,
				}),
			)
		) {
			return;
		}
		setRetryingJobId(job.id);
		try {
			await workspacePipelineJobRetry({
				workspaceRoot: root,
				pipelineId: pipeline.id,
				jobId: job.id,
				forgeLogin: selectedLogin,
			});
			setActiveJobId(null);
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_PIPELINE_QUERY_KEY, root],
			});
			toast.success(t("inspector.pipeline.retryStarted", { job: job.name }));
		} catch (error) {
			toast.error(
				(error as Error)?.message ?? t("inspector.pipeline.retryFailed"),
			);
		} finally {
			setRetryingJobId(null);
		}
	};

	return (
		<div className="shrink-0 overflow-hidden rounded-md border border-border/60 bg-muted/15">
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
						className={cn("size-3.5 shrink-0 transition-transform", open && "rotate-90")}
					/>
					<StatusIcon status={pipeline.status} className="size-3.5 shrink-0" />
					<span className="truncate text-[11px] font-medium">
						{t("inspector.pipeline.title", { id: pipeline.id })}
					</span>
					<span className="truncate text-[10px] font-normal text-muted-foreground">
						{pipeline.refName ?? pipeline.sha.slice(0, 8)}
					</span>
				</Button>
				{failedJobs > 0 ? (
					<span className="text-[9.5px] text-destructive">
						{t("inspector.pipeline.failedJobs", { count: failedJobs })}
					</span>
				) : null}
				{duration ? (
					<span className="text-[9.5px] tabular-nums text-muted-foreground">
						{duration}
					</span>
				) : null}
				<Badge
					variant="outline"
					className={cn(
						"h-5 rounded-full px-1.5 text-[9px] font-medium",
						statusClass(pipeline.status),
					)}
				>
					{t(`inspector.pipeline.status.${pipeline.status}`, {
						defaultValue: pipeline.status,
					})}
				</Badge>
				{pipeline.webUrl ? (
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						className="size-6 text-muted-foreground"
						onClick={() => void openExternal(pipeline.webUrl!)}
						aria-label={t("inspector.pipeline.openPipeline")}
					>
						<ExternalLink className="size-3.5" />
					</Button>
				) : null}
			</div>

			{open ? (
				<div className="border-t border-border/40">
					<div className="max-h-56 overflow-y-auto">
						{pipeline.jobs.length > 0 ? (
							pipeline.jobs.map((job) => (
								<div key={job.id}>
									<PipelineJobRow
										job={job}
										active={activeJobId === job.id}
										busy={retryingJobId === job.id}
										onToggleLog={() =>
											setActiveJobId((current) =>
												current === job.id ? null : job.id,
											)
										}
										onRetry={() => void handleRetry(job)}
									/>
									{activeJob?.id === job.id ? (
										<div className="border-t border-border/30 bg-background/60 px-2.5 py-2">
											{logQuery.isPending ? (
												<div className="flex items-center gap-2 py-3 text-[10px] text-muted-foreground">
													<Loader2 className="size-3 animate-spin" />
													{t("inspector.pipeline.loadingLog")}
												</div>
											) : logQuery.isError ? (
												<p className="py-2 text-[10px] text-destructive">
													{(logQuery.error as Error)?.message ??
														t("inspector.pipeline.logFailed")}
												</p>
											) : (
												<>
													{logQuery.data?.truncated ? (
														<p className="mb-1.5 text-[9.5px] text-amber-700 dark:text-amber-400">
															{t("inspector.pipeline.logTruncated")}
														</p>
													) : null}
													<pre className="max-h-44 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted/50 p-2 font-mono text-[9.5px] leading-[1.45] text-foreground/85">
														{logQuery.data?.content ||
															t("inspector.pipeline.emptyLog")}
													</pre>
												</>
											)}
										</div>
									) : null}
								</div>
							))
						) : (
							<p className="px-2.5 py-3 text-[10px] text-muted-foreground">
								{t("inspector.pipeline.noJobs")}
							</p>
						)}
					</div>
				</div>
			) : null}
		</div>
	);
}
