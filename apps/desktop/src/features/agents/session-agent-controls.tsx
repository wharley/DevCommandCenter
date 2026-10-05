import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { AgentAvatar } from "./agent-avatar";
import { dispatchCallAgent } from "./call-agent-command";
import { callAgentBlockKey } from "./review-offer";
import { useAgents } from "./use-agents";

/** The mascot of the agent a session runs as, or nothing for a plain session. */
export function SessionAgentBadge({
	sessionId,
	size = 18,
}: {
	sessionId: string | null;
	size?: number;
}) {
	const { agentBySessionId, agents } = useAgents();
	const agent = sessionId ? agentBySessionId.get(sessionId) : undefined;
	if (!agent) {
		return null;
	}
	const state = agents
		.find((candidate) => candidate.id === agent.id)
		?.sessions.find((session) => session.sessionId === sessionId)?.state;
	return (
		<AgentAvatar avatar={agent.avatar} state={state} size={size} label={agent.name} className="shrink-0" />
	);
}

/**
 * One button per agent to call it in the task that is open. The researcher is
 * not one of them: it works only in ideas, started from its own page.
 */
export function CallAgentButtons({
	busy = false,
	hasChanges,
}: {
	/** A turn is running in this task; calling waits until it finishes. */
	busy?: boolean;
	/** Whether the task has anything to review; `undefined` while unknown. */
	hasChanges?: boolean;
}) {
	const { t } = useTranslation("common");
	const { agents } = useAgents();
	return (
		<>
			{agents.filter((agent) => agent.preset !== "researcher").map((agent) => {
				const blockKey = callAgentBlockKey({ preset: agent.preset, busy, hasChanges });
				const blocked = blockKey !== null;
				const label = blockKey ? t(blockKey) : t("agents.call.inTask", { agent: agent.name });
				return (
					<Tooltip key={agent.id}>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								aria-label={label}
								aria-disabled={blocked}
								className={cn(blocked && "cursor-not-allowed opacity-45")}
								onClick={() => !blocked && dispatchCallAgent(agent.id)}
							>
								<AgentAvatar avatar={agent.avatar} size={18} />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">{label}</TooltipContent>
					</Tooltip>
				);
			})}
		</>
	);
}
