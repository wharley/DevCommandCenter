import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { useQueries, useQuery } from "@tanstack/react-query";
import { createContext, useContext, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { dccQueryKeys } from "@/lib/query-client";
import { workspaceSessionsQueryOptions } from "@/features/sessions/workspace-sessions-query";
import {
	type AgentSessionBinding,
	type ResidentAgent,
	agentsOverview,
} from "@/lib/agents-api";
import { agentSessionState, aggregateAgentState } from "./agent-activity";
import { isResultUnread, useSeenAgentResults } from "./agent-seen-results";
import type { AgentActivityState } from "./agent-avatar";
import { type PrReviewJob, usePrReviewJobs } from "./pr-review-jobs";

export const AGENTS_QUERY_KEY = dccQueryKeys.agents;
const NO_PR_REVIEWS: PrReviewJob[] = [];

export type AgentSessionView = AgentSessionBinding & {
	state: AgentActivityState;
	/** When its last turn finished, if it did. */
	completedAt: string | null;
	/** It finished and the person has not opened it since. */
	unread: boolean;
};
export type AgentView = ResidentAgent & {
	state: AgentActivityState;
	sessions: AgentSessionView[];
	/** Pull request reviews run from the hub; only the built-in reviewer has them. */
	prReviews: PrReviewJob[];
};

/**
 * The person's agents with the live state of every session bound to them.
 * Session state comes from the per-workspace summaries the rail already
 * keeps fresh, so the mascot follows the same events as the task rows.
 */
export type AgentsState = {
	agents: AgentView[];
	agentBySessionId: Map<string, ResidentAgent>;
	isLoading: boolean;
};

const AgentsContext = createContext<AgentsState>({
	agents: [],
	agentBySessionId: new Map(),
	isLoading: false,
});

/** Shares the app shell's single agents query with every component below it. */
export const AgentsProvider = AgentsContext.Provider;

/** The person's agents and their live state, from the app shell's query. */
export function useAgents(): AgentsState {
	return useContext(AgentsContext);
}

/**
 * Loads the agents and subscribes to the sessions of every task they ran in.
 * Called once, by the app shell; everything else reads `useAgents()`.
 */
export function useAgentsQuery(scope: string): AgentsState {
	const { t } = useTranslation("common");
	const reviewerName = t("agents.presets.reviewer");
	const overview = useQuery({
		queryKey: AGENTS_QUERY_KEY,
		queryFn: () =>
			agentsOverview(
				{
					name: reviewerName,
					kickoffPrompt: t("agents.presets.reviewerKickoff"),
					offerPrompt: t("agents.presets.reviewerOffer"),
				},
				{ name: t("agents.presets.researcher") },
			),
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

	const seen = useSeenAgentResults();
	const prJobs = usePrReviewJobs();
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
					const summary = summaryBySessionId.get(binding.sessionId);
					const state = agentSessionState(summary);
					const completedAt = summary?.lastTurnCompletedAt ?? null;
					return {
						...binding,
						state,
						completedAt,
						unread: state === "done" && isResultUnread(completedAt, seen[binding.sessionId]),
					};
				});
			const prReviews = agent.preset === "reviewer" ? prJobs : NO_PR_REVIEWS;
			return {
				...agent,
				sessions,
				prReviews,
				state: aggregateAgentState([
					...sessions,
					...prReviews.map((job) => ({
						state: job.status === "running" ? ("working" as const) : ("done" as const),
						unread: job.status === "done" && !job.seen,
					})),
				]),
			};
		});
		return { agents, agentBySessionId, isLoading: overview.isLoading };
	}, [bindings, overview.data?.agents, overview.isLoading, prJobs, seen, summariesByWorkspace]);
}
