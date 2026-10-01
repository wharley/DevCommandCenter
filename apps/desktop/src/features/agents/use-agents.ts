import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { useQueries, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { dccQueryKeys } from "@/lib/query-client";
import { workspaceSessionsQueryOptions } from "@/features/sessions/workspace-sessions-query";
import {
	type AgentSessionBinding,
	type ResidentAgent,
	agentsOverview,
} from "@/lib/agents-api";
import { agentSessionState, aggregateAgentState } from "./agent-activity";
import type { AgentActivityState } from "./agent-avatar";

export const AGENTS_QUERY_KEY = dccQueryKeys.agents;

export type AgentSessionView = AgentSessionBinding & { state: AgentActivityState };
export type AgentView = ResidentAgent & {
	state: AgentActivityState;
	sessions: AgentSessionView[];
};

/**
 * The person's agents with the live state of every session bound to them.
 * Session state comes from the per-workspace summaries the rail already
 * keeps fresh, so the mascot follows the same events as the task rows.
 */
export function useAgents(scope: string): {
	agents: AgentView[];
	agentBySessionId: Map<string, ResidentAgent>;
	isLoading: boolean;
} {
	const { t } = useTranslation("common");
	const reviewerName = t("agents.presets.reviewer");
	const overview = useQuery({
		queryKey: AGENTS_QUERY_KEY,
		queryFn: () =>
			agentsOverview({
				name: reviewerName,
				kickoffPrompt: t("agents.presets.reviewerKickoff"),
				offerPrompt: t("agents.presets.reviewerOffer"),
			}),
		staleTime: 30_000,
		retry: false,
	});
	const bindings = overview.data?.sessions;
	const workspaceIds = useMemo(
		() => [...new Set((bindings ?? []).map((binding) => binding.workspaceId))].sort(),
		[bindings],
	);
	const summariesByWorkspace = useQueries({
		queries: workspaceIds.map((workspaceId) =>
			workspaceSessionsQueryOptions(workspaceId, { scope }),
		),
		combine: (results) => results.map((result) => result.data),
	});

	return useMemo(() => {
		const summaryBySessionId = new Map<string, WorkspaceSessionSummary>();
		for (const summaries of summariesByWorkspace) {
			for (const summary of summaries ?? []) {
				summaryBySessionId.set(summary.session.id, summary);
			}
		}
		const agentBySessionId = new Map<string, ResidentAgent>();
		const agents = (overview.data?.agents ?? []).map((agent) => {
			const sessions = (bindings ?? [])
				.filter((binding) => binding.agentId === agent.id)
				.map((binding) => {
					agentBySessionId.set(binding.sessionId, agent);
					return {
						...binding,
						state: agentSessionState(summaryBySessionId.get(binding.sessionId)),
					};
				});
			return {
				...agent,
				sessions,
				state: aggregateAgentState(sessions.map((session) => session.state)),
			};
		});
		return { agents, agentBySessionId, isLoading: overview.isLoading };
	}, [bindings, overview.data?.agents, overview.isLoading, summariesByWorkspace]);
}
