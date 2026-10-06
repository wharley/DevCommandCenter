import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { Fragment } from "react";
import { AgentAvatar } from "./agent-avatar";
import { ChroniclerRecapBubble, useRecapBubbleText } from "./chronicler-recap-bubble";
import { isRecapBubbleVisible, useDailyRecap } from "./daily-recap";
import { type ReporterCleanupAlert, ReporterCleanupBubble } from "./reporter-cleanup-bubble";
import type { AgentView } from "./use-agents";

/** Resident agents in the sidebar: one compact row each, above the projects. */
export function AgentsSidebarSection({
	agents,
	activeAgentId,
	collapsed = false,
	onOpenAgent,
	recapBlockedCount = 0,
	cleanup,
}: {
	agents: AgentView[];
	activeAgentId: string | null;
	collapsed?: boolean;
	onOpenAgent: (agentId: string) => void;
	/** Tasks blocked on the person now, for the chronicler's bubble. */
	recapBlockedCount?: number;
	/** Completed tasks over the person's limit, for the reporter to point at. */
	cleanup?: {
		alert: ReporterCleanupAlert;
		onReview: () => void;
		onCleanSafe: () => void;
		onDismiss: () => void;
	} | null;
}) {
	const { t } = useTranslation("common");
	const recap = useDailyRecap();
	const recapText = useRecapBubbleText(
		isRecapBubbleVisible(recap) ? recap.current : null,
		recapBlockedCount,
	);
	if (agents.length === 0) {
		return null;
	}
	const stateLabel = (agent: AgentView) => t(`agents.state.${agent.state}`);

	if (collapsed) {
		return (
			<>
				{agents.map((agent) => (
					<Tooltip key={agent.id}>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								onClick={() => onOpenAgent(agent.id)}
								aria-current={activeAgentId === agent.id ? "page" : undefined}
								aria-label={`${agent.name}: ${stateLabel(agent)}`}
								className={cn(activeAgentId === agent.id && "bg-accent")}
							>
								<AgentAvatar avatar={agent.avatar} state={agent.state} size={20} />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="right">{agent.name}</TooltipContent>
					</Tooltip>
				))}
			</>
		);
	}

	return (
		<section aria-label={t("agents.heading")} className="px-2 pb-3">
			<h2 className="mb-1 px-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/80">
				{t("agents.heading")}
			</h2>
			<div className="space-y-px">
				{agents.map((agent) => {
					const active = activeAgentId === agent.id;
					const isReporter = agent.preset === "chronicler";
					// One thought at a time: the recap first, unless a cleanup is under way.
					const cleanupBubble =
						isReporter && cleanup && (recapText === null || cleanup.alert.progress)
							? cleanup
							: null;
					const recapBubble = isReporter && !cleanupBubble ? recapText : null;
					const speaking = Boolean(cleanupBubble || recapBubble);
					return (
						<Fragment key={agent.id}>
							<button
								type="button"
								onClick={() => onOpenAgent(agent.id)}
								aria-current={active ? "page" : undefined}
								className={cn(
									"flex h-8 w-full cursor-pointer items-center gap-2.5 rounded-lg px-2 text-left transition-colors",
									active
										? "bg-accent text-foreground shadow-sm"
										: "text-foreground hover:bg-accent/50",
								)}
							>
								<AgentAvatar
									avatar={agent.avatar}
									state={agent.state}
									size={22}
									className={speaking ? "dcc-agent-float" : undefined}
								/>
								<span className="min-w-0 flex-1 truncate text-[13px] font-medium">
									{agent.name}
								</span>
								{agent.state !== "idle" && (
									<span
										className={cn(
											"shrink-0 text-[11px]",
											agent.state === "needsYou"
												? "text-amber-700 dark:text-amber-300"
												: agent.state === "done"
													? "text-emerald-700 dark:text-emerald-300"
													: "text-muted-foreground",
										)}
									>
										{stateLabel(agent)}
									</span>
								)}
							</button>
							{recapBubble && (
								<ChroniclerRecapBubble
									name={agent.name}
									text={recapBubble}
									onOpen={() => onOpenAgent(agent.id)}
								/>
							)}
							{cleanupBubble && (
								<ReporterCleanupBubble
									name={agent.name}
									alert={cleanupBubble.alert}
									onReview={cleanupBubble.onReview}
									onCleanSafe={cleanupBubble.onCleanSafe}
									onDismiss={cleanupBubble.onDismiss}
								/>
							)}
						</Fragment>
					);
				})}
			</div>
		</section>
	);
}
