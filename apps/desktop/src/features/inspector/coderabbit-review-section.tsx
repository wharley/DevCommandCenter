import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
	AlertCircle,
	CheckCircle2,
	ChevronRight,
	Clock3,
	LoaderCircle,
	Rabbit,
	RefreshCw,
	Trash2,
} from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type {
	CodeRabbitFinding,
	CodeRabbitFindingSeverity,
	CodeRabbitReviewType,
	WorkspaceGitChangeEntry,
} from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { CodeRabbitConnectDialog } from "@/features/settings/coderabbit-connect-dialog";
import {
	invalidateCodeRabbitCliQueries,
	useCodeRabbitCliStatus,
} from "@/features/settings/coderabbit-cli-queries";
import {
	workspaceCodeRabbitDiffFingerprint,
	workspaceCodeRabbitReview,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";
import type { WorkspaceGitPreviewSelection } from "./workspace-git-file-preview";
import { useStoredCodeRabbitReview } from "./use-workspace-coderabbit-review";

type CodeRabbitReviewSectionProps = {
	workspaceRoot: string;
	staged: WorkspaceGitChangeEntry[];
	unstaged: WorkspaceGitChangeEntry[];
	baseBranch?: string | null;
	onSelectPreview: (selection: WorkspaceGitPreviewSelection) => void;
};

const REVIEW_TYPES: CodeRabbitReviewType[] = ["all", "uncommitted", "committed"];

const SEVERITY_ORDER: CodeRabbitFindingSeverity[] = [
	"critical",
	"major",
	"minor",
	"trivial",
	"info",
	"unknown",
];

const SEVERITY_CLASS: Record<CodeRabbitFindingSeverity, string> = {
	critical: "text-destructive border-destructive/40 bg-destructive/10",
	major: "text-amber-700 border-amber-500/40 bg-amber-500/10 dark:text-amber-300",
	minor: "text-sky-700 border-sky-500/40 bg-sky-500/10 dark:text-sky-300",
	trivial: "text-muted-foreground border-border/60 bg-muted/30",
	info: "text-muted-foreground border-border/60 bg-muted/30",
	unknown: "text-muted-foreground border-border/60 bg-muted/30",
};

function fileName(path: string): string {
	const index = path.lastIndexOf("/");
	return index >= 0 ? path.slice(index + 1) : path;
}

function lineLabel(finding: CodeRabbitFinding): string | null {
	if (!finding.startLine) {
		return null;
	}
	if (!finding.endLine || finding.endLine === finding.startLine) {
		return `L${finding.startLine}`;
	}
	return `L${finding.startLine}-${finding.endLine}`;
}

function findingTitle(finding: CodeRabbitFinding): string {
	return (
		finding.comment?.trim() ||
		finding.codegenInstructions?.trim() ||
		finding.suggestions[0]?.trim() ||
		"CodeRabbit finding"
	);
}

function formatReviewAge(value?: string | null): string | null {
	if (!value) {
		return null;
	}
	const timestamp = Date.parse(value);
	if (Number.isNaN(timestamp)) {
		return null;
	}
	const seconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1000));
	if (seconds < 60) {
		return `${seconds}s`;
	}
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) {
		return `${minutes}m`;
	}
	const hours = Math.floor(minutes / 60);
	if (hours < 48) {
		return `${hours}h`;
	}
	return `${Math.floor(hours / 24)}d`;
}

function buildSelectionForFinding(
	finding: CodeRabbitFinding,
	staged: WorkspaceGitChangeEntry[],
	unstaged: WorkspaceGitChangeEntry[],
	baseBranch?: string | null,
): WorkspaceGitPreviewSelection {
	const stagedEntry = staged.find((entry) => entry.path === finding.path);
	if (stagedEntry) {
		return {
			group: "staged",
			path: finding.path,
			name: stagedEntry.name,
			status: stagedEntry.status,
			baseBranch: null,
		};
	}
	const unstagedEntry = unstaged.find((entry) => entry.path === finding.path);
	if (unstagedEntry) {
		return {
			group: "unstaged",
			path: finding.path,
			name: unstagedEntry.name,
			status: unstagedEntry.status,
			baseBranch: null,
		};
	}
	return {
		group: "committed",
		path: finding.path,
		name: fileName(finding.path),
		status: "M",
		baseBranch: baseBranch ?? null,
	};
}

