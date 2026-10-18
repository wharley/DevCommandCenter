import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, LoaderCircle, Play, Target, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { workspaceSessionsQueryOptions } from "@/features/sessions/workspace-sessions-query";
import { dismissSessionGoal } from "@/lib/goal-api";
import { dccQueryKeys } from "@/lib/query-client";
import { cn } from "@/lib/utils";
import {
	formatGoalTokens,
	goalClearPrompt,
	goalResumePrompt,
	presentGoal,
} from "./goal-strip.logic";

/**
 * Above the composer: the goal the provider is pursuing by itself (`/goal`),
 * how far it got and why the latest check said "not yet". Setting a goal is
 * a message; clearing or continuing one is too, sent from here.
 */
export function GoalStrip({
	sessionId,
	workspaceId,
	working,
	disabled,
	onSendPrompt,
}: {
	sessionId: string | null;
	workspaceId: string | null;
	working: boolean;
	disabled: boolean;
	onSendPrompt: (prompt: string) => Promise<boolean>;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [pending, setPending] = useState<"clear" | "resume" | "dismiss" | null>(null);
	const sessions = useQuery(workspaceSessionsQueryOptions(workspaceId));
	const summary = sessions.data?.find((entry) => entry.session.id === sessionId) ?? null;
	const goal = summary?.goal ?? null;
	if (!sessionId || !goal) return null;

	const presented = presentGoal(goal, working);
	const usage = [
		presented.checks > 0 ? t("goal.checks", { count: presented.checks }) : null,
		presented.tokensUsed != null
			? presented.tokenBudget != null
				? t("goal.tokensOfBudget", {
						used: formatGoalTokens(presented.tokensUsed),
						budget: formatGoalTokens(presented.tokenBudget),
					})
				: t("goal.tokens", { used: formatGoalTokens(presented.tokensUsed) })
			: null,
		presented.minutes != null ? t("goal.minutes", { count: presented.minutes }) : null,
	].filter((part): part is string => part !== null);

	const run = async (kind: "clear" | "resume" | "dismiss") => {
		setPending(kind);
		try {
			if (kind === "dismiss") {
				await dismissSessionGoal(sessionId);
				if (workspaceId) {
					await queryClient.invalidateQueries({
						queryKey: dccQueryKeys.workspaceSessions(workspaceId, "local"),
					});
				}
			} else {
				await onSendPrompt(
					kind === "clear"
						? goalClearPrompt(summary?.session.providerId)
						: goalResumePrompt(summary?.session.providerId),
				);
			}
		} catch (cause) {
			toast.error(
				t("goal.actionFailed", {
					message: cause instanceof Error ? cause.message : String(cause),
				}),
			);
		} finally {
			setPending(null);
		}
	};

	const busy = disabled || pending !== null;
	const Icon = presented.tone === "done" ? CheckCircle2 : Target;
	return (
		<div
			className={cn(
				"mb-2 rounded-xl border px-3 py-2 text-[12px]",
				presented.tone === "done"
					? "border-emerald-500/30 bg-emerald-500/[0.07]"
					: presented.tone === "attention"
						? "border-amber-500/35 bg-amber-500/10"
						: "border-sky-500/30 bg-sky-500/[0.06]",
			)}
			data-testid="goal-strip"
			data-goal-status={goal.status}
			role="status"
		>
			<div className="flex items-start gap-2">
				<Icon
					className={cn(
						"mt-0.5 size-3.5 shrink-0",
						presented.tone === "done"
							? "text-emerald-600 dark:text-emerald-300"
							: presented.tone === "attention"
								? "text-amber-600 dark:text-amber-300"
								: "text-sky-600 dark:text-sky-300",
						working && presented.tone === "active" && "animate-pulse",
					)}
					aria-hidden
				/>
				<div className="min-w-0 flex-1">
					<p className="font-medium text-foreground">
						{t(presented.titleKey)}
						{usage.length > 0 ? (
							<span className="font-normal text-muted-foreground">
								{" · "}
								{usage.join(" · ")}
							</span>
						) : null}
					</p>
					<p className="mt-0.5 line-clamp-2 text-muted-foreground" title={goal.objective}>
						{goal.objective}
					</p>
					{presented.lastCheck ? (
						<p
							className="mt-1 line-clamp-2 text-[11.5px] text-muted-foreground/80"
							title={presented.lastCheck}
						>
							{t("goal.lastCheck", { reason: presented.lastCheck })}
						</p>
					) : null}
					{!working && presented.action === "clear" ? (
						<p className="mt-1 text-[11.5px] text-muted-foreground/80">{t("goal.idleHint")}</p>
					) : null}
				</div>
				{!working ? (
					<div className="flex shrink-0 items-center gap-1">
						{presented.action === "resume_and_clear" ? (
							<Button
								type="button"
								size="sm"
								variant="outline"
								className="h-7 gap-1 px-2 text-[11px]"
								disabled={busy}
								onClick={() => void run("resume")}
							>
								{pending === "resume" ? (
									<LoaderCircle className="size-3 animate-spin" />
								) : (
									<Play className="size-3" fill="currentColor" />
								)}
								{t("goal.resume")}
							</Button>
						) : null}
						{presented.action === "dismiss" ? (
							<Button
								type="button"
								size="sm"
								variant="ghost"
								className="h-7 gap-1 px-2 text-[11px]"
								disabled={busy}
								onClick={() => void run("dismiss")}
							>
								{pending === "dismiss" ? (
									<LoaderCircle className="size-3 animate-spin" />
								) : (
									<X className="size-3" />
								)}
								{t("goal.dismiss")}
							</Button>
						) : (
							<Tooltip>
								<TooltipTrigger asChild>
									<Button
										type="button"
										size="sm"
										variant="ghost"
										className="h-7 gap-1 px-2 text-[11px]"
										disabled={busy}
										onClick={() => void run("clear")}
									>
										{pending === "clear" ? (
											<LoaderCircle className="size-3 animate-spin" />
										) : (
											<X className="size-3" />
										)}
										{t("goal.clear")}
									</Button>
								</TooltipTrigger>
								<TooltipContent side="top">{t("goal.clearHint")}</TooltipContent>
							</Tooltip>
						)}
					</div>
				) : null}
			</div>
		</div>
	);
}
