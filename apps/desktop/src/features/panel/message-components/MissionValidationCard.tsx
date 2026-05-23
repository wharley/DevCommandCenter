import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Check, CircleHelp, Copy } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { saveMissionValidation } from "@/lib/workspace-api";
import { WORKSPACE_MISSION_SPECS_QUERY_KEY } from "@/features/inspector/use-workspace-mission-specs";
import type {
	MissionValidationCheckStatus,
	MissionValidationPersistence,
	MissionValidationStatus,
	ParsedMissionValidationReport,
} from "@/features/spec/mission-spec-content";
import { buildMissionValidationSavePayload } from "@/features/spec/mission-spec-content";

type MissionValidationCardProps = {
	report: ParsedMissionValidationReport;
	workspacePath?: string | null;
	showSaveAction?: boolean;
	isStale?: boolean;
	autoSave?: boolean;
	activeSpecRelativePath?: string | null;
	activeSpecHash?: string | null;
	historyRelativePath?: string | null;
};

function statusIcon(status: MissionValidationStatus) {
	if (status === "PASS") {
		return <Check className="size-3.5 text-emerald-500" strokeWidth={2} />;
	}
	if (status === "FAIL") {
		return <AlertTriangle className="size-3.5 text-destructive" strokeWidth={2} />;
	}
	return <CircleHelp className="size-3.5 text-amber-500" strokeWidth={2} />;
}

function statusClassName(status: MissionValidationStatus) {
	if (status === "PASS") {
		return "border-emerald-500/25 bg-emerald-500/5";
	}
	if (status === "FAIL") {
		return "border-destructive/25 bg-destructive/5";
	}
	return "border-amber-500/25 bg-amber-500/5";
}

function checkStatusClassName(status: MissionValidationCheckStatus) {
	if (status === "RUN") {
		return "border-emerald-500/25 bg-emerald-500/5";
	}
	if (status === "BLOCKED") {
		return "border-destructive/25 bg-destructive/5";
	}
	return "border-slate-500/25 bg-slate-500/5";
}

function persistenceBadgeLabel(mode: MissionValidationPersistence) {
	return mode === "auto" ? "Saved automatically" : "Saved manually";
}

function formatPersistedAt(value: string | null) {
	if (!value) {
		return null;
	}
	const parsed = new Date(value);
	if (Number.isNaN(parsed.getTime())) {
		return value;
	}
	return parsed.toLocaleString();
}

