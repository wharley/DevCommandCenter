import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Bot, MessageSquarePlus, Sparkles } from "lucide-react";
import type { ProviderCatalog } from "@dcc/contracts";
import { AgentMascot } from "@/features/providers/agent-mascot";
import { listenMcpRuntimeStatusEvents, loadMcpRuntimeStatuses } from "@/lib/session-api";
import { cn } from "@/lib/utils";
import {
	conversationStartupStage,
	mcpStartupProgress,
	type McpStartupProgress,
} from "./conversation-starting.logic";
import { ElapsedClock } from "./message-components/AssistantActivity";
import { resolveModelLabel } from "./message-components/AssistantMessage";
import "./message-components/assistant-activity.css";

/** Past this, the row says so; the stage label already names what is slow. */
const SLOW_STARTUP_MS = 45_000;

type ConversationStartupStatusProps = {
	sessionId: string | null;
	lastTurnState: string | null;
	isModelRouting?: boolean;
	providers?: ProviderCatalog["providers"];
	providerId?: string | null;
	modelId?: string | null;
	/** When the person pressed send; the clock keeps counting into "Working". */
	since: number;
};

/**
 * The provider's MCP servers as the runtime reports them. Shares its cache
 * with Settings → MCP, which polls the same query.
 */
function useMcpStartupProgress(
	sessionId: string | null,
	providerId: string | null | undefined,
): McpStartupProgress | null {
	const queryClient = useQueryClient();
	const query = useQuery({
		queryKey: ["mcp", "runtime-statuses", sessionId] as const,
		queryFn: () =>
			sessionId ? loadMcpRuntimeStatuses(sessionId) : Promise.resolve({ statuses: [] }),
		enabled: Boolean(sessionId),
		staleTime: 1_000,
	});
	useEffect(() => {
		if (!sessionId) return;
		let disposed = false;
		let unlisten: (() => void) | null = null;
		void listenMcpRuntimeStatusEvents((event) => {
			if (event.sessionId === sessionId) {
				queryClient.setQueryData(["mcp", "runtime-statuses", sessionId], {
					statuses: event.statuses,
				});
			}
		})
			.then((dispose) => {
				if (disposed) void dispose();
				else unlisten = dispose;
			})
			.catch(() => undefined);
		return () => {
			disposed = true;
			if (unlisten) void unlisten();
		};
	}, [queryClient, sessionId]);
	return mcpStartupProgress(query.data?.statuses ?? [], providerId);
}

function useElapsedPast(since: number, thresholdMs: number): boolean {
	const [past, setPast] = useState(() => Date.now() - since >= thresholdMs);
	useEffect(() => {
		const remaining = since + thresholdMs - Date.now();
		if (remaining <= 0) {
			setPast(true);
			return;
		}
		setPast(false);
		const timer = window.setTimeout(() => setPast(true), remaining);
		return () => window.clearTimeout(timer);
	}, [since, thresholdMs]);
	return past;
}

/**
 * Between send and the agent's first activity: the same header and status
 * line the reply will have, so when it arrives the robot, the model and the
 * clock stay where they are and only the label changes.
 */
