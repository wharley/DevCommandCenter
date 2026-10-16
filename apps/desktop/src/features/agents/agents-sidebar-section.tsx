import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { AgentAvatar } from "./agent-avatar";
import { REPORTER_HOME_ATTRIBUTE } from "./reporter-offer";
import { useReporterAway } from "./reporter-store";
import type { AgentView } from "./use-agents";

/** Resident agents in the sidebar: one row of avatars above the task list. */
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

	// One agent speaks for the row: the one that needs the person, else a
	// result not opened yet, else one at work.
	const lead =
		agents.find((agent) => agent.state === "needsYou") ??
		agents.find((agent) => agent.state === "done") ??
		agents.find((agent) => agent.state === "working") ??
		null;

	return (
		<section aria-label={t("agents.heading")} className="flex items-center gap-0.5 px-2.5 pb-3">
			{agents.map((agent) => {
				const active = activeAgentId === agent.id;
				const away = agent.preset === "chronicler" && reporterAway;
				return (
					<Tooltip key={agent.id}>
						<TooltipTrigger asChild>
							<button
								type="button"
								onClick={() => onOpenAgent(agent.id)}
								aria-current={active ? "page" : undefined}
								aria-label={`${agent.name}: ${stateLabel(agent)}`}
								data-agent-state={agent.state}
								className={cn(
									"relative grid size-8 shrink-0 cursor-pointer place-items-center rounded-lg transition-colors",
									active ? "bg-accent shadow-sm" : "hover:bg-accent/50",
								)}
							>
								<span
									{...homeAttribute(agent)}
									className={cn(
										"inline-flex transition-opacity duration-300",
										away && "opacity-25",
									)}
								>
									<AgentAvatar avatar={agent.avatar} state={agent.state} size={22} />
								</span>
								{agent.state === "needsYou" || agent.state === "done" ? (
									<span
										aria-hidden
										className={cn(
											"absolute right-0.5 top-0.5 size-2 rounded-full ring-2 ring-sidebar",
											agent.state === "needsYou" ? "bg-amber-500" : "bg-sky-500",
										)}
									/>
								) : null}
							</button>
						</TooltipTrigger>
						<TooltipContent side="bottom">
							{agent.state === "idle" ? agent.name : `${agent.name} · ${stateLabel(agent)}`}
						</TooltipContent>
					</Tooltip>
				);
			})}
			{lead ? (
				<button
					type="button"
					onClick={() => onOpenAgent(lead.id)}
					className="ml-1 min-w-0 cursor-pointer truncate text-left"
				>
					{/* Sized on the span: a global font reset on button would undo it here. */}
					<span
						className={cn(
							"text-[11px] font-medium",
							lead.state === "needsYou"
								? "text-amber-700 dark:text-amber-300"
								: lead.state === "done"
									? "text-sky-700 dark:text-sky-300"
									: "text-muted-foreground",
						)}
					>
						{lead.name}: {stateLabel(lead).toLocaleLowerCase()}
					</span>
				</button>
			) : null}
		</section>
	);
}
