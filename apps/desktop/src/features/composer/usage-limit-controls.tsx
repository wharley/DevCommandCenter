import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Hourglass, LoaderCircle, Play } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { workspaceSessionsQueryOptions } from "@/features/sessions/workspace-sessions-query";
import { KeepAwakeForLimitWarning } from "@/features/settings/usage-limit-preference";
import { formatLimitReset, usageLimitStatus } from "@/features/workspaces/usage-limit";
import { useLimitClock } from "@/features/workspaces/usage-limit-state";
import { dccQueryKeys } from "@/lib/query-client";
import {
	readSessionAutoResume,
	resumeAfterUsageLimit,
	saveSessionAutoResume,
	SESSION_AUTO_RESUME_QUERY_KEY,
} from "@/lib/usage-limit-api";
import { cn } from "@/lib/utils";
import { ComposerButton } from "./ComposerButton";

/**
 * Composer toolbar: "Continuar sozinho se o limite acabar" for this task.
 * Before the task has a conversation, the global preference applies.
 */
export function SessionAutoResumeToggle({
	sessionId,
	disabled,
}: {
	sessionId: string | null;
	disabled: boolean;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [saving, setSaving] = useState(false);
	const query = useQuery({
		queryKey: [SESSION_AUTO_RESUME_QUERY_KEY, sessionId],
		queryFn: () => (sessionId ? readSessionAutoResume(sessionId) : false),
		enabled: Boolean(sessionId),
	});
	if (!sessionId) return null;
	const enabled = query.data ?? false;
	const toggle = async () => {
		setSaving(true);
		try {
			await saveSessionAutoResume(sessionId, !enabled);
			queryClient.setQueryData([SESSION_AUTO_RESUME_QUERY_KEY, sessionId], !enabled);
		} catch (cause) {
			toast.error(
				t("usageLimit.toggleFailed", {
					message: cause instanceof Error ? cause.message : String(cause),
				}),
			);
		} finally {
			setSaving(false);
		}
	};
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<ComposerButton
					type="button"
					aria-label={t("usageLimit.toggleLabel")}
					aria-pressed={enabled}
					data-testid="session-auto-resume-toggle"
					disabled={disabled || saving || query.isLoading}
					className={cn(
						"h-7 px-1.5",
						enabled
							? "text-violet-600 hover:text-violet-600 dark:text-violet-300 dark:hover:text-violet-300"
							: "text-muted-foreground/70 hover:text-muted-foreground/70",
					)}
					onClick={() => void toggle()}
				>
					<Hourglass className="size-[13px]" strokeWidth={1.8} />
				</ComposerButton>
			</TooltipTrigger>
			<TooltipContent side="top" className="max-w-64">
				<p className="font-medium">{t("usageLimit.toggleLabel")}</p>
				<p className="mt-0.5 opacity-80">
					{t(enabled ? "usageLimit.toggleOn" : "usageLimit.toggleOff")}
				</p>
			</TooltipContent>
		</Tooltip>
	);
}

/**
 * Above the queue: the task is stopped by its plan's usage limit. Waiting
 * for the reset ("Aguardando limite · volta às 03:40", it continues by
 * itself) or for the person ("Pausado por limite", with "Continuar" /
 * "Tentar de novo"). The queue dispatches after the continuation.
 */
export function UsageLimitStrip({
	sessionId,
	workspaceId,
	queuedCount,
	disabled,
}: {
	sessionId: string | null;
	workspaceId: string | null;
	queuedCount: number;
	disabled: boolean;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [resuming, setResuming] = useState(false);
	const sessions = useQuery(workspaceSessionsQueryOptions(workspaceId));
	const limit =
		sessions.data?.find((summary) => summary.session.id === sessionId)?.usageLimit ?? null;
	const now = useLimitClock(Boolean(limit));
	if (!sessionId || !limit) return null;

	const status = usageLimitStatus(limit, now);
	const time = formatLimitReset(limit.resetsAt, now);
	const resetPassed = limit.resetsAt ? Date.parse(limit.resetsAt) <= now : false;
	const when = resetPassed || !time ? t("usageLimit.whenSoon") : t("usageLimit.whenAt", { time });
	const waiting = status.phase === "waiting";
	const body = waiting
		? limit.autoResume
			? t("usageLimit.waitingAuto", { when })
			: t("usageLimit.waitingManual", { when: t("usageLimit.freesAt", { time: time ?? "" }) })
		: status.pauseReason === "reset_passed"
			? t("usageLimit.reasonResetPassed")
			: status.pauseReason === "attempts_exhausted"
				? t("usageLimit.reasonAttemptsExhausted")
				: t("usageLimit.reasonUnknownReset");
	const actionLabel = waiting
		? t("usageLimit.continueNow")
		: status.pauseReason === "reset_passed"
			? t("usageLimit.continue")
			: t("usageLimit.retry");

	const resume = async () => {
		setResuming(true);
		try {
			await resumeAfterUsageLimit(sessionId);
			if (workspaceId) {
				await queryClient.invalidateQueries({
					queryKey: dccQueryKeys.workspaceSessions(workspaceId, "local"),
				});
			}
		} catch (cause) {
			toast.error(
				t("usageLimit.resumeFailed", {
					message: cause instanceof Error ? cause.message : String(cause),
				}),
			);
		} finally {
			setResuming(false);
		}
	};

	return (
		<div
			className={cn(
				"mb-2 rounded-xl border px-3 py-2 text-[12px]",
				waiting
					? "border-violet-500/30 bg-violet-500/[0.07]"
					: "border-amber-500/35 bg-amber-500/10",
			)}
			data-testid="usage-limit-strip"
			data-usage-limit={status.phase}
			role="status"
		>
			<div className="flex items-start gap-2">
				<Hourglass
					className={cn(
						"mt-0.5 size-3.5 shrink-0",
						waiting ? "text-violet-600 dark:text-violet-300" : "text-amber-600 dark:text-amber-300",
					)}
					aria-hidden
				/>
				<div className="min-w-0 flex-1">
					<p className="font-medium text-foreground">
						{waiting ? t("usageLimit.waitingTitle") : t("usageLimit.pausedTitle")}
						{waiting && time && !resetPassed ? (
							<span className="font-normal text-muted-foreground">
								{" · "}
								{t(limit.autoResume ? "usageLimit.backAt" : "usageLimit.freesAt", { time })}
							</span>
						) : null}
					</p>
					<p className="mt-0.5 text-muted-foreground">
						{body}
						{queuedCount > 0 ? ` ${t("usageLimit.queued", { count: queuedCount })}` : ""}
					</p>
				</div>
				<Button
					type="button"
					size="sm"
					variant={waiting ? "ghost" : "outline"}
					className="h-7 shrink-0 gap-1 px-2 text-[11px]"
					disabled={disabled || resuming}
					onClick={() => void resume()}
				>
					{resuming ? (
						<LoaderCircle className="size-3 animate-spin" />
					) : (
						<Play className="size-3" fill="currentColor" />
					)}
					{actionLabel}
				</Button>
			</div>
			{waiting && limit.autoResume ? (
				<KeepAwakeForLimitWarning
					message={t("usageLimit.keepAwakeWarning")}
					className="mt-2 flex flex-wrap items-center gap-2 text-[11.5px] text-amber-800 dark:text-amber-200"
				/>
			) : null}
		</div>
	);
}