export function ConversationStartupStatus({
	sessionId,
	lastTurnState,
	isModelRouting = false,
	providers,
	providerId,
	modelId,
	since,
}: ConversationStartupStatusProps) {
	const { t } = useTranslation("common");
	const mcp = useMcpStartupProgress(sessionId, providerId);
	const stage = conversationStartupStage({ isModelRouting, sessionId, lastTurnState, mcp });
	const slow = useElapsedPast(since, SLOW_STARTUP_MS);
	const provider = providers?.find((candidate) => candidate.id === providerId);
	const providerLabel = provider?.label ?? t("conversation.startup.agentFallback");
	// The model is still being chosen while routing; naming it would be a guess.
	const modelLabel = stage === "routing" ? null : resolveModelLabel(modelId, providers);
	const label =
		stage === "connecting" && mcp
			? t("conversation.startup.connecting", { ready: mcp.ready, total: mcp.total })
			: t(`conversation.startup.${stage}`, { provider: providerLabel });

	return (
		<div
			data-message-role="assistant-startup"
			data-startup-stage={stage}
			className="conversation-thread-enter conversation-fade-in flex min-w-0 justify-start motion-reduce:animate-none"
		>
			<div className="relative flex w-full min-w-0 max-w-[52rem] flex-col pb-5">
				{modelLabel ? (
					<div className="mb-2 flex items-center gap-1.5 text-[11px] text-muted-foreground">
						<AgentMascot provider={providerId} model={modelId} state="idle" />
						<span className="sr-only">{t("conversation.modelLabel")}:</span>
						<span className="font-medium text-foreground/80">{modelLabel}</span>
					</div>
				) : null}
				<div className="dcc-turn dcc-turn-live">
					<div className="dcc-turn-status">
						<AgentMascot provider={providerId} model={modelId} state="working" />
						<span role="status" aria-live="polite" className="dcc-turn-status-label">
							{label}
						</span>
						<span aria-hidden className="contents">
							<ElapsedClock since={since} />
						</span>
					</div>
					{slow ? (
						<p className="text-[11.5px] leading-[17px] text-muted-foreground/80">
							{t("conversation.startup.slow")}
						</p>
					) : null}
				</div>
			</div>
		</div>
	);
}

/**
 * The startup row when there is no thread to put it under yet (a running
 * turn whose prompt has not been projected). Same padding as the thread.
 */
export function ConversationExecutionState(props: Omit<ConversationStartupStatusProps, "since">) {
	const mountedAt = useRef(Date.now());
	return (
		<div className="flex min-h-full flex-1 flex-col px-5 pt-6">
			<ConversationStartupStatus {...props} since={mountedAt.current} />
		</div>
	);
}

/** Feedback shown while the header's “new chat” action waits for the
 * provider runtime to attach. No user message exists at this point, so this
 * uses a separate copy from the prompt startup row. */
export function SessionStartingIndicator() {
	const { t } = useTranslation("common");
	const steps = [
		{ id: "creating", icon: MessageSquarePlus },
		{ id: "starting", icon: Bot },
	] as const;

	return (
		<div role="status" aria-live="polite" aria-atomic="true" className="conversation-thread-enter conversation-fade-in flex min-w-0 justify-start motion-reduce:animate-none">
			<div className="w-full max-w-[20rem] rounded-xl border border-border/60 bg-muted/15 px-3.5 py-3 shadow-[0_1px_3px_rgb(0_0_0/0.025)]">
				<div className="mb-2 flex items-center gap-2 text-[12px] font-medium text-foreground/90">
					<Sparkles className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
					<span>{t("conversation.sessionStartup.title")}</span>
				</div>
				<ol className="space-y-0.5">
					{steps.map((step, index) => {
						const active = index === 0;
						const Icon = step.icon;
						return (
							<li key={step.id} aria-current={active ? "step" : undefined} data-startup-state={active ? "active" : "pending"} className="relative flex min-h-8 items-center gap-2.5">
								{index < steps.length - 1 ? <span aria-hidden className="absolute left-[10px] top-[26px] h-3 w-px bg-border/70" /> : null}
								<span aria-hidden className={cn("relative flex size-[21px] shrink-0 items-center justify-center rounded-full", active ? "bg-background text-foreground shadow-[0_0_10px_rgb(128_128_128/0.10)]" : "text-muted-foreground/35")}>
									{active ? <span className="absolute inset-0 rounded-full border border-foreground/10 border-t-foreground/65 motion-safe:animate-[spin_1.8s_linear_infinite]" /> : null}
									<Icon className={active ? "size-[11px]" : "size-3"} strokeWidth={1.7} />
								</span>
								<span className={cn("text-[12px] leading-5", active ? "font-medium text-foreground/90" : "text-muted-foreground/45")}>
									{t(`conversation.sessionStartup.${step.id}`)}
								</span>
							</li>
						);
					})}
				</ol>
				<p className="mt-2 border-t border-border/40 pt-2 text-[11px] leading-[17px] text-muted-foreground">{t("conversation.sessionStartup.description")}</p>
			</div>
		</div>
	);
}
