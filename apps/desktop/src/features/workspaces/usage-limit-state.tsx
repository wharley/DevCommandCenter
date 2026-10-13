import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { SessionUsageLimit } from "@dcc/contracts";
import { cn } from "@/lib/utils";
import { formatLimitReset, usageLimitStatus } from "./usage-limit";

/** Re-reads the clock every 30 s: the phase changes at the reset. */
export function useLimitClock(active: boolean): number {
	const [now, setNow] = useState(() => Date.now());
	useEffect(() => {
		if (!active) return;
		setNow(Date.now());
		const interval = window.setInterval(() => setNow(Date.now()), 30_000);
		return () => window.clearInterval(interval);
	}, [active]);
	return now;
}

/**
 * The text a usage limit shows: "Aguardando limite · volta às 03:40" while
 * it waits (violet, not the amber of "needs you"), "Pausado por limite"
 * when it waits for the person.
 */
export function usageLimitLabel(
	t: (key: string, options?: Record<string, unknown>) => string,
	limit: SessionUsageLimit,
	now: number,
): { paused: boolean; label: string; detail: string | null } {
	const status = usageLimitStatus(limit, now);
	const time = formatLimitReset(limit.resetsAt, now);
	if (status.phase === "waiting") {
		const due = limit.resetsAt ? Date.parse(limit.resetsAt) <= now : false;
		return {
			paused: false,
			label: t("sidebar.agentState.limitWaiting"),
			detail: due
				? t("usageLimit.resumingSoon")
				: time
					? t(limit.autoResume ? "usageLimit.backAt" : "usageLimit.freesAt", { time })
					: null,
		};
	}
	return {
		paused: true,
		label: t("sidebar.agentState.limitPaused"),
		detail:
			status.pauseReason === "reset_passed"
				? t("usageLimit.reasonResetPassed")
				: status.pauseReason === "attempts_exhausted"
					? t("usageLimit.reasonAttemptsExhausted")
					: t("usageLimit.reasonUnknownReset"),
	};
}

export function WorkspaceUsageLimitState({ limit }: { limit: SessionUsageLimit }) {
	const { t } = useTranslation("common");
	const now = useLimitClock(true);
	const { paused, label, detail } = usageLimitLabel(t, limit, now);
	return (
		<div
			className="flex min-w-0 items-center gap-1.5 overflow-hidden"
			data-usage-limit={paused ? "paused" : "waiting"}
		>
			<span
				aria-hidden
				className={cn(
					"size-[6px] shrink-0 rounded-full",
					paused ? "bg-amber-500" : "bg-violet-500/80",
				)}
			/>
			<span className="flex min-w-0 items-center gap-1 whitespace-nowrap text-[10.5px] font-medium leading-4 text-muted-foreground">
				<span
					className={cn(
						paused
							? "text-amber-700 dark:text-amber-300/90"
							: "text-violet-700 dark:text-violet-300/90",
					)}
				>
					{label}
				</span>
				{detail && !paused ? (
					<>
						<span aria-hidden className="opacity-40">
							·
						</span>
						<span className="truncate">{detail}</span>
					</>
				) : null}
			</span>
		</div>
	);
}