export function CodeRabbitReviewSection({
	workspaceRoot,
	staged,
	unstaged,
	baseBranch,
	onSelectPreview,
}: CodeRabbitReviewSectionProps) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [open, setOpen] = useState(true);
	const [reviewType, setReviewType] = useState<CodeRabbitReviewType>("all");
	const [connectOpen, setConnectOpen] = useState(false);
	const { review, saveReview, clearReview } = useStoredCodeRabbitReview(workspaceRoot);
	const codeRabbitStatusQuery = useCodeRabbitCliStatus(workspaceRoot, {
		includeAuthStatus: true,
	});
	const codeRabbitReady = Boolean(
		codeRabbitStatusQuery.data?.installed &&
			(codeRabbitStatusQuery.data.auth?.success ||
				codeRabbitStatusQuery.data.auth?.authenticated),
	);
	const codeRabbitMessage =
		codeRabbitStatusQuery.data?.auth?.message ??
		codeRabbitStatusQuery.data?.message ??
		t("inspector.codeRabbit.checking");

	const fingerprintQuery = useQuery({
		queryKey: [
			"workspaceCodeRabbitDiffFingerprint",
			workspaceRoot,
			review?.reviewType ?? reviewType,
			baseBranch ?? null,
			review?.fingerprint?.combinedHash ?? null,
		],
		queryFn: () =>
			workspaceCodeRabbitDiffFingerprint({
				workspaceRoot,
				reviewType: review?.reviewType ?? reviewType,
				base: baseBranch ?? null,
				baseCommit: null,
			}),
		enabled: Boolean(workspaceRoot && review),
		staleTime: 8_000,
		refetchOnWindowFocus: true,
	});

	const isStale = Boolean(
		review &&
			fingerprintQuery.data &&
			fingerprintQuery.data.combinedHash !== review.fingerprint.combinedHash,
	);

	const runReview = useMutation({
		mutationFn: () =>
			workspaceCodeRabbitReview({
				workspaceRoot,
				reviewType,
				base: baseBranch ?? null,
				baseCommit: null,
				light: null,
				configPaths: [],
				cliPath: null,
				timeoutSeconds: null,
			}),
		onSuccess: (next) => {
			saveReview(next);
			if (next.success) {
				toast.success(
					t("inspector.codeRabbit.reviewComplete", {
						count: next.findings.length,
					}),
				);
			} else {
				toast.error(
					next.errors[0] ?? t("inspector.codeRabbit.reviewFailed"),
				);
			}
		},
		onError: (error) => {
			toast.error(
				error instanceof Error ? error.message : t("inspector.codeRabbit.reviewFailed"),
			);
		},
	});

	const groupedFindings = useMemo(() => {
		const grouped = new Map<CodeRabbitFindingSeverity, CodeRabbitFinding[]>();
		for (const severity of SEVERITY_ORDER) {
			grouped.set(severity, []);
		}
		for (const finding of review?.findings ?? []) {
			grouped.get(finding.severity)?.push(finding);
		}
		return SEVERITY_ORDER.map((severity) => ({
			severity,
			findings: grouped.get(severity) ?? [],
		})).filter((group) => group.findings.length > 0);
	}, [review?.findings]);

	const reviewAge = formatReviewAge(review?.completedAt);
	const canRun = !runReview.isPending && Boolean(workspaceRoot) && codeRabbitReady;

	return (
		<div className="border-t border-border/50 bg-background/70 font-sans">
			<div className="flex items-center gap-1 px-2 py-1.5 text-[11.5px]">
				<Button
					type="button"
					variant="ghost"
					size="xs"
					onClick={() => setOpen((value) => !value)}
					aria-expanded={open}
					className="h-auto min-w-0 flex-1 justify-start gap-1 rounded-none px-0 text-left font-semibold text-muted-foreground hover:bg-transparent hover:text-foreground"
				>
					<ChevronRight
						className={cn("size-3 shrink-0 transition-transform", open && "rotate-90")}
						strokeWidth={2}
						aria-hidden
					/>
					<Rabbit className="size-3 shrink-0" strokeWidth={1.8} />
					<span className="truncate">{t("inspector.codeRabbit.reviewTitle")}</span>
				</Button>
				{review ? (
					<Badge variant={isStale ? "outline" : "secondary"} className="h-5 px-1.5 text-[10px] font-normal">
						{isStale
							? t("inspector.codeRabbit.staleBadge")
							: t("inspector.codeRabbit.findingsCount", {
									count: review.findings.length,
								})}
					</Badge>
				) : null}
			</div>
			{open ? (
				<div className="space-y-2 px-3 pb-3">
					<div className="flex flex-wrap items-center justify-between gap-2">
						<ToggleGroup
							type="single"
							value={reviewType}
							onValueChange={(value) => {
								if (REVIEW_TYPES.includes(value as CodeRabbitReviewType)) {
									setReviewType(value as CodeRabbitReviewType);
								}
							}}
							className="h-7 justify-start rounded-md border border-border/50 bg-muted/20 p-0.5"
						>
							{REVIEW_TYPES.map((type) => (
								<ToggleGroupItem
									key={type}
									value={type}
									disabled={runReview.isPending}
									className="h-6 rounded px-2 text-[10.5px] data-[state=on]:bg-background data-[state=on]:text-foreground"
								>
									{t(`inspector.codeRabbit.reviewType.${type}`)}
								</ToggleGroupItem>
							))}
						</ToggleGroup>
						<div className="flex items-center gap-1.5">
							{reviewAge ? (
								<span className="inline-flex items-center gap-1 text-[10.5px] text-muted-foreground">
									<Clock3 className="size-3" />
									{t("inspector.codeRabbit.reviewAge", { age: reviewAge })}
								</span>
							) : null}
							{review ? (
								<Button
									type="button"
									variant="ghost"
									size="icon-xs"
									aria-label={t("inspector.codeRabbit.clear")}
									onClick={clearReview}
								>
									<Trash2 className="size-3.5" />
								</Button>
							) : null}
							<Button
								type="button"
								variant={review ? "outline" : "default"}
								size="xs"
								disabled={!canRun}
								onClick={() => runReview.mutate()}
								className="gap-1.5"
							>
								{runReview.isPending ? (
									<LoaderCircle className="size-3.5 animate-spin" />
								) : (
									<RefreshCw className="size-3.5" />
								)}
								{review
									? t("inspector.codeRabbit.rerun")
									: t("inspector.codeRabbit.run")}
							</Button>
						</div>
					</div>

					{isStale ? (
						<div className="flex items-start gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-700 dark:text-amber-300">
							<AlertCircle className="mt-0.5 size-3.5 shrink-0" />
							<span>{t("inspector.codeRabbit.staleMessage")}</span>
						</div>
					) : null}

					{runReview.isPending ? (
						<div className="flex items-center gap-2 rounded-md border border-border/50 bg-muted/20 px-2 py-2 text-[11px] text-muted-foreground">
							<LoaderCircle className="size-3.5 animate-spin" />
							{t("inspector.codeRabbit.running")}
						</div>
					) : null}

					{!codeRabbitReady && !runReview.isPending ? (
						<div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border/50 bg-muted/20 px-2 py-2 text-[11px] text-muted-foreground">
							<span>{codeRabbitMessage}</span>
							<Button
								type="button"
								variant="outline"
								size="xs"
								onClick={() => setConnectOpen(true)}
							>
								{t("settings.codeRabbit.connect")}
							</Button>
						</div>
					) : null}

					{review?.errors.length ? (
						<div className="rounded-md border border-destructive/30 bg-destructive/10 px-2 py-1.5 text-[11px] text-destructive">
							{review.errors[0]}
						</div>
					) : null}

					{review && review.findings.length === 0 && !review.errors.length ? (
						<div className="flex items-center gap-2 rounded-md border border-border/50 bg-muted/20 px-2 py-2 text-[11px] text-muted-foreground">
							<CheckCircle2 className="size-3.5 text-emerald-600 dark:text-emerald-400" />
							{t("inspector.codeRabbit.emptyFindings")}
						</div>
					) : null}

					{groupedFindings.length > 0 ? (
						<div className="space-y-2">
							{groupedFindings.map((group) => (
								<div key={group.severity} className="space-y-1">
									<div className="flex items-center gap-2 text-[10.5px] font-semibold uppercase tracking-[0.06em] text-muted-foreground">
										<span
											className={cn(
												"size-2 rounded-full border",
												SEVERITY_CLASS[group.severity],
											)}
										/>
										{t(`inspector.codeRabbit.severity.${group.severity}`)}
										<span className="tabular-nums">{group.findings.length}</span>
									</div>
									<div className="space-y-1">
										{group.findings.map((finding) => (
											<button
												key={finding.id}
												type="button"
												className="w-full rounded-md border border-border/45 bg-background/80 px-2 py-1.5 text-left text-[11px] transition-colors hover:bg-accent/60"
												onClick={() =>
													onSelectPreview(
														buildSelectionForFinding(
															finding,
															staged,
															unstaged,
															baseBranch,
														),
													)
												}
											>
												<div className="flex min-w-0 items-center gap-1.5">
													<span className="min-w-0 flex-1 truncate font-mono text-foreground">
														{finding.path}
													</span>
													{lineLabel(finding) ? (
														<span className="shrink-0 rounded bg-muted px-1 py-0.5 font-mono text-[10px] text-muted-foreground">
															{lineLabel(finding)}
														</span>
													) : null}
												</div>
												<p className="mt-1 line-clamp-2 text-[11px] leading-snug text-muted-foreground">
													{findingTitle(finding)}
												</p>
											</button>
										))}
									</div>
								</div>
							))}
						</div>
					) : null}
				</div>
			) : null}
			<CodeRabbitConnectDialog
				open={connectOpen}
				onOpenChange={setConnectOpen}
				workspaceRoot={workspaceRoot}
				onConnected={() => {
					void invalidateCodeRabbitCliQueries(queryClient, workspaceRoot);
				}}
			/>
		</div>
	);
}
