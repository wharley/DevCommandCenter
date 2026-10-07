import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { AgentAvatar } from "./agent-avatar";
import { REPORTER_HOME_ATTRIBUTE } from "./reporter-offer";
import { useReporterAway } from "./reporter-store";
import type { AgentView } from "./use-agents";

/** Resident agents in the sidebar: one compact row each, above the projects. */
export function AgentsSidebarSection({
	agents,
	activeAgentId,
	collapsed = false,
	onOpenAgent,
}: {
	agents: AgentView[];
	activeAgentId: string | null;
	collapsed?: boolean;
	onOpenAgent: (agentId: string) => void;
}) {
	const { t } = useTranslation("common");
	// The reporter speaks above the composer; while it stands there its row
	// keeps only a faint trace.
	const reporterAway = useReporterAway();
	const homeAttribute = (agent: AgentView) =>
		agent.preset === "chronicler" ? { [REPORTER_HOME_ATTRIBUTE]: "" } : {};
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
								<span
									{...homeAttribute(agent)}
									className={cn(
										"inline-flex transition-opacity duration-300",
										agent.preset === "chronicler" && reporterAway && "opacity-25",
									)}
								>
									<AgentAvatar avatar={agent.avatar} state={agent.state} size={20} />
								</span>
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
					const away = agent.preset === "chronicler" && reporterAway;
					return (
						<button
							key={agent.id}
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
							<span
								{...homeAttribute(agent)}
								className={cn(
									"inline-flex shrink-0 transition-opacity duration-300",
									away && "opacity-25",
								)}
							>
								<AgentAvatar
									avatar={agent.avatar}
									state={agent.state}
									size={22}
								/>
							</span>
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
					);
				})}
			</div>
		</section>
	);
}