export function MissionValidationCard({
	report,
	workspacePath,
	showSaveAction = true,
	isStale = false,
	autoSave = false,
	activeSpecRelativePath = null,
	activeSpecHash = null,
	historyRelativePath = null,
}: MissionValidationCardProps) {
	const queryClient = useQueryClient();
	const [isSaving, setIsSaving] = useState(false);
	const [autoSaveState, setAutoSaveState] = useState<
		"idle" | "saving" | "saved" | "failed"
	>("idle");
	const [savedMetadata, setSavedMetadata] = useState<{
		mode: MissionValidationPersistence;
		persistedAt: string;
		historyRelativePath: string;
	} | null>(null);
	const attemptedAutoSaveKeyRef = useRef<string | null>(null);
	const passCount = report.criteria.filter((criterion) => criterion.status === "PASS").length;
	const failCount = report.criteria.filter((criterion) => criterion.status === "FAIL").length;
	const unknownCount = report.criteria.filter(
		(criterion) => criterion.status === "UNKNOWN",
	).length;
	const runChecksCount = report.checks.filter((check) => check.status === "RUN").length;
	const skippedChecksCount = report.checks.filter(
		(check) => check.status === "SKIPPED",
	).length;
	const blockedChecksCount = report.checks.filter(
		(check) => check.status === "BLOCKED",
	).length;

	const handleCopyJson = async () => {
		try {
			await navigator.clipboard.writeText(report.rawJson);
			toast.success("Validation JSON copied");
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: "Unable to copy validation JSON.",
			);
		}
	};

	const persistValidation = async (mode: "manual" | "auto") => {
		const root = workspacePath?.trim();
		const specRelativePath = report.specRelativePath?.trim();
		if (!root || !specRelativePath) {
			if (mode === "manual") {
				toast.error("Validation cannot be saved without workspace and spec path.");
			}
			return null;
		}

		setIsSaving(true);
		if (mode === "auto") {
			setAutoSaveState("saving");
		}
		const persistedAt = new Date().toISOString();
		const reportJson =
			buildMissionValidationSavePayload({
				rawJson: report.rawJson,
				mode,
				savedAt: persistedAt,
			}) ?? report.rawJson;
		try {
			const result = await saveMissionValidation({
				workspaceRoot: root,
				specRelativePath,
				reportJson,
			});
			setSavedMetadata({
				mode,
				persistedAt,
				historyRelativePath: result.historyRelativePath,
			});
			if (mode === "manual") {
				toast.success("Validation saved", {
					description: result.relativePath,
				});
			} else {
				setAutoSaveState("saved");
			}
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_MISSION_SPECS_QUERY_KEY, root],
			});
			return result;
		} catch (error) {
			const message =
				error instanceof Error
					? error.message
					: "Unable to save validation report.";
			if (mode === "manual") {
				toast.error(message);
			} else {
				setAutoSaveState("failed");
				toast.error(`Validation auto-save failed: ${message}`);
			}
			return null;
		} finally {
			setIsSaving(false);
		}
	};

	const canAutoSave = Boolean(
		autoSave &&
			workspacePath?.trim() &&
			report.specRelativePath?.trim() &&
			activeSpecRelativePath?.trim() &&
			activeSpecHash?.trim() &&
			report.specRelativePath?.trim() === activeSpecRelativePath?.trim() &&
			report.specHash?.trim() === activeSpecHash?.trim(),
	);

	useEffect(() => {
		if (!canAutoSave) {
			return;
		}
		const autoSaveKey = [
			workspacePath?.trim() ?? "",
			report.specRelativePath?.trim() ?? "",
			report.specHash?.trim() ?? "",
			report.rawJson,
		].join("::");
		if (attemptedAutoSaveKeyRef.current === autoSaveKey) {
			return;
		}
		attemptedAutoSaveKeyRef.current = autoSaveKey;
		void persistValidation("auto");
	}, [
		activeSpecHash,
		activeSpecRelativePath,
		autoSave,
		canAutoSave,
		report.rawJson,
		report.specHash,
		report.specRelativePath,
		workspacePath,
	]);

	const displayedPersistenceMode =
		savedMetadata?.mode ?? report.persistenceMode ?? null;
	const displayedPersistedAt =
		savedMetadata?.persistedAt ?? report.persistedAt ?? null;
	const displayedHistoryRelativePath =
		savedMetadata?.historyRelativePath ?? historyRelativePath ?? null;
	const formattedPersistedAt = formatPersistedAt(displayedPersistedAt);

	return (
		<div className="rounded-[22px] border border-border/70 bg-card/75 p-4 shadow-[0_12px_40px_rgba(0,0,0,0.06)] backdrop-blur-sm">
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div className="min-w-0">
					<div className="flex items-center gap-2">
						<Badge
							variant="secondary"
							className="rounded-md px-2 py-0 text-[10px] font-semibold uppercase tracking-[0.08em]"
						>
							Validation
						</Badge>
						{isStale ? (
							<Badge
								variant="outline"
								className="rounded-md border-amber-500/30 px-2 py-0 text-[10px] font-semibold uppercase tracking-[0.08em] text-amber-600 dark:text-amber-400"
							>
								Stale
							</Badge>
						) : null}
						<p className="truncate text-sm font-medium text-foreground">
							Mission acceptance criteria
						</p>
					</div>
					<p className="mt-1 text-[11px] leading-5 text-muted-foreground">
						{passCount} pass · {failCount} fail · {unknownCount} unknown
					</p>
					{displayedPersistenceMode ? (
						<div className="mt-2 flex flex-wrap items-center gap-1.5">
							<Badge variant="outline" className="h-5 text-[10px]">
								{persistenceBadgeLabel(displayedPersistenceMode)}
							</Badge>
							{formattedPersistedAt ? (
								<span className="text-[11px] leading-5 text-muted-foreground">
									{formattedPersistedAt}
								</span>
							) : null}
						</div>
					) : null}
					{displayedHistoryRelativePath ? (
						<p className="mt-1 font-mono text-[10px] leading-5 text-muted-foreground">
							History: {displayedHistoryRelativePath}
						</p>
					) : null}
				</div>
				<div className="flex items-center gap-1.5">
					{showSaveAction ? (
						<Button
							type="button"
							variant="outline"
							size="sm"
							className="h-7 rounded-lg px-2 text-[11px]"
							disabled={isSaving || !workspacePath || !report.specRelativePath}
							onClick={() => void persistValidation("manual")}
						>
							{isSaving ? "Saving..." : "Save verdict"}
						</Button>
					) : null}
					<Button
						type="button"
						variant="outline"
						size="icon-xs"
						aria-label="Copy validation JSON"
						onClick={() => void handleCopyJson()}
					>
						<Copy className="size-3.5" aria-hidden />
					</Button>
				</div>
			</div>

			{report.summary ? (
				<p className="mt-3 text-[12px] leading-5 text-muted-foreground">
					{report.summary}
				</p>
			) : null}
			{isStale ? (
				<p className="mt-2 rounded-xl border border-amber-500/25 bg-amber-500/5 px-2.5 py-2 text-[11px] leading-5 text-amber-700 dark:text-amber-300">
					This verdict was produced for a different spec hash. Re-run validation
					before relying on it.
				</p>
			) : null}
			{autoSaveState === "saving" ? (
				<p className="mt-2 rounded-xl border border-sky-500/20 bg-sky-500/5 px-2.5 py-2 text-[11px] leading-5 text-sky-700 dark:text-sky-300">
					Auto-saving validation verdict for the active mission spec.
				</p>
			) : null}
			{autoSaveState === "saved" ? (
				<p className="mt-2 rounded-xl border border-emerald-500/20 bg-emerald-500/5 px-2.5 py-2 text-[11px] leading-5 text-emerald-700 dark:text-emerald-300">
					Validation verdict auto-saved for the active mission spec.
				</p>
			) : null}
			{autoSaveState === "failed" ? (
				<p className="mt-2 rounded-xl border border-destructive/20 bg-destructive/5 px-2.5 py-2 text-[11px] leading-5 text-destructive">
					Auto-save failed. Use Save verdict to retry explicitly.
				</p>
			) : null}

			<div className="mt-4 grid gap-1.5">
				{report.criteria.map((criterion) => (
					<div
						key={criterion.id}
						className={cn(
							"rounded-xl border px-2.5 py-2",
							statusClassName(criterion.status),
						)}
					>
						<div className="flex items-center gap-2">
							{statusIcon(criterion.status)}
							<span className="font-mono text-[11px] font-semibold text-foreground">
								{criterion.id}
							</span>
							<span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
								{criterion.status}
							</span>
						</div>
						{criterion.evidence ? (
							<p className="mt-1 pl-5 text-[12px] leading-5 text-foreground">
								{criterion.evidence}
							</p>
						) : null}
						{criterion.nextAction ? (
							<p className="mt-1 pl-5 text-[11px] leading-5 text-muted-foreground">
								Next: {criterion.nextAction}
							</p>
						) : null}
					</div>
				))}
			</div>
			{report.checks.length > 0 ? (
				<div className="mt-4">
					<p className="text-[11px] font-medium text-muted-foreground">
						Checks: {runChecksCount} run · {skippedChecksCount} skipped ·{" "}
						{blockedChecksCount} blocked
					</p>
					<div className="mt-2 grid gap-1.5">
						{report.checks.map((check) => (
							<div
								key={`${check.text}-${check.status}`}
								className={cn(
									"rounded-xl border px-2.5 py-2",
									checkStatusClassName(check.status),
								)}
							>
								<div className="flex items-center gap-2">
									<span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
										{check.status}
									</span>
									<span className="text-[12px] font-medium text-foreground">
										{check.text}
									</span>
								</div>
								{check.evidence ? (
									<p className="mt-1 text-[11px] leading-5 text-muted-foreground">
										{check.evidence}
									</p>
								) : null}
							</div>
						))}
					</div>
				</div>
			) : null}
		</div>
	);
}
